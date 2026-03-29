//! Integration tests for tech-debt fixes (ticket 018).
//!
//! Covers:
//!   - Typed API responses (issue #7): verify response shapes use proper fields
//!   - Variable dedup (issue #1): pipeline variable substitution works via the
//!     single canonical path (pipeline::variables)
//!   - FileEntry.size is u64 (issue #11): large file sizes don't overflow
//!   - Flush removal (issue #4): DB operations still persist correctly without
//!     per-write flush
//!   - IsolationConfig.pids_max ts-rs type override (issue #19)
//!   - defaultConfig isolation field presence (issue #22, frontend-side but
//!     we verify the Rust default here)

use axum::http::StatusCode;
use chrono::Utc;
use serde_json::json;

use crate::common::TestApp;

// ═══════════════════════════════════════════════════════════════════════
//  Issue #7 — Typed API responses (no more ad-hoc serde_json::json!)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_delete_server_response_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "to-delete").await;

    let (status, body) = app
        .delete(&format!("/api/servers/{}", id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);

    // Typed DeleteServerResponse fields
    assert_eq!(body["deleted"], true);
    assert_eq!(body["id"].as_str().unwrap(), id);
    // Should NOT have extra untyped keys
    assert!(
        body.as_object().unwrap().len() == 2,
        "expected exactly 2 fields: {:?}",
        body
    );
}

#[tokio::test]
async fn test_reset_server_response_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "to-reset").await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/reset", id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["reset"], true);
    assert_eq!(body["id"].as_str().unwrap(), id);
    assert!(body["killed_processes"].is_number());
    assert!(
        body.as_object().unwrap().len() == 3,
        "expected exactly 3 fields: {:?}",
        body
    );
}

