//! CurseForge API integration for fetching project files and downloading server packs.
//!
//! All requests require a CurseForge API key sent via the `x-api-key` header.
//! Keys can be generated at https://console.curseforge.com/.

use crate::error::AppError;
use serde::Deserialize;

const CURSEFORGE_API_BASE: &str = "https://api.curseforge.com";

// ─── CurseForge API Response Types (internal) ───

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiPagination {
    #[allow(dead_code)]
    #[serde(default)]
    index: u32,
    #[allow(dead_code)]
    #[serde(default)]
    page_size: u32,
    #[allow(dead_code)]
    #[serde(default)]
    result_count: u32,
    #[allow(dead_code)]
    #[serde(default)]
    total_count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeFile {
    pub id: u32,
    #[allow(dead_code)]
    #[serde(default)]
    pub game_id: u32,
    #[allow(dead_code)]
    #[serde(default)]
    pub mod_id: u32,
    #[serde(default)]
    pub is_available: bool,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub file_name: String,
    #[allow(dead_code)]
    #[serde(default)]
    pub file_date: Option<String>,
    pub download_url: Option<String>,
    pub is_server_pack: Option<bool>,
    pub server_pack_file_id: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct FilesResponse {
    data: Vec<CurseForgeFile>,
    #[allow(dead_code)]
    #[serde(default)]
    pagination: Option<ApiPagination>,
}

#[derive(Debug, Deserialize)]
struct SingleFileResponse {
    data: CurseForgeFile,
}

#[derive(Debug, Deserialize)]
struct DownloadUrlResponse {
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModResponse {
    data: ModData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModData {
    #[allow(dead_code)]
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub name: String,
    pub allow_mod_distribution: Option<bool>,
}

/// The result of resolving a server pack download.
pub struct ResolvedServerPack {
    /// The download URL for the server pack file.
    pub download_url: String,
    /// The filename of the server pack file.
    pub file_name: String,
    /// The display name of the originally selected file (for logging).
    pub display_name: String,
    /// Whether we followed a serverPackFileId redirect.
    pub was_redirected: bool,
    /// The file ID of the resolved server pack (may differ from the input).
    pub resolved_file_id: u32,
}

// ─── Public API ───

/// Validate that a CurseForge project ID is accessible.
///
/// Performs a lightweight check by fetching the mod details.
/// Returns the project name on success.
pub async fn validate_project(
    http_client: &reqwest::Client,
    api_key: &str,
    project_id: u32,
) -> Result<String, AppError> {
    let url = format!("{}/v1/mods/{}", CURSEFORGE_API_BASE, project_id);

    let response = http_client
        .get(&url)
        .header("x-api-key", api_key)
        .header("User-Agent", "AnyServer/1.0")
        .send()
        .await
        .map_err(|e| {
            AppError::Internal(format!(
                "Failed to validate CurseForge project {}: {}",
                project_id, e
            ))
        })?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            401 | 403 => {
                AppError::BadRequest("Invalid or unauthorized CurseForge API key".to_string())
            }
            404 => AppError::NotFound(format!("CurseForge project {} not found", project_id)),
            _ => AppError::Internal(format!(
                "CurseForge API returned status {}: {}",
                status, error_text
            )),
        });
    }

    let mod_resp: ModResponse = response.json().await.map_err(|e| {
        AppError::Internal(format!("Failed to parse CurseForge mod response: {}", e))
    })?;

    Ok(mod_resp.data.name)
}

/// Fetch the list of files for a CurseForge project.
///
/// Returns files sorted by date descending (newest first), filtered to
/// only available files. Fetches up to `limit` files (max 50).
pub async fn fetch_project_files(
    http_client: &reqwest::Client,
    api_key: &str,
    project_id: u32,
    limit: u32,
) -> Result<Vec<CurseForgeFile>, AppError> {
    let page_size = limit.min(50);
    let url = format!(
        "{}/v1/mods/{}/files?pageSize={}&index=0",
        CURSEFORGE_API_BASE, project_id, page_size
    );

    let response = http_client
        .get(&url)
        .header("x-api-key", api_key)
        .header("User-Agent", "AnyServer/1.0")
        .send()
        .await
        .map_err(|e| {
            AppError::Internal(format!(
                "Failed to fetch CurseForge files for project {}: {}",
                project_id, e
            ))
        })?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            401 | 403 => {
                AppError::BadRequest("Invalid or unauthorized CurseForge API key".to_string())
            }
            404 => AppError::NotFound(format!("CurseForge project {} not found", project_id)),
            _ => AppError::Internal(format!(
                "CurseForge API returned status {}: {}",
                status, error_text
            )),
        });
    }

    let files_resp: FilesResponse = response.json().await.map_err(|e| {
        AppError::Internal(format!("Failed to parse CurseForge files response: {}", e))
    })?;

    // Filter to only available files, and exclude server pack entries
    // (we only want the "main" files that users would recognise; server
    // packs are resolved automatically during download).
    let files: Vec<CurseForgeFile> = files_resp
        .data
        .into_iter()
        .filter(|f| f.is_available)
        .filter(|f| !f.is_server_pack.unwrap_or(false))
        .collect();

    Ok(files)
}

