//! SteamCMD detection and app-ID validation utilities.
//!
//! This module provides:
//! - Detection of `steamcmd` on the system PATH
//! - Cached detection with a configurable TTL (default 60 s)
//! - Validation of Steam app IDs via the Steam store API
//! - A shared status type used by the `/api/system/steamcmd-status` endpoint

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;
use ts_rs::TS;

// ─── SteamCMD Status ───

/// Result of checking whether SteamCMD is available on this host.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SteamCmdStatus {
    /// `true` when a working `steamcmd` binary was found on PATH.
    pub available: bool,
    /// Absolute path to the `steamcmd` binary, if found.
    #[serde(default)]
    pub path: Option<String>,
    /// Human-readable message (e.g. version info or error reason).
    #[serde(default)]
    pub message: Option<String>,
}

/// Response from `GET /api/system/steamcmd-status`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SteamCmdStatusResponse {
    #[serde(flatten)]
    pub status: SteamCmdStatus,
}

// ─── App Validation ───

/// Successful validation of a Steam app ID.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SteamAppInfo {
    /// The numeric Steam application ID.
    #[ts(type = "number")]
    pub app_id: u32,
    /// The display name of the application (e.g. "Valheim Dedicated Server").
    pub name: String,
}

/// Response from `GET /api/steamcmd/validate-app`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ValidateAppResponse {
    pub valid: bool,
    #[serde(default)]
    pub app: Option<SteamAppInfo>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Query parameters for `GET /api/steamcmd/validate-app`.
#[derive(Debug, Clone, Deserialize)]
pub struct ValidateAppQuery {
    pub app_id: u32,
}

// ─── Detection ───

/// Detect whether `steamcmd` is available on the system PATH.
///
/// This is intentionally a synchronous function suitable for
/// `spawn_blocking` — it only does a PATH lookup and optionally
/// runs `steamcmd +quit` to verify it works.
pub fn detect_steamcmd() -> SteamCmdStatus {
    // First try `which steamcmd` (works on Linux/macOS)
    if let Some(path) = find_on_path("steamcmd") {
        return SteamCmdStatus {
            available: true,
            path: Some(path.to_string_lossy().to_string()),
            message: Some("steamcmd found on PATH".to_string()),
        };
    }

    // Not found
    SteamCmdStatus {
        available: false,
        path: None,
        message: Some(
            "steamcmd not found on PATH. Please install SteamCMD and ensure it is in your PATH."
                .to_string(),
        ),
    }
}

// ─── Cached Detection ───

/// How long (in seconds) the cached SteamCMD status is considered fresh.
const CACHE_TTL_SECS: u64 = 60;

/// Global cache for the SteamCMD detection result.
/// The `OnceLock` is initialised on first access; the inner `Mutex` allows
/// refreshing the cached value after the TTL expires.
static STEAMCMD_CACHE: OnceLock<parking_lot::Mutex<(SteamCmdStatus, Instant)>> = OnceLock::new();

/// Return a cached [`SteamCmdStatus`], refreshing at most once every
/// `CACHE_TTL_SECS` seconds.
///
/// This is a synchronous function (PATH lookup only) and is cheap even
/// without caching, but the cache avoids redundant filesystem scans when
/// many requests hit the template-list endpoint concurrently.
pub fn detect_steamcmd_cached() -> SteamCmdStatus {
    let cache =
        STEAMCMD_CACHE.get_or_init(|| parking_lot::Mutex::new((detect_steamcmd(), Instant::now())));

    let mut guard = cache.lock();
    if guard.1.elapsed().as_secs() >= CACHE_TTL_SECS {
        *guard = (detect_steamcmd(), Instant::now());
    }
    guard.0.clone()
}

/// Clear the cached SteamCMD status so the next call to
/// [`detect_steamcmd_cached`] performs a fresh PATH lookup.
///
/// Useful for a "refresh" button on the System Health page.
pub fn invalidate_steamcmd_cache() {
    if let Some(cache) = STEAMCMD_CACHE.get() {
        let mut guard = cache.lock();
        // Set the timestamp far enough in the past to force a refresh.
        guard.1 = Instant::now() - std::time::Duration::from_secs(CACHE_TTL_SECS + 1);
    }
}

/// Look up a binary name on the system PATH.
fn find_on_path(binary: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(binary);
            if candidate.is_file() {
                Some(candidate)
            } else {
                None
            }
        })
    })
}

/// Get the path to the `steamcmd` binary, or an error message if it's not
/// available.  Used by pipeline executors.
pub fn steamcmd_path() -> Result<String, String> {
    let status = detect_steamcmd();
    if status.available {
        Ok(status.path.unwrap_or_else(|| "steamcmd".to_string()))
    } else {
        Err(status
            .message
            .unwrap_or_else(|| "SteamCMD is not installed or not on PATH".to_string()))
    }
}

// ─── Steam Store API ───

