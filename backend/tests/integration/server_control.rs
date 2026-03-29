use std::time::Duration;

use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

use crate::common::{resolve_binary, TestApp};

#[tokio::test]
async fn test_start_server_with_real_binary() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sleep_bin = resolve_binary("sleep");

    // Use `sleep` so it stays running long enough to check status
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Sleep Server",
                    "binary": sleep_bin,
                    "args": ["30"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap();

    // Start
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "running");
    assert!(body["pid"].is_number());

    // Give it a moment
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify via GET
    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(body["runtime"]["status"], "running");

    // Stop
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Give it a moment for the monitor to update
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert!(
        body["runtime"]["status"] == "stopped" || body["runtime"]["status"] == "stopping",
        "expected stopped or stopping, got: {}",
        body["runtime"]["status"]
    );
}

#[tokio::test]
async fn test_cannot_start_already_running_server() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Runner").await;

    let sleep_bin = resolve_binary("sleep");

    // Patch the server to use sleep
    app.put(
        &format!("/api/servers/{}", server_id),
        Some(&token),
        json!({
            "config": {
                "name": "Runner",
                "binary": sleep_bin,
                "args": ["30"],
                "env": {},
                "working_dir": null,
                "auto_start": false,
                "auto_restart": false,
                "max_restart_attempts": 0,
                "restart_delay_secs": 5,
                "stop_command": null,
                "stop_timeout_secs": 2,
                "sftp_username": null,
                "sftp_password": null
            }
        }),
    )
    .await;

    // Start once
    app.post(
        &format!("/api/servers/{}/start", server_id),
        Some(&token),
        json!({}),
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Start again — should be a conflict
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Cleanup: stop the server
    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_send_command_to_running_server() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let cat_bin = resolve_binary("cat");

    // cat reads stdin — good target for send_command
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Cat Server",
                    "binary": cat_bin,
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap();

    // Start
    app.post(
        &format!("/api/servers/{}/start", server_id),
        Some(&token),
        json!({}),
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Send command
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/command", server_id),
            Some(&token),
            json!({ "command": "hello world" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sent"], true);

    // Cleanup
    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_send_empty_command_rejected() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Test").await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/command", server_id),
            Some(&token),
            json!({ "command": "" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_viewer_cannot_start_server() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Admin Server").await;

    // Grant viewer only
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&user_token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_viewer_cannot_send_command() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Admin Server").await;

    // Grant viewer only
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/command", server_id),
            Some(&user_token),
            json!({ "command": "exploit" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_operator_can_start_and_stop() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;

    let sleep_bin = resolve_binary("sleep");

    // Create a server as admin with sleep binary
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&admin_token),
            json!({
                "config": {
                    "name": "Op Test",
                    "binary": sleep_bin,
                    "args": ["30"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap();

    // Grant operator
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "operator" }),
    )
    .await;

    // Operator starts
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&user_token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "running");

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Operator stops
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&user_token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

// ─── Shell-script start tests ─────────────────────────────────────────────

#[tokio::test]
async fn test_start_non_executable_script_runs_via_interpreter() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server whose binary points to a script file we'll create
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Script Server",
                    "binary": "start.sh",
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    // Write a script that sleeps WITHOUT execute permission
    let server_dir = app.state.server_dir(&sid);
    std::fs::write(server_dir.join("start.sh"), "#!/bin/sh\nsleep 30\n").unwrap();
    // Deliberately do NOT chmod +x

    // Start — should succeed because resolve_execution detects the missing
    // execute bit and invokes the interpreter explicitly.
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);
    assert_eq!(body["status"], "running");

    // The log buffer should mention the non-executable fallback
    let logs = app.state.process_manager.get_log_buffer(&sid);
    let has_hint = logs.iter().any(|l| l.line.contains("not executable"));
    assert!(
        has_hint,
        "Expected a 'not executable' message in the log buffer, got: {:?}",
        logs.iter().map(|l| &l.line).collect::<Vec<_>>()
    );

    // Cleanup: stop
    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_start_nonexistent_binary_logs_error_to_console() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server pointing at a binary that does not exist
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Ghost Server",
                    "binary": "does_not_exist.sh",
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    // Attempt to start — should fail
    let (status, _body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let sid = Uuid::parse_str(&server_id).unwrap();
    let logs = app.state.process_manager.get_log_buffer(&sid);
    assert!(
        !logs.is_empty(),
        "Expected spawn error to be logged to the console buffer, but it was empty"
    );

    let has_spawn_error = logs.iter().any(|l| l.line.contains("Failed to spawn"));
    assert!(
        has_spawn_error,
        "Expected a 'Failed to spawn' message in the log buffer, got: {:?}",
        logs.iter().map(|l| &l.line).collect::<Vec<_>>()
    );

    // Should contain hint about the file not existing
    let has_hint = logs.iter().any(|l| l.line.contains("does not exist"));
    assert!(
        has_hint,
        "Expected a 'does not exist' hint in the log buffer, got: {:?}",
        logs.iter().map(|l| &l.line).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_start_executable_shell_script_runs_successfully() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server whose binary is a shell script
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Shell Script Server",
                    "binary": "start.sh",
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    // Write an executable shell script that sleeps
    let server_dir = app.state.server_dir(&sid);
    let script_path = server_dir.join("start.sh");
    std::fs::write(&script_path, "#!/bin/sh\nsleep 30\n").unwrap();

    // Make it executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Start — should succeed
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);
    assert_eq!(body["status"], "running");
    assert!(body["pid"].is_number());

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify running via GET
    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(body["runtime"]["status"], "running");

    // Cleanup: stop
    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_start_script_without_shebang_runs_via_sh() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "No Shebang Server",
                    "binary": "start.sh",
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    // Write a script WITHOUT a shebang line that sleeps (keeps process alive)
    let server_dir = app.state.server_dir(&sid);
    let script_path = server_dir.join("start.sh");
    std::fs::write(&script_path, "sleep 30\n").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Start — should succeed because resolve_execution detects the missing
    // shebang and runs the script via sh.
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);
    assert_eq!(body["status"], "running");

    // The log buffer should mention the shebang fallback
    let logs = app.state.process_manager.get_log_buffer(&sid);
    let has_shebang_msg = logs
        .iter()
        .any(|l| l.line.contains("no #! shebang") && l.line.contains("running via sh"));
    assert!(
        has_shebang_msg,
        "Expected a 'no shebang — running via sh' message in the log buffer, got: {:?}",
        logs.iter().map(|l| &l.line).collect::<Vec<_>>()
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify running
    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(body["runtime"]["status"], "running");

    // Cleanup: stop
    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_start_crlf_script_is_auto_fixed_and_runs() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "CRLF Server",
                    "binary": "start.sh",
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    // Write a shell script with Windows CRLF line endings (\r\n).
    // This is the exact scenario when a modpack zip from CurseForge is
    // extracted on Linux — the shebang becomes "#!/bin/sh\r" and the
    // kernel tries to exec "/bin/sh\r" which doesn't exist, giving a
    // confusing "No such file or directory" error.
    let server_dir = app.state.server_dir(&sid);
    let script_path = server_dir.join("start.sh");
    std::fs::write(&script_path, "#!/bin/sh\r\nsleep 30\r\n").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Start — should succeed because the CRLF is auto-fixed before spawn
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);
    assert_eq!(body["status"], "running");
    assert!(body["pid"].is_number());

    // The log buffer should contain a message about fixing the line endings
    let logs = app.state.process_manager.get_log_buffer(&sid);
    let has_fix_msg = logs
        .iter()
        .any(|l| l.line.contains("Fixed Windows line endings"));
    assert!(
        has_fix_msg,
        "Expected a 'Fixed Windows line endings' message in the log buffer, got: {:?}",
        logs.iter().map(|l| &l.line).collect::<Vec<_>>()
    );

    // Verify the file on disk no longer contains \r
    let content = std::fs::read(&script_path).unwrap();
    assert!(
        !content.contains(&b'\r'),
        "Expected CRLF to be stripped from the script on disk"
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify running
    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(body["runtime"]["status"], "running");

    // Cleanup: stop
    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_start_script_with_bad_interpreter_falls_back_to_sh() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Bad Interp Server",
                    "binary": "start.sh",
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    // Write a script with a shebang pointing to a non-existent interpreter.
    // The body is a valid POSIX shell script that sleeps so we can verify it
    // starts successfully via the sh fallback.
    let server_dir = app.state.server_dir(&sid);
    let script_path = server_dir.join("start.sh");
    std::fs::write(
        &script_path,
        "#!/usr/bin/nonexistent-interpreter\nsleep 30\n",
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Start — should succeed because resolve_execution detects the missing
    // interpreter and falls back to sh.
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);
    assert_eq!(body["status"], "running");

    // The log buffer should mention the interpreter fallback
    let logs = app.state.process_manager.get_log_buffer(&sid);
    let has_fallback_msg = logs.iter().any(|l| {
        l.line.contains("nonexistent-interpreter")
            && (l.line.contains("not found") || l.line.contains("falling back"))
    });
    assert!(
        has_fallback_msg,
        "Expected an interpreter fallback message in the log buffer, got: {:?}",
        logs.iter().map(|l| &l.line).collect::<Vec<_>>()
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify running
    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(body["runtime"]["status"], "running");

    // Cleanup: stop
    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_start_non_executable_no_shebang_script_runs_via_sh() {
    // Edge case: script has no shebang AND no execute permission.
    // resolve_execution should still handle this via sh.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Bare Script Server",
                    "binary": "run.sh",
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    // Write a plain script — no shebang, no +x
    let server_dir = app.state.server_dir(&sid);
    std::fs::write(server_dir.join("run.sh"), "sleep 30\n").unwrap();

    // Start — should succeed via sh
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);
    assert_eq!(body["status"], "running");

    let logs = app.state.process_manager.get_log_buffer(&sid);
    let has_not_exec = logs.iter().any(|l| l.line.contains("not executable"));
    let has_no_shebang = logs.iter().any(|l| l.line.contains("no #! shebang"));
    assert!(
        has_not_exec && has_no_shebang,
        "Expected both 'not executable' and 'no shebang' messages, got: {:?}",
        logs.iter().map(|l| &l.line).collect::<Vec<_>>()
    );

    // Cleanup: stop
    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

// ─── Start Pipeline Configuration Steps ──────────────────────────────────────

#[tokio::test]
async fn test_start_pipeline_set_env_passes_variables_to_process() {
    // Verify that SetEnv steps in the start pipeline actually inject
    // environment variables into the spawned process.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh = resolve_binary("sh");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "SetEnv Test",
                    "binary": sh,
                    "args": ["-c", "echo MY_CUSTOM_VAR=$MY_CUSTOM_VAR && echo ANOTHER=$ANOTHER && sleep 30"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "start_steps": [
                        {
                            "name": "Set environment",
                            "action": {
                                "type": "set_env",
                                "variables": {
                                    "MY_CUSTOM_VAR": "hello_from_pipeline",
                                    "ANOTHER": "world"
                                }
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "install_steps": [],
                    "update_steps": [],
                    "uninstall_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    // Start — triggers the start pipeline first, then spawns the binary
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);

    // Wait for the start pipeline to run and the process to produce output
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Check the process log buffer for the echoed env vars
    let logs = app.state.process_manager.get_log_buffer(&sid);
    let log_lines: Vec<&str> = logs.iter().map(|l| l.line.as_str()).collect();

    let has_custom_var = log_lines
        .iter()
        .any(|l| l.contains("MY_CUSTOM_VAR=hello_from_pipeline"));
    let has_another = log_lines.iter().any(|l| l.contains("ANOTHER=world"));
    assert!(
        has_custom_var,
        "Expected MY_CUSTOM_VAR=hello_from_pipeline in logs, got: {:?}",
        log_lines
    );
    assert!(
        has_another,
        "Expected ANOTHER=world in logs, got: {:?}",
        log_lines
    );

    // Cleanup
    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_start_pipeline_set_env_merges_with_static_env() {
    // SetEnv from the pipeline should merge with (and override) static env
    // from ServerConfig.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh = resolve_binary("sh");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Merge Env Test",
                    "binary": sh,
                    "args": ["-c", "echo STATIC=$STATIC_VAR && echo OVERRIDDEN=$OVERRIDE_ME && sleep 30"],
                    "env": {
                        "STATIC_VAR": "from_config",
                        "OVERRIDE_ME": "original_value"
                    },
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "start_steps": [
                        {
                            "name": "Override env",
                            "action": {
                                "type": "set_env",
                                "variables": {
                                    "OVERRIDE_ME": "pipeline_wins"
                                }
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "install_steps": [],
                    "update_steps": [],
                    "uninstall_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let logs = app.state.process_manager.get_log_buffer(&sid);
    let log_lines: Vec<&str> = logs.iter().map(|l| l.line.as_str()).collect();

    // Static env should still be present
    let has_static = log_lines.iter().any(|l| l.contains("STATIC=from_config"));
    // Pipeline should have overridden the value
    let has_overridden = log_lines
        .iter()
        .any(|l| l.contains("OVERRIDDEN=pipeline_wins"));
    assert!(
        has_static,
        "Expected STATIC=from_config in logs, got: {:?}",
        log_lines
    );
    assert!(
        has_overridden,
        "Expected OVERRIDDEN=pipeline_wins in logs, got: {:?}",
        log_lines
    );

    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_start_pipeline_set_working_dir_changes_cwd() {
    // Verify that SetWorkingDir in the start pipeline changes the process's
    // working directory.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh = resolve_binary("sh");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "WorkDir Test",
                    "binary": sh,
                    "args": ["-c", "pwd && sleep 30"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "start_steps": [
                        {
                            "name": "Create subdir",
                            "action": {
                                "type": "create_dir",
                                "path": "game_data"
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Set working dir",
                            "action": {
                                "type": "set_working_dir",
                                "path": "game_data"
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "install_steps": [],
                    "update_steps": [],
                    "uninstall_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let logs = app.state.process_manager.get_log_buffer(&sid);
    let log_lines: Vec<&str> = logs.iter().map(|l| l.line.as_str()).collect();

    // The pwd output should end with /game_data
    let has_game_data = log_lines
        .iter()
        .any(|l| l.ends_with("/game_data") || l.contains("/game_data"));
    assert!(
        has_game_data,
        "Expected working directory to contain /game_data, got: {:?}",
        log_lines
    );

    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_start_pipeline_set_working_dir_overrides_static_config() {
    // When both ServerConfig.working_dir and a SetWorkingDir step are present,
    // the pipeline step should win.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh = resolve_binary("sh");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "WorkDir Override Test",
                    "binary": sh,
                    "args": ["-c", "pwd && sleep 30"],
                    "env": {},
                    "working_dir": "static_dir",
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "start_steps": [
                        {
                            "name": "Create dirs",
                            "action": {
                                "type": "create_dir",
                                "path": "pipeline_dir"
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Override working dir",
                            "action": {
                                "type": "set_working_dir",
                                "path": "pipeline_dir"
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "install_steps": [],
                    "update_steps": [],
                    "uninstall_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let logs = app.state.process_manager.get_log_buffer(&sid);
    let log_lines: Vec<&str> = logs.iter().map(|l| l.line.as_str()).collect();

    // Pipeline wins: should see pipeline_dir, NOT static_dir
    let has_pipeline_dir = log_lines.iter().any(|l| l.contains("/pipeline_dir"));
    let has_static_dir = log_lines
        .iter()
        .any(|l| l.contains("/static_dir") && !l.contains("/pipeline_dir"));
    assert!(
        has_pipeline_dir,
        "Expected pipeline_dir in pwd output, got: {:?}",
        log_lines
    );
    assert!(
        !has_static_dir,
        "Expected static_dir NOT to appear in pwd output, got: {:?}",
        log_lines
    );

    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_start_pipeline_set_stop_command_overrides_config() {
    // Verify that SetStopCommand from the start pipeline takes priority
    // over ServerConfig.stop_command when stopping the server.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh = resolve_binary("sh");

    // The script reads from stdin and echoes whatever it receives, so we
    // can verify which stop command was actually sent.
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "StopCmd Test",
                    "binary": sh,
                    "args": ["-c", "while IFS= read -r line; do echo \"GOT:$line\"; if [ \"$line\" = \"pipeline-stop\" ]; then exit 0; fi; done"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": "config-stop",
                    "stop_timeout_secs": 5,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "start_steps": [
                        {
                            "name": "Set stop command",
                            "action": {
                                "type": "set_stop_command",
                                "command": "pipeline-stop"
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "install_steps": [],
                    "update_steps": [],
                    "uninstall_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);
    assert_eq!(body["status"], "running");

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Stop — should use pipeline-stop, not config-stop
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "stop failed");

    // Wait for the process to handle the stop command and exit
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let logs = app.state.process_manager.get_log_buffer(&sid);
    let log_lines: Vec<&str> = logs.iter().map(|l| l.line.as_str()).collect();

    // The script echoes "GOT:<command>" for whatever it reads from stdin.
    // We should see pipeline-stop, NOT config-stop.
    let got_pipeline_stop = log_lines.iter().any(|l| l.contains("GOT:pipeline-stop"));
    assert!(
        got_pipeline_stop,
        "Expected 'GOT:pipeline-stop' in logs (pipeline overrides config), got: {:?}",
        log_lines
    );
}

#[tokio::test]
async fn test_start_pipeline_set_env_with_variable_substitution() {
    // Verify that SetEnv values support ${param} substitution.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh = resolve_binary("sh");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "SetEnv Subst Test",
                    "binary": sh,
                    "args": ["-c", "echo JAVA=$JAVA_HOME && sleep 30"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "java_path",
                            "label": "Java Path",
                            "param_type": "string",
                            "default": "/usr/lib/jvm/java-17",
                            "required": false,
                            "options": [],
                            "regex": null
                        }
                    ],
                    "start_steps": [
                        {
                            "name": "Set Java env",
                            "action": {
                                "type": "set_env",
                                "variables": {
                                    "JAVA_HOME": "${java_path}"
                                }
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "install_steps": [],
                    "update_steps": [],
                    "uninstall_steps": []
                },
                "parameter_values": {
                    "java_path": "/opt/custom/jdk21"
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let logs = app.state.process_manager.get_log_buffer(&sid);
    let log_lines: Vec<&str> = logs.iter().map(|l| l.line.as_str()).collect();

    // The parameter value should have been substituted into the env var
    let has_java = log_lines
        .iter()
        .any(|l| l.contains("JAVA=/opt/custom/jdk21"));
    assert!(
        has_java,
        "Expected JAVA=/opt/custom/jdk21 in logs (variable substitution), got: {:?}",
        log_lines
    );

    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_start_pipeline_config_steps_round_trip_through_api() {
    // Verify that the three new step types are accepted by the API and
    // preserved when retrieved.
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Round Trip Test",
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
                    "start_steps": [
                        {
                            "name": "Set env vars",
                            "description": "Configure environment",
                            "action": {
                                "type": "set_env",
                                "variables": {
                                    "JAVA_HOME": "/usr/lib/jvm/java-17",
                                    "GAME_MODE": "survival"
                                }
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Set working directory",
                            "description": "Run from game subdir",
                            "action": {
                                "type": "set_working_dir",
                                "path": "game_root"
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Set stop command",
                            "description": "Graceful stop",
                            "action": {
                                "type": "set_stop_command",
                                "command": "stop"
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "install_steps": [],
                    "update_steps": [],
                    "uninstall_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap();

    // Fetch the server back and verify the start steps round-tripped
    let (status, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);

    let start_steps = body["server"]["config"]["start_steps"].as_array().unwrap();
    assert_eq!(start_steps.len(), 3);

    // Step 0: set_env
    assert_eq!(start_steps[0]["action"]["type"], "set_env");
    assert_eq!(
        start_steps[0]["action"]["variables"]["JAVA_HOME"],
        "/usr/lib/jvm/java-17"
    );
    assert_eq!(
        start_steps[0]["action"]["variables"]["GAME_MODE"],
        "survival"
    );

    // Step 1: set_working_dir
    assert_eq!(start_steps[1]["action"]["type"], "set_working_dir");
    assert_eq!(start_steps[1]["action"]["path"], "game_root");

    // Step 2: set_stop_command
    assert_eq!(start_steps[2]["action"]["type"], "set_stop_command");
    assert_eq!(start_steps[2]["action"]["command"], "stop");
}

#[tokio::test]
async fn test_start_pipeline_multiple_set_env_steps_merge() {
    // Multiple SetEnv steps should merge, with later steps overriding
    // earlier ones for the same key.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh = resolve_binary("sh");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Multi SetEnv Test",
                    "binary": sh,
                    "args": ["-c", "echo A=$A && echo B=$B && echo C=$C && sleep 30"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "start_steps": [
                        {
                            "name": "First env batch",
                            "action": {
                                "type": "set_env",
                                "variables": {
                                    "A": "first",
                                    "B": "original"
                                }
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Second env batch",
                            "action": {
                                "type": "set_env",
                                "variables": {
                                    "B": "overridden",
                                    "C": "added"
                                }
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "install_steps": [],
                    "update_steps": [],
                    "uninstall_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();
    let sid = Uuid::parse_str(&server_id).unwrap();

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let logs = app.state.process_manager.get_log_buffer(&sid);
    let log_lines: Vec<&str> = logs.iter().map(|l| l.line.as_str()).collect();

    let has_a = log_lines.iter().any(|l| l.contains("A=first"));
    let has_b = log_lines.iter().any(|l| l.contains("B=overridden"));
    let has_c = log_lines.iter().any(|l| l.contains("C=added"));
    assert!(has_a, "Expected A=first in logs, got: {:?}", log_lines);
    assert!(
        has_b,
        "Expected B=overridden (second step wins), got: {:?}",
        log_lines
    );
    assert!(has_c, "Expected C=added in logs, got: {:?}", log_lines);

    let _ = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
}

#[tokio::test]
async fn test_start_pipeline_failure_prevents_server_start() {
    // If the start pipeline fails, the server should NOT be started.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh = resolve_binary("sh");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Fail Pipeline Test",
                    "binary": sh,
                    "args": ["-c", "sleep 30"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [],
                    "start_steps": [
                        {
                            "name": "Run bad command",
                            "action": {
                                "type": "run_command",
                                "command": "/nonexistent/binary/that/does/not/exist",
                                "args": [],
                                "working_dir": null,
                                "env": {}
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "This should not run",
                            "action": {
                                "type": "set_env",
                                "variables": { "SHOULD_NOT": "appear" }
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "install_steps": [],
                    "update_steps": [],
                    "uninstall_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    // Start — should fail because the start pipeline fails
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "Expected start to fail because pipeline failed, got: {:?}",
        body
    );

    // Server should NOT be running
    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    let status_str = body["runtime"]["status"].as_str().unwrap_or("unknown");
    assert!(
        status_str == "stopped" || status_str == "crashed",
        "Expected server to be stopped/crashed after pipeline failure, got: {}",
        status_str
    );
}

/// Kill must terminate the entire process tree, not just the direct child.
/// We create a shell script that spawns a background `sleep` subprocess,
/// then verify that kill brings the status to stopped and the child process
/// is also gone.
#[tokio::test]
async fn test_kill_terminates_entire_process_tree() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server with a shell script that spawns a background child.
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Tree Kill Test",
                    "binary": "start.sh",
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap();
    let sid = Uuid::parse_str(server_id).unwrap();

    // Write a shell script that spawns a long-running child and also stays alive.
    let sleep_bin = resolve_binary("sleep");
    let server_dir = app.state.server_dir(&sid);
    let script = format!(
        "#!/bin/sh\n{} 300 &\nCHILD=$!\necho \"child_pid=$CHILD\"\n{} 300\n",
        sleep_bin, sleep_bin
    );
    std::fs::write(server_dir.join("start.sh"), &script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            server_dir.join("start.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    // Start the server
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "running");

    let parent_pid = body["pid"].as_u64().unwrap();

    // Give the script time to spawn the background child.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Kill the server
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/kill", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["status"], "stopped",
        "Expected status 'stopped' after kill, got: {}",
        body["status"]
    );

    // Verify the parent process is dead.
    let parent_alive = unsafe { libc::kill(parent_pid as i32, 0) } == 0;
    assert!(
        !parent_alive,
        "Parent process (pid {}) should be dead after kill",
        parent_pid
    );

    // Verify the runtime in the process manager is also consistent.
    let runtime = app.state.process_manager.get_runtime(&sid);
    assert_eq!(
        runtime.status,
        anyserver::types::ServerStatus::Stopped,
        "Process manager runtime should be Stopped"
    );
    assert!(runtime.pid.is_none(), "PID should be cleared after kill");
}

/// Stop should send SIGTERM to the process group when no stop_command is
/// configured, allowing processes that handle SIGTERM to exit gracefully.
#[tokio::test]
async fn test_stop_sends_sigterm_when_no_stop_command() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");

    // Create a server with no stop_command — stop should fall back to SIGTERM.
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "SIGTERM Test",
                    "binary": sleep_bin,
                    "args": ["300"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 5,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap();
    let sid = Uuid::parse_str(server_id).unwrap();

    // Start
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "running");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Stop — sleep handles SIGTERM by exiting, so this should work
    // without needing to wait for the full stop_timeout_secs.
    let start = std::time::Instant::now();
    let (status, _body) = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    let elapsed = start.elapsed();
    assert_eq!(status, StatusCode::OK);

    // It should have stopped quickly (SIGTERM kills sleep immediately).
    // If it waited the full 5s timeout, SIGTERM wasn't sent.
    assert!(
        elapsed < Duration::from_secs(4),
        "Stop took {:?} — SIGTERM should have killed sleep quickly, \
         not waited for the full timeout",
        elapsed
    );

    // Verify stopped
    let runtime = app.state.process_manager.get_runtime(&sid);
    assert!(
        runtime.status == anyserver::types::ServerStatus::Stopped
            || runtime.status == anyserver::types::ServerStatus::Crashed,
        "Expected stopped/crashed, got: {:?}",
        runtime.status
    );
}

/// Kill should work even if the server is stuck in "Stopping" state
/// (e.g. a graceful stop that is taking too long).
#[tokio::test]
async fn test_kill_works_while_server_is_stopping() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Use a script that ignores SIGTERM so stop will get stuck.
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Stubborn Server",
                    "binary": "stubborn.sh",
                    "args": [],
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
    let server_id = body["server"]["id"].as_str().unwrap();
    let sid = Uuid::parse_str(server_id).unwrap();

    // Write a script that traps SIGTERM and ignores it.
    let server_dir = app.state.server_dir(&sid);
    let script = "#!/bin/sh\ntrap '' TERM\necho 'started'\nwhile true; do sleep 1; done\n";
    std::fs::write(server_dir.join("stubborn.sh"), script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            server_dir.join("stubborn.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    // Start
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "running");

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Start a graceful stop in the background — it will get stuck because
    // the script ignores SIGTERM and there's no stop_command.
    let state_clone = app.state.clone();
    let sid_clone = sid;
    let stop_task = tokio::spawn(async move {
        let _ = anyserver::server_management::process::stop_server(&state_clone, sid_clone).await;
    });

    // Give stop a moment to set status to "Stopping".
    tokio::time::sleep(Duration::from_millis(300)).await;

    let runtime = app.state.process_manager.get_runtime(&sid);
    assert_eq!(
        runtime.status,
        anyserver::types::ServerStatus::Stopping,
        "Server should be in Stopping state"
    );

    // Now force-kill while it's stuck in Stopping.
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/kill", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["status"], "stopped",
        "Kill should have forced status to stopped, got: {}",
        body["status"]
    );

    // Clean up the stop_task (it may still be running or may have errored).
    stop_task.abort();
    let _ = stop_task.await;

    // Verify the process is truly dead.
    let runtime = app.state.process_manager.get_runtime(&sid);
    assert_eq!(runtime.status, anyserver::types::ServerStatus::Stopped);
    assert!(runtime.pid.is_none());
}

/// After kill, the status reported via GET must match the process manager
/// runtime — no stale "stopping" or "running" states.
#[tokio::test]
async fn test_status_consistent_after_kill() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Status Consistency",
                    "binary": sleep_bin,
                    "args": ["300"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap();
    let sid = Uuid::parse_str(server_id).unwrap();

    // Start
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Kill
    let (status, kill_body) = app
        .post(
            &format!("/api/servers/{}/kill", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // The kill response status must be stopped.
    assert_eq!(
        kill_body["status"], "stopped",
        "Kill response should say stopped, got: {}",
        kill_body["status"]
    );

    // GET the server — the runtime there must also say stopped.
    let (_, get_body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(
        get_body["runtime"]["status"], "stopped",
        "GET response should say stopped, got: {}",
        get_body["runtime"]["status"]
    );

    // Process manager should agree.
    let pm_runtime = app.state.process_manager.get_runtime(&sid);
    assert_eq!(pm_runtime.status, anyserver::types::ServerStatus::Stopped);
    assert!(pm_runtime.pid.is_none());
}

/// Cancel-stop while a server is in the graceful shutdown phase should
/// revert it back to Running.
#[tokio::test]
async fn test_cancel_stop_during_graceful_shutdown() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Use a script that ignores SIGTERM so stop will get stuck in the
    // grace period, giving us time to cancel.
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Cancel Stop Test",
                    "binary": "ignore_term.sh",
                    "args": [],
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
    let server_id = body["server"]["id"].as_str().unwrap();
    let sid = Uuid::parse_str(server_id).unwrap();

    // Write a script that traps SIGTERM and ignores it.
    let server_dir = app.state.server_dir(&sid);
    let script = "#!/bin/sh\ntrap '' TERM\necho 'started'\nwhile true; do sleep 1; done\n";
    std::fs::write(server_dir.join("ignore_term.sh"), script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            server_dir.join("ignore_term.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    // Start the server
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "running");

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Remember the PID so we can verify the process is still alive after cancel.
    let pid_before = app.state.process_manager.get_runtime(&sid).pid;
    assert!(pid_before.is_some(), "Server should have a PID");

    // Kick off a graceful stop in the background — it will get stuck
    // because the script ignores SIGTERM.
    let state_clone = app.state.clone();
    let sid_clone = sid;
    let stop_task = tokio::spawn(async move {
        let _ = anyserver::server_management::process::stop_server(&state_clone, sid_clone).await;
    });

    // Wait for the status to transition to Stopping.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let runtime = app.state.process_manager.get_runtime(&sid);
    assert_eq!(
        runtime.status,
        anyserver::types::ServerStatus::Stopping,
        "Server should be in Stopping state"
    );

    // Cancel the stop via the API endpoint.
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/cancel-stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cancelled"], true);

    // Give stop_server a moment to notice the cancel flag.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // The server should be back to Running.
    let runtime = app.state.process_manager.get_runtime(&sid);
    assert_eq!(
        runtime.status,
        anyserver::types::ServerStatus::Running,
        "Server should have reverted to Running after cancel, got: {:?}",
        runtime.status
    );

    // The process should still be alive with the same PID.
    assert_eq!(
        runtime.pid, pid_before,
        "Process PID should be unchanged after cancel"
    );

    // Clean up: kill the server so the test doesn't leave orphans.
    stop_task.abort();
    let _ = stop_task.await;
    let _ = anyserver::server_management::process::kill_server(&app.state, sid).await;
}

/// Cancel-stop when the server is not in Stopping state should return an error.
#[tokio::test]
async fn test_cancel_stop_when_not_stopping_returns_error() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Cancel Not Stopping",
                    "binary": sleep_bin,
                    "args": ["300"],
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
    let server_id = body["server"]["id"].as_str().unwrap();

    // Start
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Try to cancel-stop while the server is Running — should fail.
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/cancel-stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "Cancel-stop on a running server should be 409, got: {} body: {:?}",
        status,
        body
    );

    // Clean up
    let sid = Uuid::parse_str(server_id).unwrap();
    let _ = anyserver::server_management::process::kill_server(&app.state, sid).await;
}

/// StopProgress messages are broadcast over the process handle's log channel
/// during a graceful stop.
#[tokio::test]
async fn test_stop_broadcasts_stop_progress() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Use a script that ignores SIGTERM so the grace period actually runs.
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "StopProgress Test",
                    "binary": "trap_term.sh",
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 3,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let server_id = body["server"]["id"].as_str().unwrap();
    let sid = Uuid::parse_str(server_id).unwrap();

    // Write the script
    let server_dir = app.state.server_dir(&sid);
    let script = "#!/bin/sh\ntrap '' TERM\necho 'started'\nwhile true; do sleep 1; done\n";
    std::fs::write(server_dir.join("trap_term.sh"), script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            server_dir.join("trap_term.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    // Start
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Subscribe to the per-server broadcast BEFORE stopping so we can
    // capture StopProgress messages.
    let mut rx = app
        .state
        .process_manager
        .subscribe(&sid)
        .expect("should have a broadcast channel");

    // Kick off the stop in the background.
    let state_clone = app.state.clone();
    let sid_clone = sid;
    let stop_task = tokio::spawn(async move {
        let _ = anyserver::server_management::process::stop_server(&state_clone, sid_clone).await;
    });

    // Collect WS messages for a few seconds, looking for StopProgress.
    let mut saw_waiting_for_exit = false;
    let mut saw_sigkill = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);

    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
            Ok(Ok(anyserver::types::WsMessage::StopProgress(p))) => {
                assert_eq!(p.server_id, sid);
                // timeout_secs is now the total budget (elapsed + grace period),
                // so it will be ≥ the configured stop_timeout_secs (3).
                assert!(
                    p.timeout_secs >= 3.0,
                    "timeout_secs should be at least the configured grace period (3), got {}",
                    p.timeout_secs,
                );
                match p.phase {
                    anyserver::types::StopPhase::WaitingForExit => {
                        saw_waiting_for_exit = true;
                    }
                    anyserver::types::StopPhase::SendingSigkill => {
                        saw_sigkill = true;
                    }
                    _ => {}
                }
                // Once we've seen SIGKILL, the stop is almost done.
                if saw_sigkill {
                    break;
                }
            }
            Ok(Ok(_)) => {
                // Other message types (Log, StatusChange) — skip.
                continue;
            }
            Ok(Err(_)) => break, // channel closed or lagged
            Err(_) => continue,  // timeout, try again
        }
    }

    // Wait for the stop task to finish.
    let _ = tokio::time::timeout(Duration::from_secs(5), stop_task).await;

    assert!(
        saw_waiting_for_exit,
        "Should have received a StopProgress with WaitingForExit phase"
    );
    assert!(
        saw_sigkill,
        "Should have received a StopProgress with SendingSigkill phase (timeout was 3s)"
    );

    // Server should be stopped after SIGKILL.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let runtime = app.state.process_manager.get_runtime(&sid);
    assert!(
        runtime.status == anyserver::types::ServerStatus::Stopped
            || runtime.status == anyserver::types::ServerStatus::Crashed,
        "Server should be stopped after SIGKILL, got: {:?}",
        runtime.status
    );
}

/// Cancel-stop broadcasts a Cancelled phase in StopProgress and then a
/// StatusChange back to Running.
#[tokio::test]
async fn test_cancel_stop_broadcasts_cancelled_phase() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Script that ignores SIGTERM so stop gets stuck.
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Cancel Broadcast Test",
                    "binary": "stuck.sh",
                    "args": [],
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
    let server_id = body["server"]["id"].as_str().unwrap();
    let sid = Uuid::parse_str(server_id).unwrap();

    let server_dir = app.state.server_dir(&sid);
    let script = "#!/bin/sh\ntrap '' TERM\necho 'started'\nwhile true; do sleep 1; done\n";
    std::fs::write(server_dir.join("stuck.sh"), script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            server_dir.join("stuck.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    // Start
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Subscribe before stop so we capture all messages.
    let mut rx = app
        .state
        .process_manager
        .subscribe(&sid)
        .expect("should have a broadcast channel");

    // Stop in background
    let state_clone = app.state.clone();
    let sid_clone = sid;
    let stop_task = tokio::spawn(async move {
        let _ = anyserver::server_management::process::stop_server(&state_clone, sid_clone).await;
    });

    // Wait for Stopping status
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        app.state.process_manager.get_runtime(&sid).status,
        anyserver::types::ServerStatus::Stopping,
    );

    // Cancel via the API
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/cancel-stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Collect messages — we should see a Cancelled StopProgress and
    // a StatusChange back to Running.
    let mut saw_cancelled = false;
    let mut saw_running_status = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);

    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
            Ok(Ok(anyserver::types::WsMessage::StopProgress(p))) => {
                if p.phase == anyserver::types::StopPhase::Cancelled {
                    saw_cancelled = true;
                }
            }
            Ok(Ok(anyserver::types::WsMessage::StatusChange(rt))) => {
                if rt.status == anyserver::types::ServerStatus::Running {
                    saw_running_status = true;
                }
            }
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => break,
            Err(_) => continue,
        }
        if saw_cancelled && saw_running_status {
            break;
        }
    }

    stop_task.abort();
    let _ = stop_task.await;

    assert!(
        saw_cancelled,
        "Should have received a StopProgress with Cancelled phase"
    );
    assert!(
        saw_running_status,
        "Should have received a StatusChange back to Running"
    );

    // Clean up
    let _ = anyserver::server_management::process::kill_server(&app.state, sid).await;
}