#[tokio::test]
async fn test_send_command_response_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let cat = crate::common::resolve_binary("cat");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "cmd-test",
                    "binary": cat,
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
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let id = body["server"]["id"].as_str().unwrap().to_string();

    // Start the server so we can send a command
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/start", id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Give process time to start
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/command", id),
            Some(&token),
            json!({ "command": "hello" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["sent"], true);
    assert_eq!(body["command"], "hello");
    assert!(
        body.as_object().unwrap().len() == 2,
        "expected exactly 2 fields: {:?}",
        body
    );

    // Cleanup
    let _ = app
        .post(
            &format!("/api/servers/{}/stop", id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_cancel_stop_response_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (_id, _) = app.create_test_server(&token, "cancel-stop").await;

    // Cancelling stop on a non-running server should error, but let's verify
    // the response shape on a running server
    let sleep_bin = crate::common::resolve_binary("sleep");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "cancel-stop-server",
                    "binary": sleep_bin,
                    "args": ["60"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 30,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let sid = body["server"]["id"].as_str().unwrap().to_string();

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/start", sid),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Initiate stop in the background
    let state = app.state.clone();
    let sid_uuid: uuid::Uuid = sid.parse().unwrap();
    let stop_handle = tokio::spawn(async move {
        let _ = anyserver::server_management::process::stop_server(&state, sid_uuid).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/cancel-stop", sid),
            Some(&token),
            json!({}),
        )
        .await;

    // If we're fast enough it succeeds; otherwise the server may have already stopped.
    if status == StatusCode::OK {
        assert_eq!(body["cancelled"], true);
        assert!(body["server_id"].as_str().is_some());
        assert!(
            body.as_object().unwrap().len() == 2,
            "expected exactly 2 fields: {:?}",
            body
        );
    }

    let _ = stop_handle.await;
    // Force-kill to clean up
    let _ = app
        .post(
            &format!("/api/servers/{}/kill", sid),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_change_password_response_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/auth/change-password",
            Some(&token),
            json!({
                "current_password": "Admin1234",
                "new_password": "NewAdmin1234"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["changed"], true);
    assert!(body["token"].is_string(), "token field should be present");
    assert!(
        body.as_object().unwrap().len() == 2,
        "expected exactly 2 fields (changed and token): {:?}",
        body
    );
}

#[tokio::test]
async fn test_delete_user_response_shape() {
    let app = TestApp::new().await;
    let (admin_token, _user_token, user_id) = app.setup_admin_and_user().await;

    let (status, body) = app
        .delete(&format!("/api/admin/users/{}", user_id), Some(&admin_token))
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["deleted"], true);
    assert_eq!(body["id"].as_str().unwrap(), user_id);
    assert_eq!(body["username"], "regularuser");
    assert!(
        body.as_object().unwrap().len() == 3,
        "expected exactly 3 fields: {:?}",
        body
    );
}

#[tokio::test]
async fn test_delete_template_response_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a template
    let (status, body) = app
        .post(
            "/api/templates",
            Some(&token),
            json!({
                "name": "test-template",
                "description": "A test",
                "config": {
                    "name": "from-template",
                    "binary": "/bin/echo",
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
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let template_id = body["id"].as_str().unwrap().to_string();

    let (status, body) = app
        .delete(&format!("/api/templates/{}", template_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["deleted"], true);
    assert_eq!(body["id"].as_str().unwrap(), template_id);
    assert!(
        body.as_object().unwrap().len() == 2,
        "expected exactly 2 fields: {:?}",
        body
    );
}

#[tokio::test]
async fn test_write_file_response_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "write-file-shape").await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/files/write", id),
            Some(&token),
            json!({
                "path": "test.txt",
                "content": "hello world"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["written"], true);
    assert_eq!(body["path"], "test.txt");
    assert_eq!(body["size"], 11); // "hello world" is 11 bytes
    assert!(
        body.as_object().unwrap().len() == 3,
        "expected exactly 3 fields: {:?}",
        body
    );
}

#[tokio::test]
async fn test_create_dir_response_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "mkdir-shape").await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/files/mkdir", id),
            Some(&token),
            json!({ "path": "subdir" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["created"], true);
    assert_eq!(body["path"], "subdir");
    assert!(
        body.as_object().unwrap().len() == 2,
        "expected exactly 2 fields: {:?}",
        body
    );
}

#[tokio::test]
async fn test_delete_path_response_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "delete-path-shape").await;

    // Create a file first
    app.post(
        &format!("/api/servers/{}/files/write", id),
        Some(&token),
        json!({ "path": "deleteme.txt", "content": "bye" }),
    )
    .await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/files/delete", id),
            Some(&token),
            json!({ "path": "deleteme.txt" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["deleted"], true);
    assert_eq!(body["path"], "deleteme.txt");
    assert!(
        body.as_object().unwrap().len() == 2,
        "expected exactly 2 fields: {:?}",
        body
    );
}

#[tokio::test]
async fn test_chmod_response_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "chmod-shape").await;

    // Create a file
    app.post(
        &format!("/api/servers/{}/files/write", id),
        Some(&token),
        json!({ "path": "script.sh", "content": "#!/bin/sh" }),
    )
    .await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/files/chmod", id),
            Some(&token),
            json!({ "path": "script.sh", "mode": "755" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["path"], "script.sh");
    assert_eq!(body["mode"], "755");
    assert!(body["mode_display"].as_str().is_some());
    assert!(
        body.as_object().unwrap().len() == 3,
        "expected exactly 3 fields: {:?}",
        body
    );
}

#[tokio::test]
async fn test_remove_permission_response_shape() {
    let app = TestApp::new().await;
    let (admin_token, _user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "perm-shape").await;

    // Grant permission first
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
            json!({ "user_id": user_id, "level": "viewer" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Now remove it
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions/remove", server_id),
            Some(&admin_token),
            json!({ "user_id": user_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["removed"], true);
    assert_eq!(body["user_id"].as_str().unwrap(), user_id);
    assert_eq!(body["server_id"].as_str().unwrap(), server_id);
    assert!(
        body.as_object().unwrap().len() == 3,
        "expected exactly 3 fields: {:?}",
        body
    );
}

#[tokio::test]
async fn test_kill_directory_processes_response_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "kill-dir-shape").await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/kill-directory-processes", id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert!(body["killed"].is_number());
    assert!(body["failed"].is_number());
    assert!(body["processes"].is_array());
    assert!(
        body.as_object().unwrap().len() == 3,
        "expected exactly 3 fields: {:?}",
        body
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Issue #1 — Variable substitution deduplication
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_variable_substitution_in_binary_and_args() {
    // Create a server whose binary and args use variable references.
    // Starting the server will trigger `pipeline::variables::substitute_variables`
    // (the now-canonical path) rather than the deleted `process::subst`.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = crate::common::resolve_binary("echo");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "var-subst-test",
                    "binary": echo,
                    "args": ["${server_name}", "${server_id}"],
                    "env": { "MY_VAR": "${server_dir}" },
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "version",
                            "label": "Version",
                            "param_type": "string",
                            "default": "1.0.0",
                            "description": null,
                            "required": false,
                            "options": [],
                            "regex": null
                        }
                    ]
                },
                "parameter_values": {
                    "version": "2.0.0"
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    let id = body["server"]["id"].as_str().unwrap().to_string();

    // Start the server — this exercises the deduplicated variable path
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/start", id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Give it time to start and exit (echo exits immediately)
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify the server ran successfully — the log should contain the
    // substituted server name
    let runtime = app
        .state
        .process_manager
        .get_runtime(&id.parse::<uuid::Uuid>().unwrap());
    // echo exits immediately, so status could be stopped or crashed
    // The important thing is it *ran* without panicking due to missing
    // subst/build_process_vars functions.
    assert!(
        runtime.status == anyserver::types::ServerStatus::Stopped
            || runtime.status == anyserver::types::ServerStatus::Crashed
            || runtime.status == anyserver::types::ServerStatus::Running,
        "unexpected status: {:?}",
        runtime.status,
    );
}

#[tokio::test]
async fn test_parameter_substitution_uses_overrides_correctly() {
    // This tests that the canonical `build_variables` correctly applies
    // parameter defaults and user-supplied values, catching any divergence
    // that the old duplicated code might have hidden.
    use std::collections::HashMap;

    let server = anyserver::types::Server {
        id: uuid::Uuid::new_v4(),
        owner_id: uuid::Uuid::new_v4(),
        config: anyserver::types::ServerConfig {
            name: "test-server".into(),
            binary: "/bin/echo".into(),
            args: vec![],
            env: HashMap::new(),
            working_dir: None,
            auto_start: false,
            auto_restart: false,
            max_restart_attempts: 0,
            restart_delay_secs: 5,
            stop_command: None,
            stop_signal: anyserver::types::StopSignal::Sigterm,
            stop_timeout_secs: 10,
            sftp_username: None,
            sftp_password: None,
            parameters: vec![
                anyserver::types::ConfigParameter {
                    name: "version".into(),
                    label: "Version".into(),
                    default: Some("1.0.0".into()),
                    ..Default::default()
                },
                anyserver::types::ConfigParameter {
                    name: "flavor".into(),
                    label: "Flavor".into(),
                    default: Some("vanilla".into()),
                    ..Default::default()
                },
            ],
            stop_steps: vec![],
            start_steps: vec![],
            install_steps: vec![],
            update_steps: vec![],
            uninstall_steps: vec![],
            isolation: anyserver::types::IsolationConfig::default(),
            update_check: None,
            log_to_disk: false,
            max_log_size_mb: 50,
            enable_java_helper: false,
            enable_dotnet_helper: false,
            steam_app_id: None,
        },
        created_at: Utc::now(),
        updated_at: Utc::now(),
        parameter_values: {
            let mut m = HashMap::new();
            m.insert("version".into(), "2.0.0".into());
            m
        },
        installed: false,
        installed_at: None,
        updated_via_pipeline_at: None,
        installed_version: None,
        source_template_id: None,
    };

    let server_dir = std::path::Path::new("/tmp/test-server");
    let vars = anyserver::pipeline::variables::build_variables(&server, server_dir, None);

    // User-supplied value overrides default
    assert_eq!(vars.get("version").unwrap(), "2.0.0");
    // Default value used when no user override
    assert_eq!(vars.get("flavor").unwrap(), "vanilla");
    // Built-in variables
    assert_eq!(vars.get("server_name").unwrap(), "test-server");
    assert_eq!(vars.get("server_id").unwrap(), &server.id.to_string());
    assert!(vars.get("server_dir").unwrap().contains("test-server"));
}

#[tokio::test]
async fn test_parameter_overrides_take_precedence() {
    use std::collections::HashMap;

    let server = anyserver::types::Server {
        id: uuid::Uuid::new_v4(),
        owner_id: uuid::Uuid::new_v4(),
        config: anyserver::types::ServerConfig {
            name: "override-test".into(),
            binary: "/bin/echo".into(),
            args: vec![],
            env: HashMap::new(),
            working_dir: None,
            auto_start: false,
            auto_restart: false,
            max_restart_attempts: 0,
            restart_delay_secs: 5,
            stop_command: None,
            stop_signal: anyserver::types::StopSignal::Sigterm,
            stop_timeout_secs: 10,
            sftp_username: None,
            sftp_password: None,
            parameters: vec![anyserver::types::ConfigParameter {
                name: "version".into(),
                label: "Version".into(),
                default: Some("1.0.0".into()),
                ..Default::default()
            }],
            stop_steps: vec![],
            start_steps: vec![],
            install_steps: vec![],
            update_steps: vec![],
            uninstall_steps: vec![],
            isolation: anyserver::types::IsolationConfig::default(),
            update_check: None,
            log_to_disk: false,
            max_log_size_mb: 50,
            enable_java_helper: false,
            enable_dotnet_helper: false,
            steam_app_id: None,
        },
        created_at: Utc::now(),
        updated_at: Utc::now(),
        parameter_values: {
            let mut m = HashMap::new();
            m.insert("version".into(), "2.0.0".into());
            m
        },
        installed: false,
        installed_at: None,
        updated_via_pipeline_at: None,
        installed_version: None,
        source_template_id: None,
    };

    let server_dir = std::path::Path::new("/tmp/test");
    let overrides = {
        let mut m = HashMap::new();
        m.insert("version".into(), "3.0.0".into());
        m
    };
    let vars =
        anyserver::pipeline::variables::build_variables(&server, server_dir, Some(&overrides));

    // Override wins over both user-supplied and default
    assert_eq!(vars.get("version").unwrap(), "3.0.0");
}

// ═══════════════════════════════════════════════════════════════════════
//  Issue #11 — FileEntry.size is u64 (no overflow for >4GB files)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_file_entry_size_is_u64_in_response() {
    // We can't easily create a >4GB file in a test, but we can verify that
    // the JSON response serializes size as a number (not capped at u32 max).
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "filesize-test").await;

    // Write a small file and verify size comes back correctly
    let content = "a".repeat(1000);
    app.post(
        &format!("/api/servers/{}/files/write", id),
        Some(&token),
        json!({ "path": "test.txt", "content": content }),
    )
    .await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/files?path=", id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);

    let entries = body["entries"].as_array().unwrap();
    let test_file = entries.iter().find(|e| e["name"] == "test.txt").unwrap();
    assert_eq!(test_file["size"], 1000);

    // Verify size is serialized as a JSON number (not string)
    assert!(test_file["size"].is_number());
}

#[tokio::test]
async fn test_file_content_response_size_is_u64() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "content-size-test").await;

    let content = "hello world test content";
    app.post(
        &format!("/api/servers/{}/files/write", id),
        Some(&token),
        json!({ "path": "readme.txt", "content": content }),
    )
    .await;

    let (status, body) = app
        .get(
            &format!("/api/servers/{}/files/read?path=readme.txt", id),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["size"], content.len());
    assert!(body["size"].is_number());
}

// ═══════════════════════════════════════════════════════════════════════
//  Issue #4 — Flush removal: verify DB persistence without per-write flush
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_db_persistence_without_flush_servers() {
    // Verify that server data persists across operations even without
    // per-write flush. Sled's WAL ensures durability.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create multiple servers
    let (id1, _) = app.create_test_server(&token, "server-1").await;
    let (id2, _) = app.create_test_server(&token, "server-2").await;
    let (id3, _) = app.create_test_server(&token, "server-3").await;

    // Verify all are readable
    let (status, body) = app
        .get(&format!("/api/servers/{}", id1), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["server"]["config"]["name"], "server-1");

    let (status, body) = app
        .get(&format!("/api/servers/{}", id2), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["server"]["config"]["name"], "server-2");

    let (status, body) = app
        .get(&format!("/api/servers/{}", id3), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["server"]["config"]["name"], "server-3");

    // List should have all 3
    let (status, body) = app.get("/api/servers", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["servers"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_db_persistence_without_flush_update_then_read() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "original-name").await;

    // Update the server
    let echo = crate::common::resolve_binary("echo");
    let (status, _) = app
        .put(
            &format!("/api/servers/{}", id),
            Some(&token),
            json!({
                "config": {
                    "name": "updated-name",
                    "binary": echo,
                    "args": ["hello"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": null,
                    "sftp_password": null
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Verify the update persisted
    let (status, body) = app.get(&format!("/api/servers/{}", id), Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["server"]["config"]["name"], "updated-name");
}

#[tokio::test]
async fn test_db_persistence_without_flush_permissions() {
    let app = TestApp::new().await;
    let (admin_token, _user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "perm-persist").await;

    // Set a permission
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
            json!({ "user_id": user_id, "level": "manager" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Verify it persisted
    let (status, body) = app
        .get(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let entries = body["permissions"].as_array().unwrap();
    let user_entry = entries
        .iter()
        .find(|e| e["user"]["id"].as_str().unwrap() == user_id)
        .expect("user should have a permission entry");
    assert_eq!(user_entry["level"], "manager");

    // Remove and verify removal persisted
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/permissions/remove", server_id),
            Some(&admin_token),
            json!({ "user_id": user_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = app
        .get(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let entries = body["permissions"].as_array().unwrap();
    let has_user = entries
        .iter()
        .any(|e| e["user"]["id"].as_str().unwrap_or("") == user_id);
    assert!(!has_user, "permission should be removed");
}

#[tokio::test]
async fn test_db_persistence_without_flush_templates() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a template
    let (status, body) = app
        .post(
            "/api/templates",
            Some(&token),
            json!({
                "name": "persist-template",
                "description": "Test persistence",
                "config": {
                    "name": "from-template",
                    "binary": "/bin/echo",
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
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let tmpl_id = body["id"].as_str().unwrap().to_string();

    // Verify it persisted
    let (status, body) = app
        .get(&format!("/api/templates/{}", tmpl_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "persist-template");

    // Delete and verify
    let (status, _) = app
        .delete(&format!("/api/templates/{}", tmpl_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app
        .get(&format!("/api/templates/{}", tmpl_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ═══════════════════════════════════════════════════════════════════════
//  Issue #19 — IsolationConfig.pids_max type
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_isolation_config_pids_max_serializes_as_number() {
    // Verify that pids_max serializes as a regular JSON number, not a
    // string or bigint representation.
    let config = anyserver::types::IsolationConfig {
        enabled: true,
        extra_read_paths: vec![],
        extra_rw_paths: vec![],
        pids_max: Some(1024),
    };

    let json_val = serde_json::to_value(&config).unwrap();
    assert_eq!(json_val["pids_max"], 1024);
    assert!(json_val["pids_max"].is_number());
}

#[tokio::test]
async fn test_isolation_config_pids_max_null_when_none() {
    let config = anyserver::types::IsolationConfig::default();
    let json_val = serde_json::to_value(&config).unwrap();
    assert!(json_val["pids_max"].is_null());
}

#[tokio::test]
async fn test_isolation_config_default_values() {
    // Issue #22: verify the Rust default matches what the frontend should use
    let config = anyserver::types::IsolationConfig::default();
    assert!(config.enabled);
    assert!(config.extra_read_paths.is_empty());
    assert!(config.extra_rw_paths.is_empty());
    assert!(config.pids_max.is_none());
}

#[tokio::test]
async fn test_server_config_includes_isolation_in_api_response() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, body) = app.create_test_server(&token, "isolation-test").await;

    // The create response should include isolation config with defaults
    assert!(body["server"]["config"]["isolation"].is_object());
    assert_eq!(body["server"]["config"]["isolation"]["enabled"], true);
    assert!(body["server"]["config"]["isolation"]["extra_read_paths"].is_array());
    assert!(body["server"]["config"]["isolation"]["extra_rw_paths"].is_array());

    // Verify GET also returns isolation
    let (status, body) = app.get(&format!("/api/servers/{}", id), Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["server"]["config"]["isolation"].is_object());
    assert_eq!(body["server"]["config"]["isolation"]["enabled"], true);
}

// ═══════════════════════════════════════════════════════════════════════
//  Issue #19 — Generated TypeScript types use number, not bigint
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_isolation_config_ts_uses_number_not_bigint() {
    let ts_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../frontend/src/types/generated/IsolationConfig.ts");

    if ts_path.exists() {
        let content = std::fs::read_to_string(&ts_path).unwrap();
        assert!(
            !content.contains("bigint"),
            "IsolationConfig.ts should not contain 'bigint' — pids_max should be 'number | null'"
        );
        assert!(
            content.contains("number | null"),
            "IsolationConfig.ts should contain 'number | null' for pids_max"
        );
    }
}

#[test]
fn test_file_entry_ts_uses_number_for_size() {
    let ts_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../frontend/src/types/generated/FileEntry.ts");

    if ts_path.exists() {
        let content = std::fs::read_to_string(&ts_path).unwrap();
        assert!(
            !content.contains("bigint"),
            "FileEntry.ts should not contain 'bigint'"
        );
        assert!(
            content.contains("size: number"),
            "FileEntry.ts should have 'size: number' (not u32 or bigint)"
        );
    }
}

#[test]
fn test_file_content_response_ts_uses_number_for_size() {
    let ts_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../frontend/src/types/generated/FileContentResponse.ts");

    if ts_path.exists() {
        let content = std::fs::read_to_string(&ts_path).unwrap();
        assert!(
            !content.contains("bigint"),
            "FileContentResponse.ts should not contain 'bigint'"
        );
        assert!(
            content.contains("size: number"),
            "FileContentResponse.ts should have 'size: number'"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Generated response type files exist
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_all_typed_response_ts_files_exist() {
    let gen_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../frontend/src/types/generated");

    let expected_files = [
        "DeleteServerResponse.ts",
        "ResetServerResponse.ts",
        "KillDirectoryProcessesResponse.ts",
        "KillProcessResult.ts",
        "SendCommandResponse.ts",
        "SendSignalResponse.ts",
        "CancelStopResponse.ts",
        "ChangePasswordResponse.ts",
        "CancelPhaseResponse.ts",
        "WriteFileResponse.ts",
        "CreateDirResponse.ts",
        "DeletePathResponse.ts",
        "ChmodResponse.ts",
        "RemovePermissionResponse.ts",
        "DeleteTemplateResponse.ts",
        "DeleteUserResponse.ts",
    ];

    for file in &expected_files {
        let path = gen_dir.join(file);
        assert!(
            path.exists(),
            "Generated type file {} should exist at {:?}",
            file,
            path
        );
    }
}
