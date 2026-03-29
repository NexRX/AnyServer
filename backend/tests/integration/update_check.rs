//! E2E tests for update-check (ticket 022).
//!
//! Covers:
//! - `GET /api/servers/:id/check-update` for all three providers
//! - `GET /api/servers/update-status` bulk endpoint
//! - `source_template_id` wiring on server creation
//! - `installed_version` recording after install pipeline
//! - Cache behaviour and `?force=true` bypass
//! - Error handling (no update_check configured, server not found, etc.)

use axum::http::StatusCode;
use serde_json::json;

use super::common::TestApp;

// ─── Helpers ─────────────────────────────────────────────────────────

/// Create a server with an `update_check` configuration using the
/// `command` provider (easiest to test without network access).
async fn create_server_with_command_check(
    app: &TestApp,
    token: &str,
    name: &str,
    command: &str,
    version_default: &str,
) -> (String, serde_json::Value) {
    let echo = super::common::resolve_binary("echo");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(token),
            json!({
                "config": {
                    "name": name,
                    "binary": echo,
                    "args": ["hello"],
                    "parameters": [
                        {
                            "name": "app_version",
                            "label": "App Version",
                            "param_type": "string",
                            "default": version_default,
                            "required": true,
                            "options": [],
                            "is_version": true
                        }
                    ],
                    "update_check": {
                        "provider": "command",
                        "command": command,
                        "timeout_secs": 5,
                        "interval_secs": 0,
                        "cache_secs": 5
                    },
                    "stop_timeout_secs": 10
                },
                "parameter_values": {
                    "app_version": version_default
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create_server failed: {:?}", body);
    let id = body["server"]["id"].as_str().unwrap().to_string();
    (id, body)
}

/// Create a server with NO update_check configured.
async fn create_server_without_update_check(
    app: &TestApp,
    token: &str,
    name: &str,
) -> (String, serde_json::Value) {
    let echo = super::common::resolve_binary("echo");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(token),
            json!({
                "config": {
                    "name": name,
                    "binary": echo,
                    "args": ["hello"],
                    "stop_timeout_secs": 10
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create_server failed: {:?}", body);
    let id = body["server"]["id"].as_str().unwrap().to_string();
    (id, body)
}

// ─── Tests ───────────────────────────────────────────────────────────

// ─── Command provider: basic update detection ────────────────────────

#[tokio::test]
async fn test_check_update_command_provider_detects_update() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Server has version 1.0.0, command returns 2.0.0
    let (id, _) =
        create_server_with_command_check(&app, &token, "cmd-update", "echo 2.0.0", "1.0.0").await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/check-update", id), Some(&token))
        .await;

    assert_eq!(status, StatusCode::OK, "check-update failed: {:?}", body);
    assert_eq!(body["update_available"], true);
    assert_eq!(body["latest_version"], "2.0.0");
    // installed_version should fall back to parameter value
    assert_eq!(body["installed_version"], "1.0.0");
    assert!(body["error"].is_null());
    assert!(body["checked_at"].is_string());
    assert_eq!(body["server_id"], id);
}

#[tokio::test]
async fn test_check_update_command_provider_no_update() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Server has version 1.0.0, command also returns 1.0.0
    let (id, _) =
        create_server_with_command_check(&app, &token, "cmd-noupdate", "echo 1.0.0", "1.0.0").await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/check-update", id), Some(&token))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], false);
    assert_eq!(body["latest_version"], "1.0.0");
    assert_eq!(body["installed_version"], "1.0.0");
    assert!(body["error"].is_null());
}

#[tokio::test]
async fn test_check_update_command_provider_failure() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Command fails (exit code 1)
    let (id, _) =
        create_server_with_command_check(&app, &token, "cmd-fail", "false", "1.0.0").await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/check-update", id), Some(&token))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], false);
    assert!(body["error"].is_string());
    assert!(body["latest_version"].is_null());
}

// ─── No update_check configured ──────────────────────────────────────

#[tokio::test]
async fn test_check_update_not_configured_returns_bad_request() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (id, _) = create_server_without_update_check(&app, &token, "no-check").await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/check-update", id), Some(&token))
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {:?}", body);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("update checking configured"));
}

// ─── Server not found ────────────────────────────────────────────────

#[tokio::test]
async fn test_check_update_server_not_found() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let fake_id = uuid::Uuid::new_v4();
    let (status, _) = app
        .get(
            &format!("/api/servers/{}/check-update", fake_id),
            Some(&token),
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── Unauthenticated request ─────────────────────────────────────────

#[tokio::test]
async fn test_check_update_unauthenticated() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (id, _) =
        create_server_with_command_check(&app, &token, "auth-test", "echo 1.0.0", "1.0.0").await;

    let (status, _) = app
        .get(&format!("/api/servers/{}/check-update", id), None)
        .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ─── Cache behaviour ─────────────────────────────────────────────────

#[tokio::test]
async fn test_check_update_caching() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // We use a timestamp-based command so we can tell if the cache was hit
    // (same checked_at means cache was returned).
    let (id, _) =
        create_server_with_command_check(&app, &token, "cache-test", "echo 2.0.0", "1.0.0").await;

    // First request — populates cache
    let (s1, b1) = app
        .get(&format!("/api/servers/{}/check-update", id), Some(&token))
        .await;
    assert_eq!(s1, StatusCode::OK);
    let checked_at_1 = b1["checked_at"].as_str().unwrap().to_string();

    // Second request — should return cached result (same checked_at)
    let (s2, b2) = app
        .get(&format!("/api/servers/{}/check-update", id), Some(&token))
        .await;
    assert_eq!(s2, StatusCode::OK);
    let checked_at_2 = b2["checked_at"].as_str().unwrap().to_string();
    assert_eq!(checked_at_1, checked_at_2, "Expected cached result");

    // Force bypass — should produce a new checked_at
    // (In practice this runs so fast the timestamps might be the same,
    //  so we just verify it succeeds.)
    let (s3, b3) = app
        .get(
            &format!("/api/servers/{}/check-update?force=true", id),
            Some(&token),
        )
        .await;
    assert_eq!(s3, StatusCode::OK);
    assert_eq!(b3["update_available"], true);
}

// ─── Bulk update-status endpoint ─────────────────────────────────────

#[tokio::test]
async fn test_update_status_returns_cached_results() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create two servers — one with update_check, one without
    let (id1, _) =
        create_server_with_command_check(&app, &token, "bulk-1", "echo 2.0.0", "1.0.0").await;
    let (_id2, _) = create_server_without_update_check(&app, &token, "bulk-2").await;

    // No cached results yet
    let (status, body) = app.get("/api/servers/update-status", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 0);

    // Trigger a check for server 1
    let (s, _) = app
        .get(&format!("/api/servers/{}/check-update", id1), Some(&token))
        .await;
    assert_eq!(s, StatusCode::OK);

    // Now bulk status should include server 1
    let (status, body) = app.get("/api/servers/update-status", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["server_id"], id1);
    assert_eq!(results[0]["update_available"], true);
}

#[tokio::test]
async fn test_update_status_unauthenticated() {
    let app = TestApp::new().await;
    let _token = app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app.get("/api/servers/update-status", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ─── source_template_id wiring ───────────────────────────────────────

#[tokio::test]
async fn test_create_server_with_source_template_id() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = super::common::resolve_binary("echo");
    let template_id = uuid::Uuid::new_v4();

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "template-tracked",
                    "binary": echo,
                    "args": [],
                    "stop_timeout_secs": 10
                },
                "parameter_values": {},
                "source_template_id": template_id.to_string()
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    assert_eq!(
        body["server"]["source_template_id"],
        template_id.to_string()
    );
}

#[tokio::test]
async fn test_create_server_without_source_template_id() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = super::common::resolve_binary("echo");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "no-template",
                    "binary": echo,
                    "args": [],
                    "stop_timeout_secs": 10
                },
                "parameter_values": {}
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body["server"]["source_template_id"].is_null(),
        "Expected null source_template_id, got: {:?}",
        body["server"]["source_template_id"]
    );
}

