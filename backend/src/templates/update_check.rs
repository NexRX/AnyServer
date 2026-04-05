//! Update-check logic: provider execution, version extraction, and
//! version comparison helpers.
//!
//! This module is consumed by the API handler in `api/update_check.rs` and
//! by tests.  It deliberately does **not** depend on `AppState` so that the
//! core logic can be unit-tested in isolation.
//!
//! JSON path navigation is provided by [`crate::json_path::json_navigate`].

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::types::{UpdateCheck, UpdateCheckProvider, UpdateCheckResult, VersionPick};
pub use crate::utils::json_path::json_navigate;

/// Given a JSON value that is either:
///   - a string → return it directly
///   - an array of strings/objects → pick one via `VersionPick` and
///     optionally read `value_key` from the object
///
/// Returns `None` if the shape doesn't match.
pub fn extract_version(
    value: &Value,
    pick: VersionPick,
    value_key: Option<&str>,
) -> Option<String> {
    match value {
        // Scalar string
        Value::String(s) => Some(s.clone()),
        // Array
        Value::Array(arr) if !arr.is_empty() => {
            let element = match pick {
                VersionPick::First => arr.first()?,
                VersionPick::Last => arr.last()?,
            };

            match value_key {
                Some(key) => {
                    // Element should be an object with `key`
                    element.get(key).and_then(|v| match v {
                        Value::String(s) => Some(s.clone()),
                        Value::Number(n) => Some(n.to_string()),
                        _ => None,
                    })
                }
                None => {
                    // Element should be a string
                    match element {
                        Value::String(s) => Some(s.clone()),
                        Value::Number(n) => Some(n.to_string()),
                        _ => None,
                    }
                }
            }
        }
        // Number at top level (unusual but possible)
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Substitute `${param}` placeholders in a string using the supplied
/// variable map.  Re-uses the same convention as pipeline variables.
pub fn substitute_variables(input: &str, vars: &HashMap<String, String>) -> String {
    let mut result = input.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("${{{}}}", key), value);
    }
    result
}

// ─── Provider execution ──────────────────────────────────────────────

