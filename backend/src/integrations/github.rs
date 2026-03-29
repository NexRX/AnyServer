//! GitHub API integration for fetching releases and assets.
//!
//! Supports both authenticated (with token) and unauthenticated requests.
//! The unauthenticated API is limited to 60 requests/hour per IP, which is
//! acceptable for most self-hosted scenarios.

use crate::error::AppError;
use crate::types::system::{GithubAsset, GithubRelease, GithubReleaseTag};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Deserialize;

const GITHUB_API_BASE: &str = "https://api.github.com";

/// Internal structure for GitHub API release response.
#[derive(Debug, Deserialize)]
struct ApiRelease {
    tag_name: String,
    name: Option<String>,
    published_at: String,
    body: Option<String>,
    assets: Vec<ApiAsset>,
}

/// Internal structure for GitHub API asset response.
#[derive(Debug, Deserialize)]
struct ApiAsset {
    id: u64,
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Validates that a GitHub repository string is in the correct "owner/repo" format.
pub fn validate_repo_format(repo: &str) -> Result<(), AppError> {
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() != 2 {
        return Err(AppError::BadRequest(
            "GitHub repository must be in 'owner/repo' format".to_string(),
        ));
    }
    if parts[0].is_empty() || parts[1].is_empty() {
        return Err(AppError::BadRequest(
            "GitHub repository owner and name cannot be empty".to_string(),
        ));
    }
    Ok(())
}

/// Fetches release tags for a GitHub repository.
///
/// Returns a list of releases sorted by published date ascending (oldest first).
/// Uses authenticated requests if a token is provided, otherwise uses the
/// unauthenticated API.
pub async fn fetch_release_tags(
    http_client: &reqwest::Client,
    repo: &str,
    token: Option<&str>,
) -> Result<Vec<GithubReleaseTag>, AppError> {
    validate_repo_format(repo)?;

    let url = format!("{}/repos/{}/releases", GITHUB_API_BASE, repo);

    let mut request = http_client.get(&url).header("User-Agent", "AnyServer/1.0");

    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request.send().await.map_err(|e| {
        AppError::Internal(format!(
            "Failed to fetch GitHub releases for {}: {}",
            repo, e
        ))
    })?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            401 => AppError::BadRequest("Invalid GitHub token".to_string()),
            403 => {
                if error_text.contains("rate limit") {
                    AppError::BadRequest(
                        "GitHub API rate limit exceeded. Configure a GitHub token to increase limits.".to_string(),
                    )
                } else {
                    AppError::BadRequest(format!("GitHub API access denied: {}", error_text))
                }
            }
            404 => AppError::NotFound(format!(
                "GitHub repository '{}' not found or is private",
                repo
            )),
            _ => AppError::Internal(format!(
                "GitHub API returned status {}: {}",
                status, error_text
            )),
        });
    }

    let releases: Vec<ApiRelease> = response.json().await.map_err(|e| {
        AppError::Internal(format!("Failed to parse GitHub releases response: {}", e))
    })?;

    let mut result: Vec<GithubReleaseTag> = releases
        .into_iter()
        .filter_map(|r| {
            let published_at = DateTime::parse_from_rfc3339(&r.published_at)
                .ok()?
                .with_timezone(&Utc);
            Some(GithubReleaseTag {
                name: r.tag_name.clone(),
                title: r.name.unwrap_or_else(|| r.tag_name.clone()),
                published_at,
                body: r.body.unwrap_or_default(),
            })
        })
        .collect();

    // Sort by published_at descending (newest first)
    result.sort_by(|a, b| b.published_at.cmp(&a.published_at));

    Ok(result)
}

/// Fetches detailed release information including assets for a specific tag.
pub async fn fetch_release_by_tag(
    http_client: &reqwest::Client,
    repo: &str,
    tag: &str,
    token: Option<&str>,
) -> Result<GithubRelease, AppError> {
    validate_repo_format(repo)?;

    let url = format!("{}/repos/{}/releases/tags/{}", GITHUB_API_BASE, repo, tag);

    let mut request = http_client.get(&url).header("User-Agent", "AnyServer/1.0");

    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request.send().await.map_err(|e| {
        AppError::Internal(format!(
            "Failed to fetch GitHub release {} for {}: {}",
            tag, repo, e
        ))
    })?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            401 => AppError::BadRequest("Invalid GitHub token".to_string()),
            403 => {
                if error_text.contains("rate limit") {
                    AppError::BadRequest(
                        "GitHub API rate limit exceeded. Configure a GitHub token to increase limits.".to_string(),
                    )
                } else {
                    AppError::BadRequest(format!("GitHub API access denied: {}", error_text))
                }
            }
            404 => AppError::NotFound(format!(
                "Release '{}' not found in repository '{}'",
                tag, repo
            )),
            _ => AppError::Internal(format!(
                "GitHub API returned status {}: {}",
                status, error_text
            )),
        });
    }

    let api_release: ApiRelease = response.json().await.map_err(|e| {
        AppError::Internal(format!("Failed to parse GitHub release response: {}", e))
    })?;

    let published_at = DateTime::parse_from_rfc3339(&api_release.published_at)
        .map_err(|e| AppError::Internal(format!("Invalid published_at timestamp: {}", e)))?
        .with_timezone(&Utc);

    Ok(GithubRelease {
        tag_name: api_release.tag_name,
        name: api_release.name.unwrap_or_default(),
        published_at,
        assets: api_release
            .assets
            .into_iter()
            .map(|a| GithubAsset {
                id: a.id,
                name: a.name,
                browser_download_url: a.browser_download_url,
                size: a.size,
            })
            .collect(),
    })
}