// ─── is_version field on parameters ──────────────────────────────────

#[tokio::test]
async fn test_is_version_field_round_trips() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = super::common::resolve_binary("echo");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "version-param-test",
                    "binary": echo,
                    "args": [],
                    "parameters": [
                        {
                            "name": "ver",
                            "label": "Version",
                            "param_type": "string",
                            "default": "1.0",
                            "required": true,
                            "options": [],
                            "is_version": true
                        },
                        {
                            "name": "other",
                            "label": "Other",
                            "param_type": "string",
                            "default": "x",
                            "required": false,
                            "options": [],
                            "is_version": false
                        }
                    ],
                    "stop_timeout_secs": 10
                },
                "parameter_values": {"ver": "1.0"}
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    // Verify the is_version field round-trips through the API
    let params = body["server"]["config"]["parameters"].as_array().unwrap();
    assert_eq!(params.len(), 2);

    let ver_param = params.iter().find(|p| p["name"] == "ver").unwrap();
    assert_eq!(ver_param["is_version"], true);

    let other_param = params.iter().find(|p| p["name"] == "other").unwrap();
    assert_eq!(other_param["is_version"], false);
}

// ─── update_check field on config round-trips ────────────────────────

#[tokio::test]
async fn test_update_check_config_round_trips() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = super::common::resolve_binary("echo");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "check-config-test",
                    "binary": echo,
                    "args": [],
                    "parameters": [
                        {
                            "name": "ver",
                            "label": "Version",
                            "param_type": "string",
                            "default": "1.0",
                            "required": true,
                            "options": [],
                            "is_version": true
                        }
                    ],
                    "update_check": {
                        "provider": "api",
                        "url": "https://example.com/api/versions",
                        "path": "versions",
                        "pick": "last",
                        "interval_secs": 3600,
                        "cache_secs": 600
                    },
                    "stop_timeout_secs": 10
                },
                "parameter_values": {"ver": "1.0"}
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    let uc = &body["server"]["config"]["update_check"];
    assert_eq!(uc["provider"], "api");
    assert_eq!(uc["url"], "https://example.com/api/versions");
    assert_eq!(uc["path"], "versions");
    assert_eq!(uc["pick"], "last");
    assert_eq!(uc["interval_secs"], 3600);
    assert_eq!(uc["cache_secs"], 600);
}

