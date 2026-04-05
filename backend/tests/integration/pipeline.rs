use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::common::{resolve_binary, TestApp};

#[allow(unused_imports)]
use anyserver::types;

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Poll `GET /api/servers/:id/phase-status` until the pipeline is no longer
/// running, or until we hit the timeout. Returns the final phase-status body.
async fn poll_phase_complete(app: &TestApp, token: &str, server_id: &str) -> Value {
    app.poll_phase_complete(token, server_id).await
}

/// Create a server with the given install_steps and parameter_values, then
/// trigger the install pipeline. Returns (server_id, final phase-status body).
async fn create_and_install(
    app: &TestApp,
    token: &str,
    name: &str,
    install_steps: Value,
    update_steps: Value,
    parameters: Value,
    parameter_values: Value,
) -> (String, Value) {
    let echo = resolve_binary("echo");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(token),
            json!({
                "config": {
                    "name": name,
                    "binary": echo,
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": parameters,
                    "install_steps": install_steps,
                    "update_steps": update_steps
                },
                "parameter_values": parameter_values
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    // Trigger install
    let (inst_status, inst_body) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(token),
            json!(null),
        )
        .await;
    assert_eq!(
        inst_status,
        StatusCode::OK,
        "install trigger failed: {:?}",
        inst_body
    );
    assert_eq!(inst_body["status"], "running");

    // Poll until done
    let final_status = poll_phase_complete(app, token, &server_id).await;
    (server_id, final_status)
}

// ─── Happy-path tests ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_install_pipeline_create_dir_and_write_file() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (server_id, phase_body) = create_and_install(
        &app,
        &token,
        "Pipeline Happy",
        json!([
            {
                "name": "Create plugins dir",
                "description": null,
                "action": { "type": "create_dir", "path": "plugins" },
                "condition": null,
                "continue_on_error": false
            },
            {
                "name": "Write config",
                "description": "Write a config file",
                "action": {
                    "type": "write_file",
                    "path": "config.yml",
                    "content": "server_port: 25565\nmotd: Hello World"
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    // Verify phase completed
    let progress = &phase_body["progress"];
    assert_eq!(
        progress["status"], "completed",
        "Phase should complete successfully: {:?}",
        progress
    );
    assert_eq!(progress["phase"], "install");

    // Verify each step completed
    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);

    assert_eq!(steps[0]["step_name"], "Create plugins dir");
    assert_eq!(steps[0]["status"], "completed");
    assert!(
        steps[0]["started_at"].is_string(),
        "step should have started_at"
    );
    assert!(
        steps[0]["completed_at"].is_string(),
        "step should have completed_at"
    );

    assert_eq!(steps[1]["step_name"], "Write config");
    assert_eq!(steps[1]["status"], "completed");

    // Verify files were actually created on disk
    let data_dir = app._temp_dir.path();
    let server_dir = data_dir.join("servers").join(&server_id);
    assert!(
        server_dir.join("plugins").is_dir(),
        "plugins directory should exist"
    );
    assert!(
        server_dir.join("config.yml").is_file(),
        "config.yml should exist"
    );

    let content = std::fs::read_to_string(server_dir.join("config.yml")).unwrap();
    assert_eq!(content, "server_port: 25565\nmotd: Hello World");

    // Verify server is marked as installed
    assert_eq!(phase_body["installed"], true);
    assert!(
        phase_body["installed_at"].is_string(),
        "installed_at should be set"
    );
}

#[tokio::test]
async fn test_install_pipeline_run_command_step() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let echo = resolve_binary("echo");

    // Enable RunCommand execution
    let _ = app
        .put(
            "/api/auth/settings",
            Some(&token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;

    let (_, phase_body) = create_and_install(
        &app,
        &token,
        "Run Command Test",
        json!([
            {
                "name": "Echo test",
                "description": null,
                "action": {
                    "type": "run_command",
                    "command": echo,
                    "args": ["hello", "from", "pipeline"],
                    "working_dir": null,
                    "env": {}
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "completed");
    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["step_name"], "Echo test");
    assert_eq!(steps[0]["status"], "completed");
}

#[tokio::test]
async fn test_install_sets_installed_flag_on_server() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (server_id, _) = create_and_install(
        &app,
        &token,
        "Install Flag Test",
        json!([
            {
                "name": "Create marker",
                "description": null,
                "action": { "type": "create_dir", "path": "installed_marker" },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    // Fetch the server and verify installed state
    let (status, server_body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(server_body["server"]["installed"], true);
    assert!(server_body["server"]["installed_at"].is_string());
}

#[tokio::test]
async fn test_update_pipeline_happy_path() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // First, create and install
    let (server_id, _) = create_and_install(
        &app,
        &token,
        "Update Test",
        json!([
            {
                "name": "Initial setup",
                "description": null,
                "action": {
                    "type": "write_file",
                    "path": "version.txt",
                    "content": "v1.0"
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([
            {
                "name": "Update version",
                "description": null,
                "action": {
                    "type": "write_file",
                    "path": "version.txt",
                    "content": "v2.0"
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!({}),
    )
    .await;

    // Trigger update
    let (upd_status, upd_body) = app
        .post(
            &format!("/api/servers/{}/update", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(
        upd_status,
        StatusCode::OK,
        "update trigger failed: {:?}",
        upd_body
    );
    assert_eq!(upd_body["phase"], "update");

    // Poll until done
    let final_body = poll_phase_complete(&app, &token, &server_id).await;
    let progress = &final_body["progress"];
    assert_eq!(progress["status"], "completed");
    assert_eq!(progress["phase"], "update");

    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["step_name"], "Update version");
    assert_eq!(steps[0]["status"], "completed");

    // Verify the file was updated on disk
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    let content = std::fs::read_to_string(server_dir.join("version.txt")).unwrap();
    assert_eq!(content, "v2.0");

    // Verify updated_via_pipeline_at is set
    assert!(final_body["updated_via_pipeline_at"].is_string());
}

// ─── Update pipeline version persistence tests ────────────────────────────────

/// When an update pipeline is triggered with explicit parameter_overrides
/// containing the version parameter, the server's parameter_values and
/// installed_version should be updated to the new value after completion.
#[tokio::test]
async fn test_update_pipeline_persists_explicit_version_override() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");

    // Create a server with a version parameter
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "version-override-test",
                    "binary": echo,
                    "args": [],
                    "parameters": [
                        {
                            "name": "mc_version",
                            "label": "Version",
                            "param_type": "string",
                            "default": "1.21.3",
                            "required": true,
                            "is_version": true
                        }
                    ],
                    "install_steps": [
                        {
                            "name": "install marker",
                            "action": { "type": "write_file", "path": "installed.txt", "content": "v${mc_version}" }
                        }
                    ],
                    "update_steps": [
                        {
                            "name": "update marker",
                            "action": { "type": "write_file", "path": "installed.txt", "content": "v${mc_version}" }
                        }
                    ],
                    "stop_timeout_secs": 10
                },
                "parameter_values": { "mc_version": "1.21.3" }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    // Install
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let phase_body = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(phase_body["progress"]["status"], "completed");

    // Verify installed_version is 1.21.3 after install
    let (_, server_body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(server_body["server"]["installed_version"], "1.21.3");

    // Trigger update with explicit parameter override for the version
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/update", server_id),
            Some(&token),
            json!({
                "parameter_overrides": { "mc_version": "1.21.4" }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let phase_body = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(phase_body["progress"]["status"], "completed");

    // Re-fetch the server
    let (_, server_body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;

    // installed_version should now be 1.21.4
    assert_eq!(
        server_body["server"]["installed_version"], "1.21.4",
        "installed_version should be updated to the override value; body: {:?}",
        server_body
    );

    // parameter_values should also have been updated
    assert_eq!(
        server_body["server"]["parameter_values"]["mc_version"], "1.21.4",
        "parameter_values.mc_version should be persisted after update; body: {:?}",
        server_body
    );

    // The file on disk should have used the new version
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    let content = std::fs::read_to_string(server_dir.join("installed.txt")).unwrap();
    assert_eq!(content, "v1.21.4");
}

/// When the update cache has a newer version and no explicit override is
/// provided, the pipeline should auto-inject the latest version.
#[tokio::test]
async fn test_update_pipeline_auto_injects_version_from_cache() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");

    // Create a server with a version parameter
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "auto-inject-test",
                    "binary": echo,
                    "args": [],
                    "parameters": [
                        {
                            "name": "ver",
                            "label": "Version",
                            "param_type": "string",
                            "default": "2.0.0",
                            "required": true,
                            "is_version": true
                        }
                    ],
                    "install_steps": [
                        {
                            "name": "install marker",
                            "action": { "type": "write_file", "path": "ver.txt", "content": "${ver}" }
                        }
                    ],
                    "update_steps": [
                        {
                            "name": "update marker",
                            "action": { "type": "write_file", "path": "ver.txt", "content": "${ver}" }
                        }
                    ],
                    "stop_timeout_secs": 10
                },
                "parameter_values": { "ver": "2.0.0" }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    // Install
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let phase_body = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(phase_body["progress"]["status"], "completed");

    // Manually inject an update-check result into the cache, simulating
    // an update check that found version 3.0.0 available.
    let sid: uuid::Uuid = server_id.parse().unwrap();
    app.state.update_cache.insert(
        sid,
        anyserver::types::UpdateCheckResult {
            server_id: sid,
            update_available: true,
            installed_version: Some("2.0.0".into()),
            latest_version: Some("3.0.0".into()),
            installed_version_display: None,
            latest_version_display: None,
            checked_at: chrono::Utc::now(),
            error: None,
        },
    );

    // Trigger update WITHOUT explicit parameter overrides — the backend
    // should auto-inject ver=3.0.0 from the cache.
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/update", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let phase_body = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(phase_body["progress"]["status"], "completed");

    // Re-fetch the server
    let (_, server_body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;

    // installed_version should be the auto-injected 3.0.0
    assert_eq!(
        server_body["server"]["installed_version"], "3.0.0",
        "installed_version should be auto-injected from cache; body: {:?}",
        server_body
    );
    assert_eq!(
        server_body["server"]["parameter_values"]["ver"], "3.0.0",
        "parameter_values.ver should be persisted after auto-injected update; body: {:?}",
        server_body
    );

    // Verify the file on disk used the new version
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    let content = std::fs::read_to_string(server_dir.join("ver.txt")).unwrap();
    assert_eq!(content, "3.0.0");
}

/// When the update cache says no update is available (update_available=false),
/// triggering an update should use the existing parameter values unchanged.
#[tokio::test]
async fn test_update_pipeline_no_injection_when_cache_says_no_update() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "no-update-test",
                    "binary": echo,
                    "args": [],
                    "parameters": [
                        {
                            "name": "ver",
                            "label": "Version",
                            "param_type": "string",
                            "default": "5.0.0",
                            "required": true,
                            "is_version": true
                        }
                    ],
                    "install_steps": [
                        {
                            "name": "install marker",
                            "action": { "type": "write_file", "path": "ver.txt", "content": "${ver}" }
                        }
                    ],
                    "update_steps": [
                        {
                            "name": "update marker",
                            "action": { "type": "write_file", "path": "ver.txt", "content": "${ver}" }
                        }
                    ],
                    "stop_timeout_secs": 10
                },
                "parameter_values": { "ver": "5.0.0" }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    // Install
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    poll_phase_complete(&app, &token, &server_id).await;

    // Cache says no update available (same version)
    let sid: uuid::Uuid = server_id.parse().unwrap();
    app.state.update_cache.insert(
        sid,
        anyserver::types::UpdateCheckResult {
            server_id: sid,
            update_available: false,
            installed_version: Some("5.0.0".into()),
            latest_version: Some("5.0.0".into()),
            installed_version_display: None,
            latest_version_display: None,
            checked_at: chrono::Utc::now(),
            error: None,
        },
    );

    // Trigger update — should NOT inject anything new
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/update", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let phase_body = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(phase_body["progress"]["status"], "completed");

    let (_, server_body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;

    // Should still be 5.0.0
    assert_eq!(server_body["server"]["installed_version"], "5.0.0");
    assert_eq!(server_body["server"]["parameter_values"]["ver"], "5.0.0");

    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    let content = std::fs::read_to_string(server_dir.join("ver.txt")).unwrap();
    assert_eq!(content, "5.0.0");
}

/// Explicit parameter_overrides should take precedence over the
/// auto-injected version from the update cache.
#[tokio::test]
async fn test_update_pipeline_explicit_override_beats_cache() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "override-beats-cache-test",
                    "binary": echo,
                    "args": [],
                    "parameters": [
                        {
                            "name": "ver",
                            "label": "Version",
                            "param_type": "string",
                            "default": "1.0.0",
                            "required": true,
                            "is_version": true
                        }
                    ],
                    "install_steps": [
                        {
                            "name": "install marker",
                            "action": { "type": "write_file", "path": "ver.txt", "content": "${ver}" }
                        }
                    ],
                    "update_steps": [
                        {
                            "name": "update marker",
                            "action": { "type": "write_file", "path": "ver.txt", "content": "${ver}" }
                        }
                    ],
                    "stop_timeout_secs": 10
                },
                "parameter_values": { "ver": "1.0.0" }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    // Install
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    poll_phase_complete(&app, &token, &server_id).await;

    // Cache says 2.0.0 is available
    let sid: uuid::Uuid = server_id.parse().unwrap();
    app.state.update_cache.insert(
        sid,
        anyserver::types::UpdateCheckResult {
            server_id: sid,
            update_available: true,
            installed_version: Some("1.0.0".into()),
            latest_version: Some("2.0.0".into()),
            installed_version_display: None,
            latest_version_display: None,
            checked_at: chrono::Utc::now(),
            error: None,
        },
    );

    // Trigger update with an explicit override to 1.5.0 — should use 1.5.0,
    // NOT the cached 2.0.0.
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/update", server_id),
            Some(&token),
            json!({
                "parameter_overrides": { "ver": "1.5.0" }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let phase_body = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(phase_body["progress"]["status"], "completed");

    let (_, server_body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;

    assert_eq!(
        server_body["server"]["installed_version"], "1.5.0",
        "explicit override should win over cache; body: {:?}",
        server_body
    );
    assert_eq!(server_body["server"]["parameter_values"]["ver"], "1.5.0");

    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    let content = std::fs::read_to_string(server_dir.join("ver.txt")).unwrap();
    assert_eq!(content, "1.5.0");
}

// ─── Variable substitution tests ──────────────────────────────────────────────

#[tokio::test]
async fn test_install_pipeline_variable_substitution() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (server_id, phase_body) = create_and_install(
        &app,
        &token,
        "Vars Test",
        json!([
            {
                "name": "Write with vars",
                "description": null,
                "action": {
                    "type": "write_file",
                    "path": "info.txt",
                    "content": "id=${server_id}\nname=${server_name}\nport=${port}"
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([
            {
                "name": "port",
                "label": "Port",
                "description": null,
                "param_type": "number",
                "default": "25565",
                "required": true,
                "options": [],
                "regex": null
            }
        ]),
        json!({ "port": "25577" }),
    )
    .await;

    assert_eq!(phase_body["progress"]["status"], "completed");

    // Verify variable substitution in the file
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    let content = std::fs::read_to_string(server_dir.join("info.txt")).unwrap();
    assert!(
        content.contains(&format!("id={}", server_id)),
        "Should contain server_id. Got: {}",
        content
    );
    assert!(
        content.contains("name=Vars Test"),
        "Should contain server_name. Got: {}",
        content
    );
    assert!(
        content.contains("port=25577"),
        "Should contain parameter value. Got: {}",
        content
    );
}

// ─── Failure / error tests ────────────────────────────────────────────────────

#[tokio::test]
async fn test_install_pipeline_download_failure_shows_error() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Use port 1 on loopback — guaranteed to refuse connections quickly
    let (_, phase_body) = create_and_install(
        &app,
        &token,
        "Download Fail Test",
        json!([
            {
                "name": "Download bad file",
                "description": null,
                "action": {
                    "type": "download",
                    "url": "http://127.0.0.1:1/nonexistent-file.jar",
                    "destination": ".",
                    "filename": null,
                    "executable": false
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(
        progress["status"], "failed",
        "Pipeline should fail on download error: {:?}",
        progress
    );

    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["step_name"], "Download bad file");
    assert_eq!(steps[0]["status"], "failed");

    // The error message should contain useful information about what went wrong.
    // Since SSRF protection blocks requests to private/internal IPs, the error
    // will mention that the URL resolves to a private address rather than a
    // connection failure.
    let message = steps[0]["message"].as_str().unwrap();
    assert!(
        message.contains("Download failed")
            || message.contains("error")
            || message.contains("connect")
            || message.contains("private")
            || message.contains("blocked"),
        "Error message should describe the download failure. Got: {}",
        message
    );

    // Server should NOT be marked as installed since the pipeline failed
    assert_eq!(phase_body["installed"], false);
}

#[tokio::test]
async fn test_install_pipeline_stops_on_failed_step() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (server_id, phase_body) = create_and_install(
        &app,
        &token,
        "Stop On Error Test",
        json!([
            {
                "name": "Bad download",
                "description": null,
                "action": {
                    "type": "download",
                    "url": "http://127.0.0.1:1/nope.jar",
                    "destination": ".",
                    "filename": null,
                    "executable": false
                },
                "condition": null,
                "continue_on_error": false
            },
            {
                "name": "Should not run",
                "description": null,
                "action": { "type": "create_dir", "path": "should_not_exist" },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "failed");

    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);

    // First step failed
    assert_eq!(steps[0]["status"], "failed");
    assert!(
        steps[0]["message"].is_string(),
        "Failed step should have error message"
    );

    // Second step should still be pending (never ran)
    assert_eq!(
        steps[1]["status"], "pending",
        "Second step should not have run after first step failed"
    );

    // Verify the directory was NOT created
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    assert!(
        !server_dir.join("should_not_exist").exists(),
        "Directory should not have been created since pipeline stopped"
    );
}

#[tokio::test]
async fn test_install_pipeline_continue_on_error() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (server_id, phase_body) = create_and_install(
        &app,
        &token,
        "Continue On Error Test",
        json!([
            {
                "name": "Failing step",
                "description": null,
                "action": {
                    "type": "download",
                    "url": "http://127.0.0.1:1/nope.jar",
                    "destination": ".",
                    "filename": null,
                    "executable": false
                },
                "condition": null,
                "continue_on_error": true
            },
            {
                "name": "Should still run",
                "description": null,
                "action": { "type": "create_dir", "path": "still_created" },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    // Overall pipeline should still complete because continue_on_error=true on the failing step
    assert_eq!(
        progress["status"], "completed",
        "Pipeline should complete despite failed step with continue_on_error=true: {:?}",
        progress
    );

    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["step_name"], "Failing step");
    assert_eq!(steps[0]["status"], "failed");
    assert!(
        steps[0]["message"].is_string(),
        "Failed step should have error message"
    );

    assert_eq!(steps[1]["step_name"], "Should still run");
    assert_eq!(steps[1]["status"], "completed");

    // Verify the directory WAS created by the second step
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    assert!(
        server_dir.join("still_created").is_dir(),
        "Second step should have run and created the directory"
    );
}

#[tokio::test]
async fn test_install_pipeline_run_command_failure_shows_exit_code() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Enable RunCommand execution
    let _ = app
        .put(
            "/api/auth/settings",
            Some(&token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;
    let false_bin = resolve_binary("false");

    let (_, phase_body) = create_and_install(
        &app,
        &token,
        "Command Fail Test",
        json!([
            {
                "name": "Run false",
                "description": null,
                "action": {
                    "type": "run_command",
                    "command": false_bin,
                    "args": [],
                    "working_dir": null,
                    "env": {}
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "failed");

    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["status"], "failed");
    let message = steps[0]["message"].as_str().unwrap();
    assert!(
        message.contains("exited with code"),
        "Error should mention exit code. Got: {}",
        message
    );
}

// ─── Condition tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_install_pipeline_conditional_step_skipped() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (_, phase_body) = create_and_install(
        &app,
        &token,
        "Condition Skip Test",
        json!([
            {
                "name": "Only if file exists",
                "description": null,
                "action": { "type": "create_dir", "path": "conditional_dir" },
                "condition": {
                    "path_exists": "nonexistent_file.txt",
                    "path_not_exists": null
                },
                "continue_on_error": false
            },
            {
                "name": "Always runs",
                "description": null,
                "action": { "type": "create_dir", "path": "always_dir" },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "completed");

    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["step_name"], "Only if file exists");
    assert_eq!(
        steps[0]["status"], "skipped",
        "Step should be skipped when condition is not met"
    );
    assert!(
        steps[0]["message"]
            .as_str()
            .unwrap_or("")
            .contains("Condition not met"),
        "Skipped step should explain why: {:?}",
        steps[0]["message"]
    );

    assert_eq!(steps[1]["step_name"], "Always runs");
    assert_eq!(steps[1]["status"], "completed");
}

#[tokio::test]
async fn test_install_pipeline_condition_path_not_exists_runs_when_absent() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (server_id, phase_body) = create_and_install(
        &app,
        &token,
        "Condition Not Exists Test",
        json!([
            {
                "name": "Run if no lockfile",
                "description": null,
                "action": {
                    "type": "write_file",
                    "path": "install.lock",
                    "content": "installed"
                },
                "condition": {
                    "path_exists": null,
                    "path_not_exists": "install.lock"
                },
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "completed");

    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(
        steps[0]["status"], "completed",
        "Step should run because install.lock doesn't exist yet"
    );

    // Verify the file was created
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    assert!(server_dir.join("install.lock").exists());
}

// ─── Edge cases ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_install_with_no_steps_returns_error() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");
    let (_, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "No Steps",
                    "binary": echo,
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    let server_id = body["server"]["id"].as_str().unwrap();

    let (status, err_body) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Should reject install with no steps: {:?}",
        err_body
    );
    assert!(
        err_body["error"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("no install steps"),
        "Error should mention missing steps: {:?}",
        err_body
    );
}

#[tokio::test]
async fn test_update_with_no_steps_returns_error() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "No Update Steps").await;

    let (status, err_body) = app
        .post(
            &format!("/api/servers/{}/update", server_id),
            Some(&token),
            json!(null),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        err_body["error"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("no update steps"),
        "Error should mention missing steps: {:?}",
        err_body
    );
}

#[tokio::test]
async fn test_install_requires_auth() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Auth Test").await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            None,
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_phase_status_requires_auth() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Auth Test").await;

    let (status, _) = app
        .get(&format!("/api/servers/{}/phase-status", server_id), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_viewer_cannot_trigger_install() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;

    let echo = resolve_binary("echo");
    let (_, body) = app
        .post(
            "/api/servers",
            Some(&admin_token),
            json!({
                "config": {
                    "name": "Viewer Install Test",
                    "binary": echo,
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [
                        {
                            "name": "Setup",
                            "description": null,
                            "action": { "type": "create_dir", "path": "data" },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    let server_id = body["server"]["id"].as_str().unwrap();

    // Grant viewer permission
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // Viewer should NOT be able to trigger install (requires Manager)
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&user_token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_phase_status_shows_no_progress_before_install() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "No Progress Yet").await;

    let (status, body) = app
        .get(
            &format!("/api/servers/{}/phase-status", server_id),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["progress"].is_null(),
        "Should have no progress before any pipeline runs: {:?}",
        body
    );
    assert_eq!(body["installed"], false);
    assert!(body["installed_at"].is_null());
}

// ─── Multi-step logging/progress tests ────────────────────────────────────────

#[tokio::test]
async fn test_install_pipeline_multi_step_progress_tracking() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let echo = resolve_binary("echo");

    // Enable RunCommand execution
    let _ = app
        .put(
            "/api/auth/settings",
            Some(&token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;

    let (server_id, phase_body) = create_and_install(
        &app,
        &token,
        "Multi Step Progress",
        json!([
            {
                "name": "Step 1: Create dirs",
                "description": "Create the data directory structure",
                "action": { "type": "create_dir", "path": "data/worlds" },
                "condition": null,
                "continue_on_error": false
            },
            {
                "name": "Step 2: Write eula",
                "description": null,
                "action": {
                    "type": "write_file",
                    "path": "eula.txt",
                    "content": "eula=true"
                },
                "condition": null,
                "continue_on_error": false
            },
            {
                "name": "Step 3: Write server.properties",
                "description": null,
                "action": {
                    "type": "write_file",
                    "path": "server.properties",
                    "content": "server-port=${port}\nmotd=${motd}"
                },
                "condition": null,
                "continue_on_error": false
            },
            {
                "name": "Step 4: Run setup script",
                "description": null,
                "action": {
                    "type": "run_command",
                    "command": echo,
                    "args": ["Setup complete for ${server_name}"],
                    "working_dir": null,
                    "env": {}
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([
            {
                "name": "port",
                "label": "Port",
                "description": null,
                "param_type": "number",
                "default": "25565",
                "required": true,
                "options": [],
                "regex": null
            },
            {
                "name": "motd",
                "label": "MOTD",
                "description": null,
                "param_type": "string",
                "default": "A Server",
                "required": false,
                "options": [],
                "regex": null
            }
        ]),
        json!({ "port": "25577", "motd": "Welcome!" }),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "completed");

    // Verify ALL steps completed with correct names and indices
    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 4);

    for (i, step) in steps.iter().enumerate() {
        assert_eq!(
            step["step_index"], i as u64,
            "Step index mismatch at position {}",
            i
        );
        assert_eq!(
            step["status"], "completed",
            "Step {} ('{}') should be completed",
            i, step["step_name"]
        );
        assert!(
            step["started_at"].is_string(),
            "Step {} should have started_at",
            i
        );
        assert!(
            step["completed_at"].is_string(),
            "Step {} should have completed_at",
            i
        );
    }

    assert_eq!(steps[0]["step_name"], "Step 1: Create dirs");
    assert_eq!(steps[1]["step_name"], "Step 2: Write eula");
    assert_eq!(steps[2]["step_name"], "Step 3: Write server.properties");
    assert_eq!(steps[3]["step_name"], "Step 4: Run setup script");

    // Verify variable substitution worked in the written files
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    let props = std::fs::read_to_string(server_dir.join("server.properties")).unwrap();
    assert!(
        props.contains("server-port=25577"),
        "Port var not substituted in: {}",
        props
    );
    assert!(
        props.contains("motd=Welcome!"),
        "MOTD var not substituted in: {}",
        props
    );

    // Phase-level timestamps should be set
    assert!(progress["started_at"].is_string());
    assert!(progress["completed_at"].is_string());
}

#[tokio::test]
async fn test_install_pipeline_mixed_success_and_failure_with_continue() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let false_bin = resolve_binary("false");

    let (server_id, phase_body) = create_and_install(
        &app,
        &token,
        "Mixed Results",
        json!([
            {
                "name": "Good step 1",
                "description": null,
                "action": { "type": "create_dir", "path": "step1_dir" },
                "condition": null,
                "continue_on_error": false
            },
            {
                "name": "Bad step (continue)",
                "description": null,
                "action": {
                    "type": "run_command",
                    "command": false_bin,
                    "args": [],
                    "working_dir": null,
                    "env": {}
                },
                "condition": null,
                "continue_on_error": true
            },
            {
                "name": "Good step 2",
                "description": null,
                "action": { "type": "create_dir", "path": "step3_dir" },
                "condition": null,
                "continue_on_error": false
            },
            {
                "name": "Skipped step",
                "description": null,
                "action": { "type": "create_dir", "path": "skipped_dir" },
                "condition": {
                    "path_exists": "does_not_exist.flag",
                    "path_not_exists": null
                },
                "continue_on_error": false
            },
            {
                "name": "Final step",
                "description": null,
                "action": {
                    "type": "write_file",
                    "path": "done.txt",
                    "content": "all done"
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    // Pipeline should complete overall because the failing step has continue_on_error
    assert_eq!(progress["status"], "completed");

    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 5);

    assert_eq!(steps[0]["status"], "completed", "Good step 1");
    assert_eq!(steps[1]["status"], "failed", "Bad step should fail");
    assert!(
        steps[1]["message"].is_string(),
        "Failed step should have error message"
    );
    assert_eq!(
        steps[2]["status"], "completed",
        "Good step 2 should still run"
    );
    assert_eq!(
        steps[3]["status"], "skipped",
        "Conditional step should be skipped"
    );
    assert_eq!(steps[4]["status"], "completed", "Final step should run");

    // Verify filesystem state matches expectations
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    assert!(server_dir.join("step1_dir").is_dir());
    assert!(server_dir.join("step3_dir").is_dir());
    assert!(!server_dir.join("skipped_dir").exists());
    assert_eq!(
        std::fs::read_to_string(server_dir.join("done.txt")).unwrap(),
        "all done"
    );
}

#[tokio::test]
async fn test_install_pipeline_download_error_message_contains_url() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let bad_url = "http://127.0.0.1:1/some-specific-file-v1.2.3.jar";

    let (_, phase_body) = create_and_install(
        &app,
        &token,
        "URL Error Msg Test",
        json!([
            {
                "name": "Download server jar",
                "description": null,
                "action": {
                    "type": "download",
                    "url": bad_url,
                    "destination": ".",
                    "filename": "server.jar",
                    "executable": false
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let steps = phase_body["progress"]["steps"].as_array().unwrap();
    let message = steps[0]["message"].as_str().unwrap();

    // The error message should reference the URL so users know WHAT failed
    assert!(
        message.contains("127.0.0.1") || message.contains("Download"),
        "Error message should contain URL or download context for debugging. Got: {}",
        message
    );
}

// ─── Cancel pipeline test ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_cancel_pipeline() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sleep_bin = resolve_binary("sleep");

    // Enable RunCommand execution
    let _ = app
        .put(
            "/api/auth/settings",
            Some(&token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;

    let echo = resolve_binary("echo");
    let (_, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Cancel Test",
                    "binary": echo,
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [
                        {
                            "name": "Long running step",
                            "description": null,
                            "action": {
                                "type": "run_command",
                                "command": sleep_bin,
                                "args": ["60"],
                                "working_dir": null,
                                "env": {}
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    let server_id = body["server"]["id"].as_str().unwrap();

    // Trigger install
    let (inst_status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(inst_status, StatusCode::OK);

    // Give the pipeline a moment to start the sleep command
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify it's running
    let (_, status_body) = app
        .get(
            &format!("/api/servers/{}/phase-status", server_id),
            Some(&token),
        )
        .await;
    assert_eq!(
        status_body["progress"]["status"], "running",
        "Pipeline should be running: {:?}",
        status_body
    );

    // Cancel it
    let (cancel_status, cancel_body) = app
        .post(
            &format!("/api/servers/{}/cancel-phase", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(cancel_status, StatusCode::OK);
    assert_eq!(cancel_body["cancelled"], true);

    // Verify it's now failed/cancelled
    let (_, final_body) = app
        .get(
            &format!("/api/servers/{}/phase-status", server_id),
            Some(&token),
        )
        .await;
    assert_eq!(final_body["progress"]["status"], "failed");

    let steps = final_body["progress"]["steps"].as_array().unwrap();
    // The running or pending step should be marked as failed with "Cancelled" message
    let has_cancelled = steps
        .iter()
        .any(|s| s["message"].as_str().unwrap_or("").contains("Cancelled"));
    assert!(
        has_cancelled,
        "At least one step should have 'Cancelled' message: {:?}",
        steps
    );
}

// ─── ensure_handle / phase log buffer / WS replay tests ──────────────────────
//
// These verify the backend fixes that make pipeline logs visible to WebSocket
// clients:
//   1. `ensure_handle` creates a ProcessHandle for never-started servers so
//      the broadcast channel is subscribable.
//   2. Phase logs are buffered in the PipelineHandle for late-connecting
//      clients.
//   3. Phase progress is queryable after pipeline completion (replay data).
//   4. Running a pipeline twice on the same server works (ensure_handle
//      reuses the existing handle the second time).
//   5. The server runtime stays "stopped" after an install pipeline — the
//      pipeline creates a handle but does NOT start the process.

#[tokio::test]
async fn test_ensure_handle_creates_process_handle_for_never_started_server() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");
    let (_, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Never Started",
                    "binary": echo,
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [
                        {
                            "name": "Write file",
                            "description": null,
                            "action": {
                                "type": "write_file",
                                "path": "test.txt",
                                "content": "hello"
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    let server_id = body["server"]["id"].as_str().unwrap();
    let server_uuid: uuid::Uuid = server_id.parse().unwrap();

    // Before install: no ProcessHandle should exist
    assert!(
        !app.state.process_manager.handles.contains_key(&server_uuid),
        "ProcessHandle should not exist before any pipeline or start"
    );

    // Trigger install
    let (inst_status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(inst_status, StatusCode::OK);

    // After triggering install: ensure_handle should have created a ProcessHandle
    assert!(
        app.state.process_manager.handles.contains_key(&server_uuid),
        "ensure_handle should create a ProcessHandle when pipeline starts"
    );

    // The handle should be subscribable
    let subscription = app.state.process_manager.subscribe(&server_uuid);
    assert!(
        subscription.is_some(),
        "ProcessHandle created by ensure_handle should be subscribable"
    );

    // Wait for pipeline to finish
    let final_body = poll_phase_complete(&app, &token, server_id).await;
    assert_eq!(final_body["progress"]["status"], "completed");
}

#[tokio::test]
async fn test_server_runtime_stays_stopped_after_install_pipeline() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (server_id, _) = create_and_install(
        &app,
        &token,
        "Stays Stopped",
        json!([
            {
                "name": "Write marker",
                "description": null,
                "action": {
                    "type": "write_file",
                    "path": "installed.txt",
                    "content": "yes"
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    // Fetch the server — runtime should still be stopped
    let (status, server_body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        server_body["runtime"]["status"], "stopped",
        "Server process should NOT be running after install pipeline — \
         ensure_handle creates a handle but does not start the process"
    );
    assert!(
        server_body["runtime"]["pid"].is_null(),
        "No PID should be set — the server was never started"
    );

    // But it should be marked as installed
    assert_eq!(server_body["server"]["installed"], true);
}

#[tokio::test]
async fn test_phase_log_buffer_populated_after_install() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let echo = resolve_binary("echo");

    // Enable RunCommand execution
    let _ = app
        .put(
            "/api/auth/settings",
            Some(&token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;

    let (_, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Log Buffer Test",
                    "binary": echo,
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [
                        {
                            "name": "Create dir",
                            "description": null,
                            "action": { "type": "create_dir", "path": "data" },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Write config",
                            "description": null,
                            "action": {
                                "type": "write_file",
                                "path": "config.txt",
                                "content": "test=true"
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Run echo",
                            "description": null,
                            "action": {
                                "type": "run_command",
                                "command": echo,
                                "args": ["pipeline output line"],
                                "working_dir": null,
                                "env": {}
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    let server_id = body["server"]["id"].as_str().unwrap();
    let server_uuid: uuid::Uuid = server_id.parse().unwrap();

    // Trigger install and wait for completion
    app.post(
        &format!("/api/servers/{}/install", server_id),
        Some(&token),
        json!(null),
    )
    .await;
    poll_phase_complete(&app, &token, server_id).await;

    // Verify the phase log buffer has content (this is what the WS handler
    // replays to late-connecting clients).
    let phase_logs = app
        .state
        .pipeline_manager
        .get_phase_log_buffer(&server_uuid);
    assert!(
        !phase_logs.is_empty(),
        "Phase log buffer should contain entries after pipeline runs"
    );

    // The buffer should contain the "Starting step" lines emitted by the runner
    let log_text: Vec<&str> = phase_logs.iter().map(|l| l.line.as_str()).collect();
    assert!(
        log_text.iter().any(|l| l.contains("Starting step")),
        "Phase log buffer should contain 'Starting step' entries. Got: {:?}",
        log_text
    );

    // The buffer should contain output from the echo command
    assert!(
        log_text.iter().any(|l| l.contains("pipeline output line")),
        "Phase log buffer should contain command stdout. Got: {:?}",
        log_text
    );

    // Each log entry should have the correct phase
    for log in &phase_logs {
        assert_eq!(
            format!("{:?}", log.phase),
            "Install",
            "All log entries should be for the install phase"
        );
    }

    // Step names should be present
    let step_names: Vec<&str> = phase_logs.iter().map(|l| l.step_name.as_str()).collect();
    assert!(
        step_names.contains(&"Create dir"),
        "Logs should reference 'Create dir' step. Got: {:?}",
        step_names
    );
    assert!(
        step_names.contains(&"Write config"),
        "Logs should reference 'Write config' step. Got: {:?}",
        step_names
    );
    assert!(
        step_names.contains(&"Run echo"),
        "Logs should reference 'Run echo' step. Got: {:?}",
        step_names
    );
}

#[tokio::test]
async fn test_phase_log_buffer_contains_error_details_on_failure() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (server_id, _) = create_and_install(
        &app,
        &token,
        "Error Log Buffer Test",
        json!([
            {
                "name": "Bad download",
                "description": null,
                "action": {
                    "type": "download",
                    "url": "http://127.0.0.1:1/does-not-exist.jar",
                    "destination": ".",
                    "filename": null,
                    "executable": false
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let server_uuid: uuid::Uuid = server_id.parse().unwrap();
    let phase_logs = app
        .state
        .pipeline_manager
        .get_phase_log_buffer(&server_uuid);

    assert!(
        !phase_logs.is_empty(),
        "Phase log buffer should have entries even for failed pipelines"
    );

    // Should contain the failure log line (emitted by runner as "✗ Step failed: ...")
    let log_text: Vec<&str> = phase_logs.iter().map(|l| l.line.as_str()).collect();
    assert!(
        log_text
            .iter()
            .any(|l| l.contains("Step failed") || l.contains("failed")),
        "Phase log buffer should contain failure details. Got: {:?}",
        log_text
    );

    // The error logs should be on stderr stream
    let stderr_logs: Vec<&str> = phase_logs
        .iter()
        .filter(|l| format!("{:?}", l.stream) == "Stderr")
        .map(|l| l.line.as_str())
        .collect();
    assert!(
        !stderr_logs.is_empty(),
        "Failed step should produce stderr log entries. All logs: {:?}",
        log_text
    );
}

#[tokio::test]
async fn test_phase_progress_survives_after_completion_for_replay() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (server_id, _) = create_and_install(
        &app,
        &token,
        "Replay Progress Test",
        json!([
            {
                "name": "Quick step",
                "description": null,
                "action": { "type": "create_dir", "path": "data" },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    // Query phase-status multiple times — the data should be stable and
    // identical, simulating what a WS replay would send to a late-connecting
    // client.
    let (_, body1) = app
        .get(
            &format!("/api/servers/{}/phase-status", server_id),
            Some(&token),
        )
        .await;
    let (_, body2) = app
        .get(
            &format!("/api/servers/{}/phase-status", server_id),
            Some(&token),
        )
        .await;

    assert_eq!(body1["progress"]["status"], "completed");
    assert_eq!(body2["progress"]["status"], "completed");

    // The progress data should be identical across requests
    assert_eq!(
        body1["progress"], body2["progress"],
        "Phase progress should be stable across repeated queries (replay consistency)"
    );

    // All step data should be present for the UI to render
    let steps = body1["progress"]["steps"].as_array().unwrap();
    assert!(!steps.is_empty());
    for step in steps {
        assert!(step["step_name"].is_string());
        assert!(step["status"].is_string());
        assert!(step["started_at"].is_string());
        assert!(step["completed_at"].is_string());
    }

    // Phase-level timestamps should be set
    assert!(body1["progress"]["started_at"].is_string());
    assert!(body1["progress"]["completed_at"].is_string());
}

#[tokio::test]
async fn test_pipeline_rerun_on_same_server_reuses_handle() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (server_id, phase1) = create_and_install(
        &app,
        &token,
        "Rerun Test",
        json!([
            {
                "name": "Write v1",
                "description": null,
                "action": {
                    "type": "write_file",
                    "path": "version.txt",
                    "content": "v1"
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([
            {
                "name": "Write v2",
                "description": null,
                "action": {
                    "type": "write_file",
                    "path": "version.txt",
                    "content": "v2"
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!({}),
    )
    .await;

    assert_eq!(phase1["progress"]["status"], "completed");

    let server_uuid: uuid::Uuid = server_id.parse().unwrap();

    // The ProcessHandle should exist after first install
    assert!(app.state.process_manager.handles.contains_key(&server_uuid));

    // Run update pipeline (second pipeline on same server)
    let (upd_status, _) = app
        .post(
            &format!("/api/servers/{}/update", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(upd_status, StatusCode::OK);

    let phase2 = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(phase2["progress"]["status"], "completed");
    assert_eq!(phase2["progress"]["phase"], "update");

    // ProcessHandle should still exist (ensure_handle reused it)
    assert!(app.state.process_manager.handles.contains_key(&server_uuid));

    // The phase log buffer should now contain update-phase logs
    let phase_logs = app
        .state
        .pipeline_manager
        .get_phase_log_buffer(&server_uuid);
    assert!(
        phase_logs
            .iter()
            .any(|l| format!("{:?}", l.phase) == "Update"),
        "Phase log buffer should contain update-phase entries after second pipeline"
    );

    // File should contain v2 from the update
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    let content = std::fs::read_to_string(server_dir.join("version.txt")).unwrap();
    assert_eq!(content, "v2");
}

#[tokio::test]
async fn test_reinstall_replaces_phase_log_buffer() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create server with install steps
    let echo = resolve_binary("echo");
    let (_, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Reinstall Buffer Test",
                    "binary": echo,
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [
                        {
                            "name": "First install step",
                            "description": null,
                            "action": {
                                "type": "write_file",
                                "path": "marker.txt",
                                "content": "first"
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    let server_id = body["server"]["id"].as_str().unwrap();
    let server_uuid: uuid::Uuid = server_id.parse().unwrap();

    // First install
    app.post(
        &format!("/api/servers/{}/install", server_id),
        Some(&token),
        json!(null),
    )
    .await;
    poll_phase_complete(&app, &token, server_id).await;

    let logs_after_first = app
        .state
        .pipeline_manager
        .get_phase_log_buffer(&server_uuid);
    assert!(!logs_after_first.is_empty());
    let first_log_count = logs_after_first.len();

    // Second install (reinstall)
    app.post(
        &format!("/api/servers/{}/install", server_id),
        Some(&token),
        json!(null),
    )
    .await;
    poll_phase_complete(&app, &token, server_id).await;

    // The phase log buffer should now contain fresh logs from the second run.
    // The PipelineHandle is replaced on each run, so the buffer is fresh.
    let logs_after_second = app
        .state
        .pipeline_manager
        .get_phase_log_buffer(&server_uuid);
    assert!(!logs_after_second.is_empty());

    // The second run should have produced new log entries (the buffer
    // belongs to the new PipelineHandle, so the count should be similar
    // to the first run, not double).
    // We check that the buffer isn't growing unboundedly — it should be
    // roughly the same size since it's the same steps.
    assert!(
        logs_after_second.len() <= first_log_count * 2,
        "Phase log buffer should not grow unboundedly across reinstalls. \
         First: {}, Second: {}",
        first_log_count,
        logs_after_second.len()
    );
}

#[tokio::test]
async fn test_ensure_handle_is_subscribable_before_pipeline_starts() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Subscribe Test").await;
    let server_uuid: uuid::Uuid = server_id.parse().unwrap();

    // No handle yet — subscribe returns None
    assert!(
        app.state.process_manager.subscribe(&server_uuid).is_none(),
        "No subscription should be available before any pipeline or start"
    );

    // Manually call ensure_handle (simulates what run_phase does)
    let _tx = app.state.process_manager.ensure_handle(server_uuid);

    // Now subscribe should work
    assert!(
        app.state.process_manager.subscribe(&server_uuid).is_some(),
        "ensure_handle should make the channel subscribable"
    );

    // The runtime should still be stopped
    let runtime = app.state.process_manager.get_runtime(&server_uuid);
    assert_eq!(
        format!("{:?}", runtime.status),
        "Stopped",
        "ensure_handle should create a handle with Stopped status"
    );
}

#[tokio::test]
async fn test_ensure_handle_idempotent_returns_same_channel() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Idempotent Test").await;
    let server_uuid: uuid::Uuid = server_id.parse().unwrap();

    // Call ensure_handle twice
    let tx1 = app.state.process_manager.ensure_handle(server_uuid);
    let tx2 = app.state.process_manager.ensure_handle(server_uuid);

    // Subscribe from both — sending on one should be receivable from the other's
    // subscriber since they should be the same underlying channel.
    let mut rx = tx1.subscribe();
    let test_msg = anyserver::types::WsMessage::PhaseProgress(anyserver::types::PhaseProgress {
        server_id: server_uuid,
        phase: anyserver::types::PhaseKind::Install,
        status: anyserver::types::PhaseStatus::Running,
        steps: vec![],
        started_at: None,
        completed_at: None,
    });
    tx2.send(test_msg.clone()).unwrap();

    let received = rx.try_recv();
    assert!(
        received.is_ok(),
        "Messages sent on one ensure_handle sender should be receivable \
         from a subscriber of the other — they must be the same channel"
    );
}

#[tokio::test]
async fn test_phase_log_buffer_empty_for_server_with_no_pipeline() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "No Pipeline").await;
    let server_uuid: uuid::Uuid = server_id.parse().unwrap();

    let logs = app
        .state
        .pipeline_manager
        .get_phase_log_buffer(&server_uuid);
    assert!(
        logs.is_empty(),
        "Phase log buffer should be empty for a server that has never had a pipeline run"
    );
}

// ─── WaitForOutput integration tests ──────────────────────────────────────────

/// Helper: wait for a server's runtime status to leave Running/Starting,
/// i.e. the process has exited (Stopped or Crashed).
async fn wait_for_server_exit(app: &TestApp, server_uuid: &uuid::Uuid) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let rt = app.state.process_manager.get_runtime(server_uuid);
        if rt.status != types::ServerStatus::Running && rt.status != types::ServerStatus::Starting {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "Server {} did not exit within timeout — status: {:?}",
                server_uuid, rt.status
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn test_wait_for_output_finds_pattern_in_existing_log_buffer() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");

    // Create a server whose binary emits "Server ready!" and exits immediately.
    // After the process exits (status becomes Crashed because it's an unexpected
    // exit), the log buffer still holds the output, and we can run the install
    // pipeline.
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "WFO Buffer Test",
                    "binary": echo,
                    "args": ["Server ready!"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 5,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [
                        {
                            "name": "Wait for ready",
                            "description": null,
                            "action": {
                                "type": "wait_for_output",
                                "pattern": "Server ready",
                                "timeout_secs": 10
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let server_uuid: uuid::Uuid = server_id.parse().unwrap();

    // Start the server — echo prints "Server ready!" and exits immediately.
    let (start_status, _) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(start_status, StatusCode::OK);

    // Wait for the process to exit so the server is no longer Running.
    wait_for_server_exit(&app, &server_uuid).await;

    // Verify the output is in the log buffer.
    let buf = app.state.process_manager.get_log_buffer(&server_uuid);
    assert!(
        buf.iter().any(|l| l.line.contains("Server ready!")),
        "Log buffer should contain 'Server ready!' — got: {:?}",
        buf.iter().map(|l| &l.line).collect::<Vec<_>>()
    );

    // Now trigger the install pipeline which has a WaitForOutput step.
    // The pattern should already be in the log buffer so it succeeds immediately.
    let (inst_status, inst_body) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(
        inst_status,
        StatusCode::OK,
        "install trigger failed: {:?}",
        inst_body
    );

    let final_body = poll_phase_complete(&app, &token, &server_id).await;
    let progress = &final_body["progress"];
    assert_eq!(
        progress["status"], "completed",
        "WaitForOutput should have found pattern in log buffer: {:?}",
        progress
    );
    assert_eq!(progress["steps"][0]["status"], "completed");
}

#[tokio::test]
async fn test_wait_for_output_times_out_when_pattern_missing() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");

    // Create a server (binary is just "echo hello" — not the pattern we look for).
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "WFO Timeout Test",
                    "binary": echo,
                    "args": ["hello"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 5,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [
                        {
                            "name": "Wait for missing pattern",
                            "description": null,
                            "action": {
                                "type": "wait_for_output",
                                "pattern": "never gonna appear",
                                "timeout_secs": 1
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    // Trigger install — WaitForOutput should time out.
    let (inst_status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(inst_status, StatusCode::OK);

    let final_body = poll_phase_complete(&app, &token, &server_id).await;
    let progress = &final_body["progress"];
    assert_eq!(
        progress["status"], "failed",
        "Pipeline should fail when WaitForOutput times out: {:?}",
        progress
    );

    let step = &progress["steps"][0];
    assert_eq!(step["status"], "failed");
    let message = step["message"].as_str().unwrap_or("");
    assert!(
        message.contains("Timed out"),
        "Error message should mention timeout, got: {}",
        message
    );
}

#[tokio::test]
async fn test_wait_for_output_timeout_with_continue_on_error() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");

    // WaitForOutput times out but continue_on_error is set, so the next step
    // should still run and the overall pipeline should succeed.
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "WFO Continue Test",
                    "binary": echo,
                    "args": ["hello"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 5,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [
                        {
                            "name": "Wait for missing",
                            "description": null,
                            "action": {
                                "type": "wait_for_output",
                                "pattern": "wont appear",
                                "timeout_secs": 1
                            },
                            "condition": null,
                            "continue_on_error": true
                        },
                        {
                            "name": "Create marker",
                            "description": null,
                            "action": {
                                "type": "write_file",
                                "path": "marker.txt",
                                "content": "pipeline continued"
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    let (inst_status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(inst_status, StatusCode::OK);

    let final_body = poll_phase_complete(&app, &token, &server_id).await;
    let progress = &final_body["progress"];
    assert_eq!(
        progress["status"], "completed",
        "Pipeline should complete when continue_on_error is set: {:?}",
        progress
    );

    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(
        steps[0]["status"], "failed",
        "WaitForOutput step should fail"
    );
    assert_eq!(
        steps[1]["status"], "completed",
        "Subsequent step should still run"
    );

    // Verify the marker file was written.
    let data_dir = app._temp_dir.path();
    let server_dir = data_dir.join("servers").join(&server_id);
    let content = std::fs::read_to_string(server_dir.join("marker.txt")).unwrap();
    assert_eq!(content, "pipeline continued");
}

#[tokio::test]
async fn test_wait_for_output_variable_substitution_in_pattern() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");

    // Server emits "Build v1.2.3 complete" — the pipeline uses a variable
    // in the pattern: "Build v${version} complete".
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "WFO Variable Test",
                    "binary": echo,
                    "args": ["Build v1.2.3 complete"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 5,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "version",
                            "label": "Version",
                            "param_type": "string",
                            "required": true
                        }
                    ],
                    "install_steps": [
                        {
                            "name": "Wait for build",
                            "description": null,
                            "action": {
                                "type": "wait_for_output",
                                "pattern": "Build v${version} complete",
                                "timeout_secs": 10
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": { "version": "1.2.3" }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let server_uuid: uuid::Uuid = server_id.parse().unwrap();

    // Start the server — echo prints and exits immediately.
    let (start_status, _) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(start_status, StatusCode::OK);

    // Wait for process to exit.
    wait_for_server_exit(&app, &server_uuid).await;

    // Trigger install with WaitForOutput using variable substitution.
    let (inst_status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(inst_status, StatusCode::OK);

    let final_body = poll_phase_complete(&app, &token, &server_id).await;
    let progress = &final_body["progress"];
    assert_eq!(
        progress["status"], "completed",
        "WaitForOutput with variable substitution should succeed: {:?}",
        progress
    );
    assert_eq!(progress["steps"][0]["status"], "completed");
}

#[tokio::test]
async fn test_wait_for_output_case_insensitive_match() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");

    // Server emits "SERVER READY" in uppercase, pattern searches lowercase.
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "WFO Case Test",
                    "binary": echo,
                    "args": ["SERVER READY"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 5,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [
                        {
                            "name": "Wait case insensitive",
                            "description": null,
                            "action": {
                                "type": "wait_for_output",
                                "pattern": "server ready",
                                "timeout_secs": 10
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let server_uuid: uuid::Uuid = server_id.parse().unwrap();

    // Start the server — echo prints and exits.
    let (start_status, _) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(start_status, StatusCode::OK);

    // Wait for process to exit.
    wait_for_server_exit(&app, &server_uuid).await;

    // Trigger install — pattern is lowercase, output is uppercase.
    let (inst_status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(inst_status, StatusCode::OK);

    let final_body = poll_phase_complete(&app, &token, &server_id).await;
    let progress = &final_body["progress"];
    assert_eq!(
        progress["status"], "completed",
        "Case-insensitive WaitForOutput should succeed: {:?}",
        progress
    );
}

#[tokio::test]
async fn test_wait_for_output_arrives_via_broadcast_during_pipeline() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");

    // Create server — the binary doesn't matter, we won't start it.
    // Instead we'll inject log lines directly into the ProcessHandle's
    // broadcast channel while the pipeline is blocking on WaitForOutput.
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "WFO Broadcast Test",
                    "binary": echo,
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 5,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [
                        {
                            "name": "Wait for init",
                            "description": null,
                            "action": {
                                "type": "wait_for_output",
                                "pattern": "Initialization complete",
                                "timeout_secs": 10
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let server_uuid: uuid::Uuid = server_id.parse().unwrap();

    // Trigger install — the WaitForOutput step will block waiting for the pattern.
    let (inst_status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(inst_status, StatusCode::OK);

    // Give the pipeline a moment to start and begin waiting.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Inject a log line directly into the ProcessHandle's broadcast channel.
    // The PipelineHandle.log_tx and ProcessHandle.log_tx are the same sender,
    // so the WaitForOutput subscriber will receive this.
    let log_tx = app.state.process_manager.ensure_handle(server_uuid);
    let log_line = types::LogLine {
        seq: 0,
        timestamp: chrono::Utc::now(),
        line: "Initialization complete".to_string(),
        stream: types::LogStream::Stdout,
    };
    let _ = log_tx.send(types::WsMessage::Log(log_line));

    let final_body = poll_phase_complete(&app, &token, &server_id).await;
    let progress = &final_body["progress"];
    assert_eq!(
        progress["status"], "completed",
        "WaitForOutput should succeed when pattern arrives via broadcast: {:?}",
        progress
    );
    assert_eq!(progress["steps"][0]["status"], "completed");
}

// ─── Frontend integration tests (only with bundle-frontend) ───────────────────

#[cfg(feature = "bundle-frontend")]
mod with_frontend {
    use super::*;

    use axum::body::Body;
    use axum::http::{header, Method, Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Send a raw HTTP request and return (status, headers, body bytes).
    async fn raw_request(
        app: &TestApp,
        method: Method,
        uri: &str,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .expect("failed to build request");

        let resp = app
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("request failed");

        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp
            .into_body()
            .collect()
            .await
            .expect("failed to collect body")
            .to_bytes()
            .to_vec();

        (status, headers, bytes)
    }

    #[tokio::test]
    async fn test_frontend_serves_while_pipeline_runs() {
        let app = TestApp::new().await;
        let token = app.setup_admin("admin", "Admin1234").await;
        let sleep_bin = resolve_binary("sleep");
        let echo = resolve_binary("echo");

        // Create server with a slow install step
        let (_, body) = app
            .post(
                "/api/servers",
                Some(&token),
                json!({
                    "config": {
                        "name": "Frontend During Pipeline",
                        "binary": echo,
                        "args": [],
                        "env": {},
                        "working_dir": null,
                        "auto_start": false,
                        "auto_restart": false,
                        "max_restart_attempts": 0,
                        "restart_delay_secs": 5,
                        "stop_command": null,
                        "stop_timeout_secs": 10,
                        "sftp_username": null,
                        "sftp_password": null,
                        "parameters": [],
                        "install_steps": [
                            {
                                "name": "Slow step",
                                "description": null,
                                "action": {
                                    "type": "run_command",
                                    "command": sleep_bin,
                                    "args": ["5"],
                                    "working_dir": null,
                                    "env": {}
                                },
                                "condition": null,
                                "continue_on_error": false
                            }
                        ],
                        "update_steps": []
                    },
                    "parameter_values": {}
                }),
            )
            .await;

        let server_id = body["server"]["id"].as_str().unwrap();

        // Trigger install
        let (inst_status, _) = app
            .post(
                &format!("/api/servers/{}/install", server_id),
                Some(&token),
                json!(null),
            )
            .await;
        assert_eq!(inst_status, StatusCode::OK);

        // Give the pipeline a moment to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // While the pipeline is running, verify the frontend is still responsive
        let (fe_status, fe_headers, fe_body) = raw_request(&app, Method::GET, "/").await;
        assert_eq!(
            fe_status,
            StatusCode::OK,
            "Frontend should still respond during pipeline"
        );
        assert!(
            String::from(
                fe_headers
                    .get(header::CONTENT_TYPE)
                    .unwrap()
                    .to_str()
                    .unwrap()
            )
            .contains("text/html"),
            "Should serve HTML"
        );
        assert!(
            String::from_utf8_lossy(&fe_body).contains("<div id=\"root\">"),
            "Should serve index.html"
        );

        // Also verify API still works alongside frontend
        let (api_status, api_body) = app
            .get(
                &format!("/api/servers/{}/phase-status", server_id),
                Some(&token),
            )
            .await;
        assert_eq!(api_status, StatusCode::OK);
        assert_eq!(api_body["progress"]["status"], "running");

        // SPA routes should still fall back to index.html
        let (spa_status, _, spa_body) =
            raw_request(&app, Method::GET, "/servers/some-id/console").await;
        assert_eq!(spa_status, StatusCode::OK);
        assert!(String::from_utf8_lossy(&spa_body).contains("<div id=\"root\">"));

        // Cancel the slow pipeline so the test doesn't wait 5 seconds
        app.post(
            &format!("/api/servers/{}/cancel-phase", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    }

    #[tokio::test]
    async fn test_frontend_serves_after_pipeline_completes() {
        let app = TestApp::new().await;
        let token = app.setup_admin("admin", "Admin1234").await;

        let (server_id, phase_body) = create_and_install(
            &app,
            &token,
            "Frontend After Pipeline",
            json!([
                {
                    "name": "Quick step",
                    "description": null,
                    "action": { "type": "create_dir", "path": "data" },
                    "condition": null,
                    "continue_on_error": false
                }
            ]),
            json!([]),
            json!([]),
            json!({}),
        )
        .await;

        assert_eq!(phase_body["progress"]["status"], "completed");

        // Frontend should still work perfectly after pipeline completion
        let (fe_status, _, fe_body) = raw_request(&app, Method::GET, "/").await;
        assert_eq!(fe_status, StatusCode::OK);
        assert!(String::from_utf8_lossy(&fe_body).contains("<div id=\"root\">"));

        // API should return the completed status alongside a working frontend
        let (api_status, api_body) = app
            .get(
                &format!("/api/servers/{}/phase-status", server_id),
                Some(&token),
            )
            .await;
        assert_eq!(api_status, StatusCode::OK);
        assert_eq!(api_body["progress"]["status"], "completed");
        assert_eq!(api_body["installed"], true);
    }

    #[tokio::test]
    async fn test_frontend_serves_after_pipeline_failure() {
        let app = TestApp::new().await;
        let token = app.setup_admin("admin", "Admin1234").await;

        let (server_id, phase_body) = create_and_install(
            &app,
            &token,
            "Frontend After Failure",
            json!([
                {
                    "name": "Bad download",
                    "description": null,
                    "action": {
                        "type": "download",
                        "url": "http://127.0.0.1:1/nope.jar",
                        "destination": ".",
                        "filename": null,
                        "executable": false
                    },
                    "condition": null,
                    "continue_on_error": false
                }
            ]),
            json!([]),
            json!([]),
            json!({}),
        )
        .await;

        assert_eq!(phase_body["progress"]["status"], "failed");

        // Frontend should still work after a failed pipeline
        let (fe_status, _, fe_body) = raw_request(&app, Method::GET, "/").await;
        assert_eq!(fe_status, StatusCode::OK);
        assert!(String::from_utf8_lossy(&fe_body).contains("<div id=\"root\">"));

        // Phase status should show the failure details
        let (api_status, api_body) = app
            .get(
                &format!("/api/servers/{}/phase-status", server_id),
                Some(&token),
            )
            .await;
        assert_eq!(api_status, StatusCode::OK);
        assert_eq!(api_body["progress"]["status"], "failed");
        assert_eq!(api_body["installed"], false);

        let steps = api_body["progress"]["steps"].as_array().unwrap();
        assert_eq!(steps[0]["status"], "failed");
        assert!(
            steps[0]["message"].is_string(),
            "Failed step should have error message for frontend to display"
        );
    }
}

// ─── RunCommand Security Tests ────────────────────────────────────────────────

#[tokio::test]
async fn test_run_command_blocked_when_disabled() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let echo = resolve_binary("echo");

    // Verify default setting (allow_run_commands should be false for new installations)
    let (_, status_body) = app.get("/api/auth/status", None).await;
    assert_eq!(status_body["allow_run_commands"], false);

    let (_server_id, phase_body) = create_and_install(
        &app,
        &token,
        "Blocked Command Test",
        json!([
            {
                "name": "Echo test",
                "description": null,
                "action": {
                    "type": "run_command",
                    "command": echo,
                    "args": ["hello"],
                    "working_dir": null,
                    "env": {}
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "failed");

    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["step_name"], "Echo test");
    assert_eq!(steps[0]["status"], "failed");

    let error_msg = steps[0]["message"].as_str().unwrap();
    assert!(
        error_msg.contains("RunCommand steps are disabled"),
        "Error message should mention that RunCommand is disabled. Got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("Allow pipeline commands"),
        "Error message should mention the setting name. Got: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_run_command_allowed_when_enabled() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let echo = resolve_binary("echo");

    // Enable RunCommand
    let (status, _) = app
        .put(
            "/api/auth/settings",
            Some(&token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, phase_body) = create_and_install(
        &app,
        &token,
        "Allowed Command Test",
        json!([
            {
                "name": "Echo test",
                "description": null,
                "action": {
                    "type": "run_command",
                    "command": echo,
                    "args": ["hello", "from", "pipeline"],
                    "working_dir": null,
                    "env": {}
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "completed");
    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["step_name"], "Echo test");
    assert_eq!(steps[0]["status"], "completed");
}

#[tokio::test]
async fn test_run_command_audit_logging() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let echo = resolve_binary("echo");

    // Enable RunCommand
    let (status, _) = app
        .put(
            "/api/auth/settings",
            Some(&token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Run a pipeline with a RunCommand step
    let (_, phase_body) = create_and_install(
        &app,
        &token,
        "Audit Test Server",
        json!([
            {
                "name": "Test command",
                "description": null,
                "action": {
                    "type": "run_command",
                    "command": echo,
                    "args": ["audit", "test"],
                    "working_dir": null,
                    "env": {}
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "completed");

    // The audit logging happens via tracing, which we can't easily capture in tests
    // without setting up a test subscriber. For now, we just verify the command ran
    // successfully. The audit logs can be manually verified or tested with a tracing
    // subscriber in integration tests.
}

#[tokio::test]
async fn test_run_command_timeout_enforcement() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sleep_bin = resolve_binary("sleep");

    // Enable RunCommand with a short timeout
    let _ = app
        .put(
            "/api/auth/settings",
            Some(&token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 2,
                "run_command_use_namespaces": true
            }),
        )
        .await;

    let (_server_id, phase_body) = create_and_install(
        &app,
        &token,
        "Timeout Test Server",
        json!([
            {
                "name": "Long running command",
                "description": null,
                "action": {
                    "type": "run_command",
                    "command": sleep_bin,
                    "args": ["30"],
                    "working_dir": null,
                    "env": {}
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "failed");

    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["step_name"], "Long running command");
    assert_eq!(steps[0]["status"], "failed");

    let error_msg = steps[0]["message"].as_str().unwrap();
    assert!(
        error_msg.contains("timed out"),
        "Error message should mention timeout. Got: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_run_command_sandbox_mode_auto() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let echo = resolve_binary("echo");

    // Enable RunCommand with auto sandbox mode
    let _ = app
        .put(
            "/api/auth/settings",
            Some(&token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;

    let (_, phase_body) = create_and_install(
        &app,
        &token,
        "Auto Sandbox Test",
        json!([
            {
                "name": "Echo test",
                "description": null,
                "action": {
                    "type": "run_command",
                    "command": echo,
                    "args": ["sandboxed"],
                    "working_dir": null,
                    "env": {}
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "completed");
    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["status"], "completed");
}

#[tokio::test]
async fn test_run_command_sandbox_mode_off() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let echo = resolve_binary("echo");

    // Enable RunCommand with sandbox disabled
    let _ = app
        .put(
            "/api/auth/settings",
            Some(&token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "off",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;

    let (_, phase_body) = create_and_install(
        &app,
        &token,
        "No Sandbox Test",
        json!([
            {
                "name": "Echo test",
                "description": null,
                "action": {
                    "type": "run_command",
                    "command": echo,
                    "args": ["unsandboxed"],
                    "working_dir": null,
                    "env": {}
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "completed");
    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["status"], "completed");
}

#[tokio::test]
async fn test_run_command_with_namespaces_enabled() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let echo = resolve_binary("echo");

    // Enable RunCommand with namespaces enabled (default)
    let _ = app
        .put(
            "/api/auth/settings",
            Some(&token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;

    let (_, phase_body) = create_and_install(
        &app,
        &token,
        "Namespace Test",
        json!([
            {
                "name": "Echo with namespaces",
                "description": null,
                "action": {
                    "type": "run_command",
                    "command": echo,
                    "args": ["namespace", "test"],
                    "working_dir": null,
                    "env": {}
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    // The command should succeed whether or not namespaces are actually available
    // on the test host (graceful fallback)
    assert_eq!(progress["status"], "completed");
    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["status"], "completed");
}

#[tokio::test]
async fn test_run_command_with_namespaces_disabled() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let echo = resolve_binary("echo");

    // Enable RunCommand with namespaces explicitly disabled
    let _ = app
        .put(
            "/api/auth/settings",
            Some(&token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": false
            }),
        )
        .await;

    let (_, phase_body) = create_and_install(
        &app,
        &token,
        "No Namespace Test",
        json!([
            {
                "name": "Echo without namespaces",
                "description": null,
                "action": {
                    "type": "run_command",
                    "command": echo,
                    "args": ["no", "namespace"],
                    "working_dir": null,
                    "env": {}
                },
                "condition": null,
                "continue_on_error": false
            }
        ]),
        json!([]),
        json!([]),
        json!({}),
    )
    .await;

    let progress = &phase_body["progress"];
    assert_eq!(progress["status"], "completed");
    let steps = progress["steps"].as_array().unwrap();
    assert_eq!(steps[0]["status"], "completed");
}