/// Max response body size for API checks (2 MB).
const API_RESPONSE_MAX_BYTES: usize = 2 * 1024 * 1024;
/// Hard timeout on outbound HTTP requests.
const API_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Execute the `api` provider: fetch a URL, navigate the response JSON,
/// and extract the latest version string.
pub async fn execute_api_provider(
    http_client: &reqwest::Client,
    url: &str,
    path: Option<&str>,
    pick: VersionPick,
    value_key: Option<&str>,
    vars: &HashMap<String, String>,
) -> Result<String, String> {
    let url = substitute_variables(url, vars);

    let resp = http_client
        .get(&url)
        .timeout(API_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("HTTP request to {} failed: {}", url, e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {} from {}", resp.status(), url));
    }

    // Read body with size cap
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body from {}: {}", url, e))?;

    if bytes.len() > API_RESPONSE_MAX_BYTES {
        return Err(format!(
            "Response from {} exceeds 2 MB limit ({} bytes)",
            url,
            bytes.len()
        ));
    }

    let json: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Response from {} is not valid JSON: {}", url, e))?;

    let navigated = json_navigate(&json, path).ok_or_else(|| {
        format!(
            "Path '{}' did not resolve in the JSON response from {}",
            path.unwrap_or(""),
            url
        )
    })?;

    extract_version(navigated, pick, value_key).ok_or_else(|| {
        format!(
            "Could not extract a version string from the resolved value at path '{}' in response from {}",
            path.unwrap_or("(root)"),
            url
        )
    })
}

/// Execute the `command` provider: run a shell command and capture the
/// first non-empty line of stdout as the latest version.
pub async fn execute_command_provider(
    command: &str,
    timeout_secs: u32,
    vars: &HashMap<String, String>,
) -> Result<String, String> {
    let command = substitute_variables(command, vars);

    let timeout = Duration::from_secs(timeout_secs as u64);

    let output = tokio::time::timeout(
        timeout,
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| {
        format!(
            "Update check command timed out after {}s: {}",
            timeout_secs, command
        )
    })?
    .map_err(|e| format!("Failed to execute update check command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Update check command exited with {}: {}",
            output.status,
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .ok_or_else(|| "Update check command produced no output".to_string())
}

// ─── High-level check orchestration ──────────────────────────────────

/// Build the variable map used for `${param}` substitution in update
/// check URLs and commands.
pub fn build_check_variables(
    parameter_values: &HashMap<String, String>,
    parameters: &[crate::types::ConfigParameter],
) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    for (key, value) in parameter_values {
        vars.insert(key.clone(), value.clone());
    }
    // Fill in defaults for parameters not in parameter_values
    for param in parameters {
        if !vars.contains_key(&param.name) {
            if let Some(ref default) = param.default {
                vars.insert(param.name.clone(), default.clone());
            }
        }
    }
    vars
}

/// Find the version parameter name in a list of config parameters.
pub fn find_version_param_name(parameters: &[crate::types::ConfigParameter]) -> Option<String> {
    parameters
        .iter()
        .find(|p| p.is_version)
        .map(|p| p.name.clone())
}

/// Get the installed version from the server, falling back to looking up
/// the version parameter value.
pub fn get_installed_version(
    installed_version: &Option<String>,
    parameter_values: &HashMap<String, String>,
    parameters: &[crate::types::ConfigParameter],
) -> Option<String> {
    if let Some(ref v) = installed_version {
        return Some(v.clone());
    }
    // Fallback: read the version parameter's current value
    let param_name = find_version_param_name(parameters)?;
    parameter_values.get(&param_name).cloned()
}

/// Perform an update check for a given server and return the result.
///
/// `template_lookup` is a callback that resolves a template ID to its
/// default version parameter value — used by the `TemplateDefault`
/// provider.
pub async fn perform_check(
    http_client: &reqwest::Client,
    server_id: Uuid,
    update_check: &UpdateCheck,
    installed_version: Option<String>,
    vars: &HashMap<String, String>,
    template_lookup: impl FnOnce() -> Result<Option<String>, String>,
    curseforge_api_key: Option<&str>,
) -> UpdateCheckResult {
    let now = Utc::now();

    // For CurseForge we get both the version ID and a human-readable
    // display name.  Other providers only produce a plain version string.
    let (latest_result, mut latest_version_display, mut installed_version_display): (
        Result<String, String>,
        Option<String>,
        Option<String>,
    ) = match &update_check.provider {
        UpdateCheckProvider::Api {
            url,
            path,
            pick,
            value_key,
        } => {
            let r = execute_api_provider(
                http_client,
                url,
                path.as_deref(),
                *pick,
                value_key.as_deref(),
                vars,
            )
            .await;
            (r, None, None)
        }
        UpdateCheckProvider::TemplateDefault => {
            let r = template_lookup().and_then(|opt| {
                opt.ok_or_else(|| {
                    "Could not determine template default version (template not found or no version parameter)".to_string()
                })
            });
            (r, None, None)
        }
        UpdateCheckProvider::Command {
            command,
            timeout_secs,
        } => {
            let r = execute_command_provider(command, *timeout_secs, vars).await;
            (r, None, None)
        }
        UpdateCheckProvider::CurseForge { project_id } => {
            match execute_curseforge_provider(http_client, *project_id, curseforge_api_key).await {
                Ok((version, display)) => (Ok(version), Some(display), None),
                Err(e) => (Err(e), None, None),
            }
        }
    };

    // For CurseForge, resolve the installed version's display name by
    // fetching the file metadata.  This is best-effort — we silently
    // fall back to `None` on any error.
    if let UpdateCheckProvider::CurseForge { project_id } = &update_check.provider {
        if let Some(ref installed) = installed_version {
            if let Ok(file_id) = installed.parse::<u32>() {
                if let Some(api_key) = curseforge_api_key {
                    if let Ok(file) = crate::integrations::curseforge::fetch_file(
                        http_client,
                        api_key,
                        *project_id,
                        file_id,
                    )
                    .await
                    {
                        if !file.display_name.is_empty() {
                            installed_version_display = Some(file.display_name);
                        }
                    }
                }
            }
        }
        // If the latest display name ended up empty, clear it
        if latest_version_display.as_deref() == Some("") {
            latest_version_display = None;
        }
    }

    match latest_result {
        Ok(latest) => {
            let update_available = match &installed_version {
                Some(installed) => installed != &latest,
                None => false, // Can't determine — no installed version
            };
            UpdateCheckResult {
                server_id,
                update_available,
                installed_version,
                latest_version: Some(latest),
                installed_version_display,
                latest_version_display,
                checked_at: now,
                error: None,
            }
        }
        Err(err) => UpdateCheckResult {
            server_id,
            update_available: false,
            installed_version,
            latest_version: None,
            installed_version_display: None,
            latest_version_display: None,
            checked_at: now,
            error: Some(err),
        },
    }
}

/// Execute the `curseforge` provider: query the CurseForge API for the
/// newest file of a project and return its file ID as a string together
/// with the human-readable display name.
///
/// The installed version (a file ID) is compared against this to
/// determine whether an update is available.
async fn execute_curseforge_provider(
    http_client: &reqwest::Client,
    project_id: u32,
    api_key: Option<&str>,
) -> Result<(String, String), String> {
    let api_key = api_key.ok_or_else(|| {
        "CurseForge API key is not configured. \
         Ask an admin to set it up in Admin Panel → CurseForge."
            .to_string()
    })?;

    let files =
        crate::integrations::curseforge::fetch_project_files(http_client, api_key, project_id, 1)
            .await
            .map_err(|e| {
                format!(
                    "Failed to fetch CurseForge files for project {}: {}",
                    project_id, e
                )
            })?;

    let latest = files.first().ok_or_else(|| {
        format!(
            "No available files found for CurseForge project {}",
            project_id
        )
    })?;

    Ok((latest.id.to_string(), latest.display_name.clone()))
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ─── json_navigate ───

    #[test]
    fn navigate_none_path_returns_root() {
        let data = json!({"a": 1});
        assert_eq!(json_navigate(&data, None), Some(&data));
    }

    #[test]
    fn navigate_empty_path_returns_root() {
        let data = json!({"a": 1});
        assert_eq!(json_navigate(&data, Some("")), Some(&data));
    }

    #[test]
    fn navigate_single_key() {
        let data = json!({"versions": [1, 2, 3]});
        assert_eq!(
            json_navigate(&data, Some("versions")),
            Some(&json!([1, 2, 3]))
        );
    }

    #[test]
    fn navigate_nested_path() {
        let data = json!({"data": {"builds": {"latest": "42"}}});
        assert_eq!(
            json_navigate(&data, Some("data.builds.latest")),
            Some(&json!("42"))
        );
    }

    #[test]
    fn navigate_missing_key_returns_none() {
        let data = json!({"a": 1});
        assert_eq!(json_navigate(&data, Some("b")), None);
    }

    #[test]
    fn navigate_through_non_object_returns_none() {
        let data = json!({"a": "hello"});
        assert_eq!(json_navigate(&data, Some("a.b")), None);
    }

    // ─── extract_version ───

    #[test]
    fn extract_from_string() {
        let val = json!("1.21.4");
        assert_eq!(
            extract_version(&val, VersionPick::Last, None),
            Some("1.21.4".into())
        );
    }

    #[test]
    fn extract_from_number() {
        let val = json!(42);
        assert_eq!(
            extract_version(&val, VersionPick::Last, None),
            Some("42".into())
        );
    }

    #[test]
    fn extract_last_from_string_array() {
        let val = json!(["1.8", "1.8.1", "1.21.4"]);
        assert_eq!(
            extract_version(&val, VersionPick::Last, None),
            Some("1.21.4".into())
        );
    }

    #[test]
    fn extract_first_from_string_array() {
        let val = json!(["1.8", "1.8.1", "1.21.4"]);
        assert_eq!(
            extract_version(&val, VersionPick::First, None),
            Some("1.8".into())
        );
    }

    #[test]
    fn extract_with_value_key_first() {
        let val = json!([
            {"tag_name": "v5.2.1", "other": "x"},
            {"tag_name": "v5.2.0", "other": "y"}
        ]);
        assert_eq!(
            extract_version(&val, VersionPick::First, Some("tag_name")),
            Some("v5.2.1".into())
        );
    }

    #[test]
    fn extract_with_value_key_last() {
        let val = json!([
            {"tag_name": "v5.2.0", "other": "y"},
            {"tag_name": "v5.2.1", "other": "x"}
        ]);
        assert_eq!(
            extract_version(&val, VersionPick::Last, Some("tag_name")),
            Some("v5.2.1".into())
        );
    }

    #[test]
    fn extract_empty_array_returns_none() {
        let val = json!([]);
        assert_eq!(extract_version(&val, VersionPick::Last, None), None);
    }

    #[test]
    fn extract_object_without_value_key_returns_none() {
        let val = json!([{"a": 1}]);
        assert_eq!(extract_version(&val, VersionPick::First, None), None);
    }

    #[test]
    fn extract_with_missing_value_key_returns_none() {
        let val = json!([{"a": 1}]);
        assert_eq!(
            extract_version(&val, VersionPick::First, Some("tag_name")),
            None
        );
    }

    #[test]
    fn extract_numeric_value_key() {
        let val = json!([{"build": 123}]);
        assert_eq!(
            extract_version(&val, VersionPick::First, Some("build")),
            Some("123".into())
        );
    }

    #[test]
    fn extract_null_returns_none() {
        let val = json!(null);
        assert_eq!(extract_version(&val, VersionPick::First, None), None);
    }

    #[test]
    fn extract_bool_returns_none() {
        let val = json!(true);
        assert_eq!(extract_version(&val, VersionPick::First, None), None);
    }

    // ─── Full path + extraction (simulating PaperMC response) ───

    #[test]
    fn papermc_style_response() {
        let response = json!({
            "project_id": "paper",
            "project_name": "Paper",
            "versions": ["1.8", "1.8.1", "1.19", "1.20", "1.21.4"]
        });
        let navigated = json_navigate(&response, Some("versions")).unwrap();
        let version = extract_version(navigated, VersionPick::Last, None).unwrap();
        assert_eq!(version, "1.21.4");
    }

    #[test]
    fn github_releases_style_response() {
        let response = json!([
            {"tag_name": "v5.2.1", "name": "TShock 5.2.1"},
            {"tag_name": "v5.2.0", "name": "TShock 5.2.0"},
            {"tag_name": "v5.1.0", "name": "TShock 5.1.0"}
        ]);
        let navigated = json_navigate(&response, None).unwrap();
        let version = extract_version(navigated, VersionPick::First, Some("tag_name")).unwrap();
        assert_eq!(version, "v5.2.1");
    }

    #[test]
    fn nested_build_number_response() {
        let response = json!({
            "project": "paper",
            "version": "1.21.4",
            "builds": [
                {"build": 100, "channel": "default"},
                {"build": 101, "channel": "default"},
                {"build": 102, "channel": "experimental"}
            ]
        });
        let navigated = json_navigate(&response, Some("builds")).unwrap();
        let version = extract_version(navigated, VersionPick::Last, Some("build")).unwrap();
        assert_eq!(version, "102");
    }

    // ─── substitute_variables ───

    #[test]
    fn substitute_replaces_placeholders() {
        let mut vars = HashMap::new();
        vars.insert("mc_version".into(), "1.21.4".into());
        vars.insert("server_port".into(), "25565".into());

        assert_eq!(
            substitute_variables(
                "https://api.papermc.io/v2/projects/paper/versions/${mc_version}/builds",
                &vars
            ),
            "https://api.papermc.io/v2/projects/paper/versions/1.21.4/builds"
        );
    }

    #[test]
    fn substitute_no_vars() {
        let vars = HashMap::new();
        assert_eq!(
            substitute_variables("https://example.com/api", &vars),
            "https://example.com/api"
        );
    }

    // ─── find_version_param_name ───

    #[test]
    fn find_version_param() {
        use crate::types::pipeline::ConfigParameter;
        let params = vec![
            ConfigParameter {
                name: "memory".into(),
                label: "Memory".into(),
                default: Some("2G".into()),
                ..Default::default()
            },
            ConfigParameter {
                name: "mc_version".into(),
                label: "Minecraft Version".into(),
                default: Some("1.21.4".into()),
                required: true,
                is_version: true,
                ..Default::default()
            },
        ];
        assert_eq!(find_version_param_name(&params), Some("mc_version".into()));
    }

    #[test]
    fn find_version_param_none() {
        use crate::types::pipeline::ConfigParameter;
        let params = vec![ConfigParameter {
            name: "memory".into(),
            label: "Memory".into(),
            default: Some("2G".into()),
            ..Default::default()
        }];
        assert_eq!(find_version_param_name(&params), None);
    }

    // ─── get_installed_version ───

    #[test]
    fn installed_version_prefers_explicit() {
        use crate::types::pipeline::ConfigParameter;
        let mut pv = HashMap::new();
        pv.insert("mc_version".into(), "1.20.0".into());
        let params = vec![ConfigParameter {
            name: "mc_version".into(),
            label: "MC Version".into(),
            default: Some("1.21.4".into()),
            required: true,
            is_version: true,
            ..Default::default()
        }];

        // Explicit installed_version takes priority
        assert_eq!(
            get_installed_version(&Some("1.19.0".into()), &pv, &params),
            Some("1.19.0".into())
        );
    }

    #[test]
    fn installed_version_falls_back_to_param_value() {
        use crate::types::pipeline::ConfigParameter;
        let mut pv = HashMap::new();
        pv.insert("mc_version".into(), "1.20.0".into());
        let params = vec![ConfigParameter {
            name: "mc_version".into(),
            label: "MC Version".into(),
            default: Some("1.21.4".into()),
            required: true,
            is_version: true,
            ..Default::default()
        }];

        assert_eq!(
            get_installed_version(&None, &pv, &params),
            Some("1.20.0".into())
        );
    }

    #[test]
    fn installed_version_none_when_no_version_param() {
        let pv = HashMap::new();
        let params: Vec<crate::types::ConfigParameter> = vec![];
        assert_eq!(get_installed_version(&None, &pv, &params), None);
    }

    // ─── perform_check with template_default ───

    #[tokio::test]
    async fn template_default_update_available() {
        let update_check = UpdateCheck {
            provider: UpdateCheckProvider::TemplateDefault,
            interval_secs: None,
            cache_secs: 300,
        };
        let server_id = Uuid::new_v4();
        let vars = HashMap::new();

        let client = reqwest::Client::new();
        let result = perform_check(
            &client,
            server_id,
            &update_check,
            Some("1.20.0".into()),
            &vars,
            || Ok(Some("1.21.4".into())),
            None,
        )
        .await;

        assert!(result.update_available);
        assert_eq!(result.installed_version, Some("1.20.0".into()));
        assert_eq!(result.latest_version, Some("1.21.4".into()));
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn template_default_up_to_date() {
        let update_check = UpdateCheck {
            provider: UpdateCheckProvider::TemplateDefault,
            interval_secs: None,
            cache_secs: 300,
        };
        let server_id = Uuid::new_v4();
        let vars = HashMap::new();

        let client = reqwest::Client::new();
        let result = perform_check(
            &client,
            server_id,
            &update_check,
            Some("1.21.4".into()),
            &vars,
            || Ok(Some("1.21.4".into())),
            None,
        )
        .await;

        assert!(!result.update_available);
        assert_eq!(result.latest_version, Some("1.21.4".into()));
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn template_default_lookup_error() {
        let update_check = UpdateCheck {
            provider: UpdateCheckProvider::TemplateDefault,
            interval_secs: None,
            cache_secs: 300,
        };
        let server_id = Uuid::new_v4();
        let vars = HashMap::new();

        let client = reqwest::Client::new();
        let result = perform_check(
            &client,
            server_id,
            &update_check,
            Some("1.21.4".into()),
            &vars,
            || Err("Template not found".to_string()),
            None,
        )
        .await;

        assert!(!result.update_available);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("Template not found"));
    }

    #[tokio::test]
    async fn template_default_no_installed_version() {
        let update_check = UpdateCheck {
            provider: UpdateCheckProvider::TemplateDefault,
            interval_secs: None,
            cache_secs: 300,
        };
        let server_id = Uuid::new_v4();
        let vars = HashMap::new();

        let client = reqwest::Client::new();
        let result = perform_check(
            &client,
            server_id,
            &update_check,
            None,
            &vars,
            || Ok(Some("2.0.0".to_string())),
            None,
        )
        .await;

        // No installed version → can't determine if update is available
        assert!(!result.update_available);
        assert_eq!(result.latest_version, Some("2.0.0".into()));
        assert!(result.error.is_none());
    }

    // ─── perform_check with command provider ───

    #[tokio::test]
    async fn command_provider_basic() {
        let update_check = UpdateCheck {
            provider: UpdateCheckProvider::Command {
                command: "echo 1.21.5".into(),
                timeout_secs: 5,
            },
            interval_secs: None,
            cache_secs: 300,
        };
        let server_id = Uuid::new_v4();
        let vars = HashMap::new();

        let client = reqwest::Client::new();
        let result = perform_check(
            &client,
            server_id,
            &update_check,
            Some("1.21.4".into()),
            &vars,
            || unreachable!(),
            None,
        )
        .await;

        assert!(result.update_available);
        assert_eq!(result.latest_version, Some("1.21.5".into()));
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn command_provider_with_variable_substitution() {
        let update_check = UpdateCheck {
            provider: UpdateCheckProvider::Command {
                command: "echo v${mc_version}-latest".into(),
                timeout_secs: 5,
            },
            interval_secs: None,
            cache_secs: 300,
        };
        let server_id = Uuid::new_v4();
        let mut vars = HashMap::new();
        vars.insert("mc_version".into(), "1.21.4".into());

        let client = reqwest::Client::new();
        let result = perform_check(
            &client,
            server_id,
            &update_check,
            Some("1.21.4".into()),
            &vars,
            || unreachable!(),
            None,
        )
        .await;

        assert!(result.update_available);
        assert_eq!(result.latest_version, Some("v1.21.4-latest".into()));
    }

    #[tokio::test]
    async fn command_provider_failure() {
        let update_check = UpdateCheck {
            provider: UpdateCheckProvider::Command {
                command: "false".into(), // exits with code 1
                timeout_secs: 5,
            },
            interval_secs: None,
            cache_secs: 300,
        };
        let server_id = Uuid::new_v4();
        let vars = HashMap::new();

        let client = reqwest::Client::new();
        let result = perform_check(
            &client,
            server_id,
            &update_check,
            Some("1.21.4".into()),
            &vars,
            || unreachable!(),
            None,
        )
        .await;

        assert!(!result.update_available);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn command_provider_empty_output() {
        let update_check = UpdateCheck {
            provider: UpdateCheckProvider::Command {
                command: "echo -n ''".into(), // no output
                timeout_secs: 5,
            },
            interval_secs: None,
            cache_secs: 300,
        };
        let server_id = Uuid::new_v4();
        let vars = HashMap::new();

        let client = reqwest::Client::new();
        let result = perform_check(
            &client,
            server_id,
            &update_check,
            Some("1.21.4".into()),
            &vars,
            || unreachable!(),
            None,
        )
        .await;

        assert!(!result.update_available);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("no output"));
    }

    // ─── build_check_variables ───

    #[test]
    fn build_vars_merges_params_and_defaults() {
        use crate::types::pipeline::ConfigParameter;
        let mut pv = HashMap::new();
        pv.insert("mc_version".into(), "1.21.4".into());

        let params = vec![
            ConfigParameter {
                name: "mc_version".into(),
                label: "MC Version".into(),
                default: Some("1.20.0".into()),
                required: true,
                is_version: true,
                ..Default::default()
            },
            ConfigParameter {
                name: "memory".into(),
                label: "Memory".into(),
                default: Some("2G".into()),
                ..Default::default()
            },
        ];

        let vars = build_check_variables(&pv, &params);
        // Explicit value wins over default
        assert_eq!(vars.get("mc_version"), Some(&"1.21.4".into()));
        // Default is used when not in parameter_values
        assert_eq!(vars.get("memory"), Some(&"2G".into()));
    }
}
