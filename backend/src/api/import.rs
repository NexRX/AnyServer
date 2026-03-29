use std::sync::Arc;

use axum::extract::State;
use axum::Json;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::security::ssrf::check_url_not_private;
use crate::types::*;
use crate::AppState;

const IMPORT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// POST /api/import/url — server-side proxy to fetch a remote JSON config,
/// avoiding CORS issues.
pub async fn import_url(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(req): Json<ImportUrlRequest>,
) -> Result<Json<ImportUrlResponse>, AppError> {
    let url = req.url.trim().to_string();

    if url.is_empty() {
        return Err(AppError::BadRequest("URL is required".into()));
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::BadRequest(
            "URL must start with http:// or https://".into(),
        ));
    }

    check_url_not_private(&url).map_err(AppError::BadRequest)?;

    let response = state
        .http_client
        .get(&url)
        .timeout(IMPORT_TIMEOUT)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to fetch URL '{}': {}", url, e)))?;

    if !response.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "Remote server returned HTTP {}: {}",
            response.status().as_u16(),
            response.status().canonical_reason().unwrap_or("Unknown"),
        )));
    }

    let content_length = response.content_length().unwrap_or(0);
    if content_length > 1_048_576 {
        return Err(AppError::BadRequest(
            "Remote file is too large (>1MB). Server configs should be small JSON files.".into(),
        ));
    }

    let body = response
        .text()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read response body: {}", e)))?;

    if body.len() > 1_048_576 {
        return Err(AppError::BadRequest(
            "Remote file is too large (>1MB).".into(),
        ));
    }

    let config: ServerConfig = serde_json::from_str(&body).map_err(|e| {
        AppError::BadRequest(format!(
            "Failed to parse remote JSON as a ServerConfig: {}. \
             Make sure the URL points to a valid AnyServer configuration JSON file.",
            e
        ))
    })?;

    if config.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Imported config is missing a 'name' field".into(),
        ));
    }
    if config.binary.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Imported config is missing a 'binary' field".into(),
        ));
    }

    tracing::info!("Imported config '{}' from URL: {}", config.name, url);

    Ok(Json(ImportUrlResponse { config }))
}

/// POST /api/import/folder — list JSON config files from a remote GitHub folder.
/// Supports both GitHub API URLs and web URLs (auto-converted).
pub async fn import_folder(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(req): Json<ImportFolderRequest>,
) -> Result<Json<ImportFolderResponse>, AppError> {
    let url = req.url.trim().to_string();

    if url.is_empty() {
        return Err(AppError::BadRequest("URL is required".into()));
    }

    let api_url = convert_github_url_to_api(&url).unwrap_or_else(|| url.clone());

    if !api_url.starts_with("http://") && !api_url.starts_with("https://") {
        return Err(AppError::BadRequest(
            "URL must start with http:// or https://".into(),
        ));
    }

    check_url_not_private(&api_url).map_err(AppError::BadRequest)?;

    let response = state
        .http_client
        .get(&api_url)
        .timeout(IMPORT_TIMEOUT)
        .send()
        .await
        .map_err(|e| {
            AppError::BadRequest(format!("Failed to fetch folder URL '{}': {}", api_url, e))
        })?;

    if !response.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "Remote server returned HTTP {} for folder listing. \
             Make sure the URL points to a GitHub repository directory. \
             Try: https://api.github.com/repos/OWNER/REPO/contents/PATH",
            response.status().as_u16(),
        )));
    }

    let body = response
        .text()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read folder response: {}", e)))?;

    let entries: Vec<GithubContentEntry> = serde_json::from_str(&body).map_err(|e| {
        AppError::BadRequest(format!(
            "Failed to parse folder listing as GitHub API response: {}. \
             Expected a JSON array of file objects.",
            e
        ))
    })?;

    let configs: Vec<RemoteConfigEntry> = entries
        .into_iter()
        .filter(|entry| {
            entry.entry_type == "file"
                && entry.name.ends_with(".json")
                && entry.download_url.is_some()
        })
        .map(|entry| RemoteConfigEntry {
            name: entry.name,
            download_url: entry.download_url.unwrap_or_default(),
        })
        .collect();

    if configs.is_empty() {
        return Err(AppError::BadRequest(
            "No .json files found in the remote folder. \
             Make sure the directory contains AnyServer configuration JSON files."
                .into(),
        ));
    }

    tracing::info!(
        "Found {} config file(s) in remote folder: {}",
        configs.len(),
        url
    );

    Ok(Json(ImportFolderResponse { configs }))
}

#[derive(Debug, serde::Deserialize)]
struct GithubContentEntry {
    name: String,
    #[serde(rename = "type")]
    entry_type: String,
    download_url: Option<String>,
}

/// Convert `https://github.com/{owner}/{repo}/tree/{branch}[/{path}]`
/// to the corresponding GitHub Contents API URL. Returns `None` if the
/// URL doesn't match.
fn convert_github_url_to_api(url: &str) -> Option<String> {
    let url = url.trim_end_matches('/');

    let prefix = "https://github.com/";
    if !url.starts_with(prefix) {
        return None;
    }

    let rest = &url[prefix.len()..];
    let parts: Vec<&str> = rest.splitn(5, '/').collect();

    if parts.len() < 4 || parts[2] != "tree" {
        return None;
    }

    let owner = parts[0];
    let repo = parts[1];
    let branch = parts[3];

    if parts.len() >= 5 {
        let path = parts[4];
        Some(format!(
            "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
            owner, repo, path, branch
        ))
    } else {
        Some(format!(
            "https://api.github.com/repos/{}/{}/contents?ref={}",
            owner, repo, branch
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_github_url_with_path() {
        let url = "https://github.com/myuser/myrepo/tree/main/configs/anyserver";
        let result = convert_github_url_to_api(url);
        assert_eq!(
            result,
            Some(
                "https://api.github.com/repos/myuser/myrepo/contents/configs/anyserver?ref=main"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_convert_github_url_root() {
        let url = "https://github.com/myuser/myrepo/tree/main";
        let result = convert_github_url_to_api(url);
        assert_eq!(
            result,
            Some("https://api.github.com/repos/myuser/myrepo/contents?ref=main".to_string())
        );
    }

    #[test]
    fn test_convert_github_url_trailing_slash() {
        let url = "https://github.com/myuser/myrepo/tree/main/configs/";
        let result = convert_github_url_to_api(url);
        assert_eq!(
            result,
            Some(
                "https://api.github.com/repos/myuser/myrepo/contents/configs?ref=main".to_string()
            )
        );
    }

    #[test]
    fn test_non_github_url_returns_none() {
        let url = "https://gitlab.com/myuser/myrepo";
        assert_eq!(convert_github_url_to_api(url), None);
    }

    #[test]
    fn test_github_api_url_returns_none() {
        let url = "https://api.github.com/repos/myuser/myrepo/contents";
        assert_eq!(convert_github_url_to_api(url), None);
    }

    #[test]
    fn test_github_blob_url_returns_none() {
        let url = "https://github.com/myuser/myrepo/blob/main/config.json";
        assert_eq!(convert_github_url_to_api(url), None);
    }
}