/// Fetch a single file's details from CurseForge.
pub async fn fetch_file(
    http_client: &reqwest::Client,
    api_key: &str,
    project_id: u32,
    file_id: u32,
) -> Result<CurseForgeFile, AppError> {
    let url = format!(
        "{}/v1/mods/{}/files/{}",
        CURSEFORGE_API_BASE, project_id, file_id
    );

    let response = http_client
        .get(&url)
        .header("x-api-key", api_key)
        .header("User-Agent", "AnyServer/1.0")
        .send()
        .await
        .map_err(|e| {
            AppError::Internal(format!(
                "Failed to fetch CurseForge file {}/{}: {}",
                project_id, file_id, e
            ))
        })?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            401 | 403 => {
                AppError::BadRequest("Invalid or unauthorized CurseForge API key".to_string())
            }
            404 => AppError::NotFound(format!(
                "CurseForge file {}/{} not found",
                project_id, file_id
            )),
            _ => AppError::Internal(format!(
                "CurseForge API returned status {}: {}",
                status, error_text
            )),
        });
    }

    let file_resp: SingleFileResponse = response.json().await.map_err(|e| {
        AppError::Internal(format!("Failed to parse CurseForge file response: {}", e))
    })?;

    Ok(file_resp.data)
}

/// Fetch the download URL for a specific file via the dedicated endpoint.
///
/// This is a fallback for when the `downloadUrl` field on the file object
/// is null (which happens for some projects that restrict distribution).
pub async fn fetch_download_url(
    http_client: &reqwest::Client,
    api_key: &str,
    project_id: u32,
    file_id: u32,
) -> Result<Option<String>, AppError> {
    let url = format!(
        "{}/v1/mods/{}/files/{}/download-url",
        CURSEFORGE_API_BASE, project_id, file_id
    );

    let response = http_client
        .get(&url)
        .header("x-api-key", api_key)
        .header("User-Agent", "AnyServer/1.0")
        .send()
        .await
        .map_err(|e| {
            AppError::Internal(format!(
                "Failed to fetch CurseForge download URL for {}/{}: {}",
                project_id, file_id, e
            ))
        })?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "CurseForge download URL API returned status {}: {}",
            status, error_text
        )));
    }

    let dl_resp: DownloadUrlResponse = response.json().await.map_err(|e| {
        AppError::Internal(format!(
            "Failed to parse CurseForge download URL response: {}",
            e
        ))
    })?;

    Ok(dl_resp.data)
}