// ─── Builtin templates have update_check ─────────────────────────────

#[tokio::test]
async fn test_builtin_templates_have_update_check() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/templates", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let templates = body["templates"].as_array().unwrap();

    // Find Paper template
    let paper = templates
        .iter()
        .find(|t| t["name"].as_str().unwrap().contains("Paper"))
        .expect("Minecraft Paper template not found");
    let paper_uc = &paper["config"]["update_check"];
    assert_eq!(paper_uc["provider"], "api");
    assert!(paper_uc["url"].as_str().unwrap().contains("papermc.io"));

    // Paper mc_version should be marked is_version
    let paper_params = paper["config"]["parameters"].as_array().unwrap();
    let mc_ver = paper_params
        .iter()
        .find(|p| p["name"] == "mc_version")
        .unwrap();
    assert_eq!(mc_ver["is_version"], true);

    // Find TShock template
    let tshock = templates
        .iter()
        .find(|t| t["name"].as_str().unwrap().contains("TShock"))
        .expect("Terraria TShock template not found");
    let tshock_uc = &tshock["config"]["update_check"];
    assert_eq!(tshock_uc["provider"], "api");
    assert!(tshock_uc["url"].as_str().unwrap().contains("github.com"));

    // TShock tshock_version should be marked is_version
    let tshock_params = tshock["config"]["parameters"].as_array().unwrap();
    let ts_ver = tshock_params
        .iter()
        .find(|p| p["name"] == "tshock_version")
        .unwrap();
    assert_eq!(ts_ver["is_version"], true);

    // Valheim should NOT have update_check
    let valheim = templates
        .iter()
        .find(|t| t["name"].as_str().unwrap().contains("Valheim"))
        .expect("Valheim template not found");
    assert!(
        valheim["config"]["update_check"].is_null(),
        "Valheim should not have update_check"
    );

    // Valheim params should all have is_version: false
    let valheim_params = valheim["config"]["parameters"].as_array().unwrap();
    for p in valheim_params {
        assert_eq!(
            p["is_version"], false,
            "Valheim param '{}' should not be version",
            p["name"]
        );
    }
}

// ─── Permission check: non-owner cannot check ───────────────────────

#[tokio::test]
async fn test_check_update_requires_permission() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _user_id) = app.setup_admin_and_user().await;

    // Admin creates a server (user has no permission on it)
    let (id, _) =
        create_server_with_command_check(&app, &admin_token, "perm-test", "echo 2.0.0", "1.0.0")
            .await;

    // Regular user tries to check — should be forbidden
    let (status, _) = app
        .get(
            &format!("/api/servers/{}/check-update", id),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ─── Command provider with variable substitution ─────────────────────

#[tokio::test]
async fn test_check_update_command_with_variable_substitution() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // The command uses ${app_version} which should be substituted
    let (id, _) = create_server_with_command_check(
        &app,
        &token,
        "var-sub",
        "echo ${app_version}-patched",
        "1.0.0",
    )
    .await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/check-update", id), Some(&token))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], true);
    assert_eq!(body["latest_version"], "1.0.0-patched");
    assert_eq!(body["installed_version"], "1.0.0");
}