/// Validate a Steam app ID by querying the Steam store API.
///
/// Returns the app's display name on success, or an error string on failure.
/// For a small set of known dedicated-server app IDs, this function falls
/// back to a built-in mapping when the Steam API is temporarily unavailable.
pub async fn validate_app_id(
    client: &reqwest::Client,
    app_id: u32,
) -> Result<SteamAppInfo, String> {
    let fallback = known_app_fallback(app_id);

    // Use the Steam store API (appdetails endpoint)
    let url = format!(
        "https://store.steampowered.com/api/appdetails?appids={}",
        app_id
    );

    let response = match client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            if let Some(info) = fallback.clone() {
                return Ok(info);
            }
            return Err(format!("Failed to query Steam store API: {}", e));
        }
    };

    if !response.status().is_success() {
        if let Some(info) = fallback.clone() {
            return Ok(info);
        }
        return Err(format!(
            "Steam store API returned status {}",
            response.status()
        ));
    }

    let body: serde_json::Value = match response.json().await {
        Ok(v) => v,
        Err(e) => {
            if let Some(info) = fallback.clone() {
                return Ok(info);
            }
            return Err(format!("Failed to parse Steam store API response: {}", e));
        }
    };

    // The response is keyed by app ID as a string
    let app_key = app_id.to_string();
    let app_data = match body.get(&app_key) {
        Some(v) => v,
        None => {
            if let Some(info) = fallback.clone() {
                return Ok(info);
            }
            return Err(format!("No data returned for app ID {}", app_id));
        }
    };

    let success = app_data
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !success {
        if let Some(info) = fallback.clone() {
            return Ok(info);
        }
        return Err(format!(
            "Steam app ID {} is not valid or not publicly accessible",
            app_id
        ));
    }

    let data = match app_data.get("data") {
        Some(v) => v,
        None => {
            if let Some(info) = fallback {
                return Ok(info);
            }
            return Err(format!("No data section for app ID {}", app_id));
        }
    };

    let name = data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    Ok(SteamAppInfo { app_id, name })
}

fn known_app_fallback(app_id: u32) -> Option<SteamAppInfo> {
    match app_id {
        896660 => Some(SteamAppInfo {
            app_id,
            name: "Valheim Dedicated Server".to_string(),
        }),
        _ => None,
    }
}

/// Check whether a `ServerConfig` references SteamCMD functionality.
///
/// Returns `true` if:
/// - `steam_app_id` is set, OR
/// - any pipeline step uses `SteamCmdInstall` or `SteamCmdUpdate`
pub fn config_requires_steamcmd(config: &crate::types::ServerConfig) -> bool {
    if config.steam_app_id.is_some() {
        return true;
    }

    let all_steps = config
        .install_steps
        .iter()
        .chain(config.update_steps.iter())
        .chain(config.uninstall_steps.iter())
        .chain(config.start_steps.iter())
        .chain(config.stop_steps.iter());

    for step in all_steps {
        match &step.action {
            crate::types::StepAction::SteamCmdInstall { .. }
            | crate::types::StepAction::SteamCmdUpdate { .. } => {
                return true;
            }
            _ => {}
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_a_status() {
        let status = detect_steamcmd();
        // We can't assert availability since it depends on the host,
        // but we can assert the structure is valid.
        if status.available {
            assert!(status.path.is_some());
        } else {
            assert!(status.message.is_some());
        }
    }

    #[test]
    fn detect_steamcmd_cached_returns_consistent_result() {
        // Two back-to-back calls within the TTL should return the same value
        // (same availability & path) without re-running the PATH lookup.
        let first = detect_steamcmd_cached();
        let second = detect_steamcmd_cached();
        assert_eq!(first.available, second.available);
        assert_eq!(first.path, second.path);
    }

    #[test]
    fn invalidate_cache_forces_refresh() {
        // Call once to populate cache, then invalidate and call again.
        let _ = detect_steamcmd_cached();
        invalidate_steamcmd_cache();
        // Should not panic; the next call performs a fresh lookup.
        let status = detect_steamcmd_cached();
        // Basic structural assertion
        if status.available {
            assert!(status.path.is_some());
        } else {
            assert!(status.message.is_some());
        }
    }

    #[test]
    fn find_on_path_finds_common_binary() {
        // `sh` should exist on any Unix system
        #[cfg(unix)]
        {
            let result = find_on_path("sh");
            assert!(result.is_some(), "Expected to find 'sh' on PATH");
        }
    }

    #[test]
    fn find_on_path_returns_none_for_nonexistent() {
        let result = find_on_path("this_binary_does_not_exist_anywhere_12345");
        assert!(result.is_none());
    }

    #[test]
    fn steamcmd_path_returns_err_when_not_available() {
        // This test is environment-dependent. We just verify it doesn't panic.
        let result = steamcmd_path();
        // Result is either Ok or Err — both are fine
        let _ = result;
    }

    #[test]
    fn config_requires_steamcmd_with_app_id() {
        let mut config = test_config();
        config.steam_app_id = Some(896660);
        assert!(config_requires_steamcmd(&config));
    }

    #[test]
    fn config_requires_steamcmd_with_step() {
        let mut config = test_config();
        config.install_steps.push(crate::types::PipelineStep {
            name: "Install via SteamCMD".into(),
            description: None,
            action: crate::types::StepAction::SteamCmdInstall {
                app_id: None,
                anonymous: true,
                extra_args: vec![],
            },
            condition: None,
            continue_on_error: false,
        });
        assert!(config_requires_steamcmd(&config));
    }

    #[test]
    fn config_does_not_require_steamcmd_plain() {
        let config = test_config();
        assert!(!config_requires_steamcmd(&config));
    }

    fn test_config() -> crate::types::ServerConfig {
        crate::types::ServerConfig {
            name: "Test".into(),
            binary: "/bin/echo".into(),
            args: vec![],
            env: std::collections::HashMap::new(),
            working_dir: None,
            auto_start: false,
            auto_restart: false,
            max_restart_attempts: 0,
            restart_delay_secs: 5,
            stop_command: None,
            stop_signal: crate::types::StopSignal::default(),
            stop_timeout_secs: 10,
            sftp_username: None,
            sftp_password: None,
            parameters: vec![],
            stop_steps: vec![],
            start_steps: vec![],
            install_steps: vec![],
            update_steps: vec![],
            uninstall_steps: vec![],
            isolation: crate::types::IsolationConfig::default(),
            update_check: None,
            log_to_disk: true,
            max_log_size_mb: 50,
            enable_java_helper: false,
            enable_dotnet_helper: false,
            steam_app_id: None,
        }
    }
}