/// Resolve the download URL for the server pack associated with a file.
///
/// This handles the full resolution chain:
/// 1. Fetch the selected file's details.
/// 2. If `isServerPack` is already true → use this file directly.
/// 3. If `isServerPack` is false and `serverPackFileId` is set → fetch
///    that file instead (it's the dedicated server pack).
/// 4. If `isServerPack` is false and `serverPackFileId` is null → error
///    (no server pack available for this file).
/// 5. If `downloadUrl` is null on the resolved file → try the dedicated
///    `/download-url` endpoint.
/// 6. If that also returns null → error (distribution not allowed).
///
/// Returns a `ResolvedServerPack` with the download URL, filename, and
/// metadata about the resolution process.
pub async fn resolve_server_pack_download(
    http_client: &reqwest::Client,
    api_key: &str,
    project_id: u32,
    file_id: u32,
) -> Result<ResolvedServerPack, AppError> {
    // Step 1: Fetch the selected file
    let selected_file = fetch_file(http_client, api_key, project_id, file_id).await?;
    let display_name = selected_file.display_name.clone();

    // Step 2-4: Determine which file to download
    let is_server_pack = selected_file.is_server_pack.unwrap_or(false);

    let (target_file, was_redirected) = if is_server_pack {
        // Already a server pack — use it directly
        (selected_file, false)
    } else if let Some(server_pack_id) = selected_file.server_pack_file_id {
        // Follow the serverPackFileId to get the actual server pack
        let server_pack = fetch_file(http_client, api_key, project_id, server_pack_id).await?;
        (server_pack, true)
    } else {
        // No server pack available
        return Err(AppError::BadRequest(format!(
            "The selected file '{}' (ID {}) does not have an associated server pack. \
             This modpack may not provide a server pack for this version.",
            display_name, file_id
        )));
    };

    let resolved_file_id = target_file.id;
    let file_name = target_file.file_name.clone();

    // Step 5-6: Resolve the download URL
    let download_url = if let Some(url) = target_file.download_url {
        url
    } else {
        // downloadUrl is null — try the dedicated endpoint
        let url = fetch_download_url(http_client, api_key, project_id, resolved_file_id)
            .await?
            .ok_or_else(|| {
                AppError::BadRequest(format!(
                    "This CurseForge project does not allow automated distribution. \
                     The server pack for '{}' cannot be downloaded via the API. \
                     You may need to download it manually from CurseForge.",
                    display_name
                ))
            })?;
        url
    };

    Ok(ResolvedServerPack {
        download_url,
        file_name,
        display_name,
        was_redirected,
        resolved_file_id,
    })
}