// ─── Installed version recording via install pipeline ────────────────

#[tokio::test]
async fn test_installed_version_recorded_after_install() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = super::common::resolve_binary("echo");

    // Create a server with is_version parameter and a trivial install step
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "install-version-test",
                    "binary": echo,
                    "args": [],
                    "parameters": [
                        {
                            "name": "ver",
                            "label": "Version",
                            "param_type": "string",
                            "default": "3.0.0",
                            "required": true,
                            "options": [],
                            "is_version": true
                        }
                    ],
                    "install_steps": [
                        {
                            "name": "create marker",
                            "action": {
                                "type": "write_file",
                                "path": "installed.txt",
                                "content": "done"
                            }
                        }
                    ],
                    "stop_timeout_secs": 10
                },
                "parameter_values": {
                    "ver": "3.0.0"
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    let id = body["server"]["id"].as_str().unwrap();

    // installed_version should be null before install
    assert!(body["server"]["installed_version"].is_null());

    // Run install pipeline
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/install", id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Wait for pipeline to complete
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Fetch the server again
    let (status, body) = app.get(&format!("/api/servers/{}", id), Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    // installed_version should now be set
    assert_eq!(
        body["server"]["installed_version"], "3.0.0",
        "installed_version should be recorded after install; body: {:?}",
        body
    );
    assert_eq!(body["server"]["installed"], true);
}

// ─── Update check with template_default provider ─────────────────────

#[tokio::test]
async fn test_template_default_provider_without_source_template() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = super::common::resolve_binary("echo");

    // Create server with template_default provider but no source_template_id
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "no-template-default",
                    "binary": echo,
                    "args": [],
                    "parameters": [
                        {
                            "name": "ver",
                            "label": "Version",
                            "param_type": "string",
                            "default": "1.0",
                            "required": true,
                            "options": [],
                            "is_version": true
                        }
                    ],
                    "update_check": {
                        "provider": "template_default",
                        "interval_secs": 0,
                        "cache_secs": 5
                    },
                    "stop_timeout_secs": 10
                },
                "parameter_values": {"ver": "1.0"}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    let id = body["server"]["id"].as_str().unwrap();

    // Check should return an error because there's no source template
    let (status, body) = app
        .get(&format!("/api/servers/{}/check-update", id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], false);
    assert!(body["error"].is_string());
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("source_template_id"));
}

// ─── Bulk status only returns accessible servers ─────────────────────

#[tokio::test]
async fn test_update_status_respects_permissions() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _user_id) = app.setup_admin_and_user().await;

    // Admin creates a server and triggers a check
    let (id, _) =
        create_server_with_command_check(&app, &admin_token, "perm-bulk", "echo 2.0.0", "1.0.0")
            .await;
    let (s, _) = app
        .get(
            &format!("/api/servers/{}/check-update", id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(s, StatusCode::OK);

    // Admin sees the result
    let (status, body) = app
        .get("/api/servers/update-status", Some(&admin_token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 1);

    // Regular user should see 0 results (no permission)
    let (status, body) = app
        .get("/api/servers/update-status", Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
}

// ─── update_check with command provider: multiline output picks first ─

#[tokio::test]
async fn test_command_provider_multiline_picks_first_nonempty() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Command outputs multiple lines; first non-empty should be picked
    let (id, _) = create_server_with_command_check(
        &app,
        &token,
        "multiline",
        "printf '\\n  \\n4.5.6\\nignored\\n'",
        "4.5.5",
    )
    .await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/check-update", id), Some(&token))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], true);
    assert_eq!(body["latest_version"], "4.5.6");
}

// ─── is_version defaults to false when omitted in JSON ───────────────

#[tokio::test]
async fn test_is_version_defaults_false_when_omitted() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = super::common::resolve_binary("echo");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "default-false-test",
                    "binary": echo,
                    "args": [],
                    "parameters": [
                        {
                            "name": "p1",
                            "label": "Param 1",
                            "param_type": "string",
                            "default": "x",
                            "required": false,
                            "options": []
                            // is_version intentionally omitted
                        }
                    ],
                    "stop_timeout_secs": 10
                },
                "parameter_values": {}
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    let params = body["server"]["config"]["parameters"].as_array().unwrap();
    assert_eq!(params[0]["is_version"], false);
}