/// Finds an asset in a release that matches the given pattern.
///
/// The pattern can be:
/// - An exact filename (e.g. "server.jar")
/// - A regex pattern wrapped in forward slashes (e.g. "/^TShock-.*-linux-x64.*\\.zip$/")
///
/// Returns the matched asset or an error if no match or multiple matches are found.
pub fn find_asset_by_matcher(
    assets: &[GithubAsset],
    matcher: &str,
) -> Result<GithubAsset, AppError> {
    // Check if it's a regex pattern (wrapped in forward slashes)
    let is_regex = matcher.starts_with('/') && matcher.ends_with('/') && matcher.len() > 2;

    if is_regex {
        let pattern = &matcher[1..matcher.len() - 1];
        let regex = Regex::new(pattern).map_err(|e| {
            AppError::BadRequest(format!("Invalid regex pattern '{}': {}", pattern, e))
        })?;

        let matches: Vec<&GithubAsset> =
            assets.iter().filter(|a| regex.is_match(&a.name)).collect();

        match matches.len() {
            0 => Err(AppError::NotFound(format!(
                "No asset found matching regex pattern: {}",
                matcher
            ))),
            1 => Ok(matches[0].clone()),
            _ => {
                let names: Vec<String> = matches.iter().map(|a| a.name.clone()).collect();
                Err(AppError::BadRequest(format!(
                    "Multiple assets match the pattern '{}': {}. Please use a more specific pattern.",
                    matcher,
                    names.join(", ")
                )))
            }
        }
    } else {
        // Exact filename match
        assets
            .iter()
            .find(|a| a.name == matcher)
            .cloned()
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "Asset '{}' not found in release. Available assets: {}",
                    matcher,
                    assets
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })
    }
}

/// Validates that a GitHub repository is accessible.
///
/// This performs a lightweight HEAD request to check if the repo exists
/// and is accessible with the given credentials (if any).
pub async fn validate_repo_accessible(
    http_client: &reqwest::Client,
    repo: &str,
    token: Option<&str>,
) -> Result<(), AppError> {
    validate_repo_format(repo)?;

    let url = format!("{}/repos/{}", GITHUB_API_BASE, repo);

    let mut request = http_client.head(&url).header("User-Agent", "AnyServer/1.0");

    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request.send().await.map_err(|e| {
        AppError::Internal(format!(
            "Failed to validate GitHub repository {}: {}",
            repo, e
        ))
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 => AppError::BadRequest("Invalid GitHub token".to_string()),
            403 => AppError::BadRequest(
                "GitHub repository is private and requires authentication".to_string(),
            ),
            404 => AppError::NotFound(format!("GitHub repository '{}' not found", repo)),
            _ => AppError::Internal(format!("GitHub API returned status {}", status)),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_repo_format_valid() {
        assert!(validate_repo_format("owner/repo").is_ok());
        assert!(validate_repo_format("my-org/my-repo").is_ok());
        assert!(validate_repo_format("Company_Name/Project.Name").is_ok());
    }

    #[test]
    fn test_validate_repo_format_invalid() {
        assert!(validate_repo_format("invalid").is_err());
        assert!(validate_repo_format("too/many/slashes").is_err());
        assert!(validate_repo_format("/repo").is_err());
        assert!(validate_repo_format("owner/").is_err());
        assert!(validate_repo_format("").is_err());
    }

    #[test]
    fn test_find_asset_exact_match() {
        let assets = vec![
            GithubAsset {
                id: 1,
                name: "server.jar".to_string(),
                browser_download_url: "https://example.com/server.jar".to_string(),
                size: 1024,
            },
            GithubAsset {
                id: 2,
                name: "client.jar".to_string(),
                browser_download_url: "https://example.com/client.jar".to_string(),
                size: 2048,
            },
        ];

        let result = find_asset_by_matcher(&assets, "server.jar");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "server.jar");

        let result = find_asset_by_matcher(&assets, "nonexistent.jar");
        assert!(result.is_err());
    }

    #[test]
    fn test_find_asset_regex_match() {
        let assets = vec![
            GithubAsset {
                id: 1,
                name: "TShock-5.2.0-for-Terraria-1.4.4.9-linux-x64-Release.zip".to_string(),
                browser_download_url: "https://example.com/tshock.zip".to_string(),
                size: 1024,
            },
            GithubAsset {
                id: 2,
                name: "TShock-5.2.0-for-Terraria-1.4.4.9-win-x64-Release.zip".to_string(),
                browser_download_url: "https://example.com/tshock-win.zip".to_string(),
                size: 2048,
            },
        ];

        // Match Linux version
        let result = find_asset_by_matcher(&assets, "/TShock-.*-linux-x64.*\\.zip$/");
        assert!(result.is_ok());
        assert!(result.unwrap().name.contains("linux-x64"));

        // No match
        let result = find_asset_by_matcher(&assets, "/^server.*\\.jar$/");
        assert!(result.is_err());
    }

    #[test]
    fn test_find_asset_regex_multiple_matches() {
        let assets = vec![
            GithubAsset {
                id: 1,
                name: "file1.zip".to_string(),
                browser_download_url: "https://example.com/1.zip".to_string(),
                size: 1024,
            },
            GithubAsset {
                id: 2,
                name: "file2.zip".to_string(),
                browser_download_url: "https://example.com/2.zip".to_string(),
                size: 2048,
            },
        ];

        // Match both files - should error
        let result = find_asset_by_matcher(&assets, "/.*\\.zip$/");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Multiple assets match"));
    }

    #[test]
    fn test_find_asset_regex_invalid_pattern() {
        let assets = vec![];
        let result = find_asset_by_matcher(&assets, "/[invalid(regex/");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid regex pattern"));
    }
}