/// Check whether a CurseForge project allows mod distribution.
///
/// Returns `Ok(())` if distribution is allowed or unknown, and an error
/// if the project explicitly disallows it.
pub async fn check_distribution_allowed(
    http_client: &reqwest::Client,
    api_key: &str,
    project_id: u32,
) -> Result<(), AppError> {
    let url = format!("{}/v1/mods/{}", CURSEFORGE_API_BASE, project_id);

    let response = http_client
        .get(&url)
        .header("x-api-key", api_key)
        .header("User-Agent", "AnyServer/1.0")
        .send()
        .await
        .map_err(|e| {
            AppError::Internal(format!(
                "Failed to check CurseForge distribution for project {}: {}",
                project_id, e
            ))
        })?;

    if !response.status().is_success() {
        // If we can't check, allow the attempt — the download will fail
        // later if distribution is actually blocked.
        return Ok(());
    }

    let mod_resp: ModResponse = response.json().await.map_err(|e| {
        AppError::Internal(format!("Failed to parse CurseForge mod response: {}", e))
    })?;

    if mod_resp.data.allow_mod_distribution == Some(false) {
        return Err(AppError::BadRequest(format!(
            "CurseForge project '{}' (ID {}) does not allow automated distribution. \
             Server packs must be downloaded manually.",
            mod_resp.data.name, project_id
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_curseforge_file_deserialization() {
        let json = r#"{
            "id": 5678,
            "gameId": 432,
            "modId": 1234,
            "isAvailable": true,
            "displayName": "All the Mods 10 - 1.1.2",
            "fileName": "All the Mods 10-1.1.2.zip",
            "fileDate": "2024-01-15T12:00:00.000Z",
            "downloadUrl": "https://edge.forgecdn.net/files/5678/All the Mods 10-1.1.2.zip",
            "isServerPack": false,
            "serverPackFileId": 5679,
            "releaseType": 1,
            "fileStatus": 4,
            "hashes": [],
            "fileLength": 123456,
            "downloadCount": 1000,
            "gameVersions": ["1.20.1"],
            "sortableGameVersions": [],
            "dependencies": [],
            "fileFingerprint": 0,
            "modules": []
        }"#;

        let file: CurseForgeFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.id, 5678);
        assert_eq!(file.display_name, "All the Mods 10 - 1.1.2");
        assert_eq!(file.is_server_pack, Some(false));
        assert_eq!(file.server_pack_file_id, Some(5679));
        assert!(file.download_url.is_some());
        assert!(file.is_available);
    }

    #[test]
    fn test_curseforge_file_null_download_url() {
        let json = r#"{
            "id": 5678,
            "gameId": 432,
            "modId": 1234,
            "isAvailable": true,
            "displayName": "Test Pack",
            "fileName": "test.zip",
            "fileDate": "2024-01-15T12:00:00.000Z",
            "downloadUrl": null,
            "isServerPack": true,
            "serverPackFileId": null,
            "releaseType": 1,
            "fileStatus": 4,
            "hashes": [],
            "fileLength": 0,
            "downloadCount": 0,
            "gameVersions": [],
            "sortableGameVersions": [],
            "dependencies": [],
            "fileFingerprint": 0,
            "modules": []
        }"#;

        let file: CurseForgeFile = serde_json::from_str(json).unwrap();
        assert!(file.download_url.is_none());
        assert_eq!(file.is_server_pack, Some(true));
        assert!(file.server_pack_file_id.is_none());
    }

    #[test]
    fn test_server_pack_file_is_filtered_from_options() {
        // Simulate the filter logic from fetch_project_files
        let files = [
            CurseForgeFile {
                id: 100,
                game_id: 432,
                mod_id: 1234,
                is_available: true,
                display_name: "Modpack v1.0".to_string(),
                file_name: "modpack-1.0.zip".to_string(),
                file_date: Some("2024-01-01T00:00:00Z".to_string()),
                download_url: Some("https://example.com/100".to_string()),
                is_server_pack: Some(false),
                server_pack_file_id: Some(101),
            },
            CurseForgeFile {
                id: 101,
                game_id: 432,
                mod_id: 1234,
                is_available: true,
                display_name: "Modpack v1.0 Server Pack".to_string(),
                file_name: "modpack-1.0-server.zip".to_string(),
                file_date: Some("2024-01-01T00:00:00Z".to_string()),
                download_url: Some("https://example.com/101".to_string()),
                is_server_pack: Some(true),
                server_pack_file_id: None,
            },
            CurseForgeFile {
                id: 102,
                game_id: 432,
                mod_id: 1234,
                is_available: false,
                display_name: "Modpack v0.9 (unavailable)".to_string(),
                file_name: "modpack-0.9.zip".to_string(),
                file_date: Some("2023-12-01T00:00:00Z".to_string()),
                download_url: None,
                is_server_pack: Some(false),
                server_pack_file_id: None,
            },
        ];

        let filtered: Vec<&CurseForgeFile> = files
            .iter()
            .filter(|f| f.is_available)
            .filter(|f| !f.is_server_pack.unwrap_or(false))
            .collect();

        // Should only include the main available file, not the server pack
        // entry or the unavailable file.
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, 100);
        assert_eq!(filtered[0].display_name, "Modpack v1.0");
    }
}
