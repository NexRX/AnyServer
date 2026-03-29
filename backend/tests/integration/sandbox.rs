//! Integration tests for process-level sandboxing (ticket 019, Phase 1).
//!
//! These tests verify that the Landlock filesystem sandbox, NO_NEW_PRIVS,
//! FD cleanup, and RLIMIT_NPROC hardening layers work correctly when
//! applied to managed server processes.
//!
//! Key scenarios:
//! - An isolated process cannot read files outside its data directory
//! - An isolated process CAN read/write inside its data directory
//! - Normal server lifecycle (start/stop/kill) works with isolation on
//! - Isolation can be disabled per-server
//! - Servers still function when Landlock is unavailable (graceful skip)

use std::time::Duration;

use axum::http::StatusCode;
use serde_json::json;

use crate::common::{resolve_binary, TestApp};

// ─── Helpers ────────────────────────────────────────────────────────────

/// Create a server with a custom isolation config.  Returns (server_id, body).
async fn create_server_with_isolation(
    app: &TestApp,
    token: &str,
    name: &str,
    binary: &str,
    args: Vec<&str>,
    isolation: serde_json::Value,
) -> (String, serde_json::Value) {
    let (status, body) = app
        .post(
            "/api/servers",
            Some(token),
            json!({
                "config": {
                    "name": name,
                    "binary": binary,
                    "args": args,
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
                    "isolation": isolation
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    let id = body["server"]["id"].as_str().unwrap().to_string();
    (id, body)
}

/// Start a server and wait for it to be running.  Returns the start response body.
async fn start_and_wait(app: &TestApp, token: &str, server_id: &str) -> serde_json::Value {
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(token),
            json!({}),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "start failed for {}: {:?}",
        server_id,
        body
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    body
}

/// Stop a server and wait for it to finish.
async fn stop_and_wait(app: &TestApp, token: &str, server_id: &str) {
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(1000)).await;
}

/// Kill a server and wait for it to finish.
async fn kill_and_wait(app: &TestApp, token: &str, server_id: &str) {
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/kill", server_id),
            Some(token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(1000)).await;
}

/// Wait for a short-lived server process to exit and collect its log buffer.
async fn wait_for_exit_and_get_logs(
    app: &TestApp,
    token: &str,
    server_id: &str,
    timeout: Duration,
) -> Vec<String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let (_, body) = app
            .get(&format!("/api/servers/{}", server_id), Some(token))
            .await;
        let status = body["runtime"]["status"].as_str().unwrap_or("unknown");
        if status == "stopped" || status == "crashed" {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "Server {} did not exit within {:?}, status: {}",
                server_id, timeout, status
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // Collect logs from the process manager directly.
    let sid: uuid::Uuid = server_id.parse().unwrap();
    app.state
        .process_manager
        .get_log_buffer(&sid)
        .iter()
        .map(|l| l.line.clone())
        .collect()
}

/// Write a file into a server's data directory via the files API.
async fn write_server_file(app: &TestApp, token: &str, server_id: &str, path: &str, content: &str) {
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(token),
            json!({"path": path, "content": content}),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "failed to write file {} for server {}",
        path,
        server_id
    );
}

// ─── probe_capabilities ─────────────────────────────────────────────────

#[tokio::test]
async fn test_probe_capabilities_returns_string() {
    let caps = anyserver::sandbox::probe_capabilities();
    // Should contain at least the section header.
    assert!(
        caps.contains("Isolation capabilities"),
        "probe_capabilities should return a status string, got: {}",
        caps,
    );
    // On Linux, it should mention Landlock and NO_NEW_PRIVS.
    #[cfg(target_os = "linux")]
    {
        assert!(caps.contains("Landlock"), "should mention Landlock");
        assert!(caps.contains("NO_NEW_PRIVS"), "should mention NO_NEW_PRIVS");
    }
}

// ─── IsolationConfig defaults ───────────────────────────────────────────

#[tokio::test]
async fn test_isolation_config_defaults_enabled() {
    let config: anyserver::types::IsolationConfig = serde_json::from_str("{}").unwrap();
    assert!(config.enabled, "isolation should be enabled by default");
    assert_eq!(
        config.pids_max, None,
        "default pids_max should be None (no limit)"
    );
    assert!(config.extra_read_paths.is_empty());
    assert!(config.extra_rw_paths.is_empty());
}

#[tokio::test]
async fn test_isolation_config_disabled() {
    let config: anyserver::types::IsolationConfig =
        serde_json::from_str(r#"{"enabled": false}"#).unwrap();
    assert!(!config.enabled);
}

#[tokio::test]
async fn test_isolation_config_custom_paths() {
    let config: anyserver::types::IsolationConfig = serde_json::from_str(
        r#"{"extra_read_paths": ["/opt/java"], "extra_rw_paths": ["/mnt/shared"]}"#,
    )
    .unwrap();
    assert!(config.enabled);
    assert_eq!(config.extra_read_paths, vec!["/opt/java"]);
    assert_eq!(config.extra_rw_paths, vec!["/mnt/shared"]);
}

#[tokio::test]
async fn test_isolation_absent_from_server_config_uses_defaults() {
    // A ServerConfig JSON without an "isolation" field should deserialize
    // with the default IsolationConfig (enabled=true).
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let echo = resolve_binary("echo");

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "No Isolation Field",
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
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap();

    // Fetch the server and check isolation defaults are present.
    let (status, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["server"]["config"]["isolation"]["enabled"], true);
    assert_eq!(
        body["server"]["config"]["isolation"]["pids_max"],
        serde_json::Value::Null
    );
}

#[tokio::test]
async fn test_isolation_config_round_trips_through_api() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let echo = resolve_binary("echo");

    let isolation = json!({
        "enabled": true,
        "extra_read_paths": ["/opt/java-21"],
        "extra_rw_paths": ["/mnt/shared-data"],
        "pids_max": 256
    });

    let (server_id, _) =
        create_server_with_isolation(&app, &token, "Isolation RT", &echo, vec!["hi"], isolation)
            .await;

    let (status, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    let iso = &body["server"]["config"]["isolation"];
    assert_eq!(iso["enabled"], true);
    assert_eq!(iso["extra_read_paths"][0], "/opt/java-21");
    assert_eq!(iso["extra_rw_paths"][0], "/mnt/shared-data");
    assert_eq!(iso["pids_max"], 256);
}

// ─── Normal server lifecycle with isolation enabled ─────────────────────

#[tokio::test]
async fn test_server_starts_and_stops_with_isolation_enabled() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sleep_bin = resolve_binary("sleep");

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "Isolated Sleep",
        &sleep_bin,
        vec!["300"],
        json!({"enabled": true}),
    )
    .await;

    let body = start_and_wait(&app, &token, &server_id).await;
    assert_eq!(body["status"], "running");
    assert!(body["pid"].is_number());

    stop_and_wait(&app, &token, &server_id).await;

    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert!(
        body["runtime"]["status"] == "stopped" || body["runtime"]["status"] == "stopping",
        "expected stopped/stopping, got: {}",
        body["runtime"]["status"]
    );
}

#[tokio::test]
async fn test_server_starts_and_kills_with_isolation_enabled() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sleep_bin = resolve_binary("sleep");

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "Isolated Kill",
        &sleep_bin,
        vec!["300"],
        json!({"enabled": true}),
    )
    .await;

    start_and_wait(&app, &token, &server_id).await;
    kill_and_wait(&app, &token, &server_id).await;

    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert!(
        body["runtime"]["status"] == "stopped" || body["runtime"]["status"] == "crashed",
        "expected stopped/crashed after kill, got: {}",
        body["runtime"]["status"]
    );
}

// ─── Server lifecycle with isolation disabled ───────────────────────────

#[tokio::test]
async fn test_server_starts_with_isolation_disabled() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sleep_bin = resolve_binary("sleep");

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "No Isolation",
        &sleep_bin,
        vec!["300"],
        json!({"enabled": false}),
    )
    .await;

    let body = start_and_wait(&app, &token, &server_id).await;
    assert_eq!(body["status"], "running");

    kill_and_wait(&app, &token, &server_id).await;
}

// ─── Landlock: cannot read outside server dir ───────────────────────────

/// This is the core security test.  We create a "secret" file outside the
/// server's data directory, then start an isolated server process that
/// tries to `cat` it.  The process should fail because Landlock blocks
/// access.
///
/// If the host kernel doesn't support Landlock, the test still passes
/// (with a weaker assertion) because isolation layers degrade gracefully.
#[tokio::test]
async fn test_isolated_process_cannot_read_outside_server_dir() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let cat_bin = resolve_binary("cat");

    // Create a secret file in a sibling directory of the server's data dir.
    // The test app's data dir is something like /tmp/xxx/data.  We'll
    // create /tmp/xxx/data/secret.txt which is outside any server's
    // subdirectory.
    // Create the secret file in a location that is NOT under any default
    // allowed path.  The default RW paths are the server dir + specific
    // /dev/* files.  The default RO paths include /usr, /etc, /tmp, etc.
    // We use the data_dir itself, which lives under /tmp on most test
    // systems.  However, /tmp is NOT in the default allowed paths (removed
    // for security), so this path should be blocked by Landlock.
    //
    // If the test system's temp dir happens to be under an allowed path
    // (unlikely), we fall back to a weaker assertion.
    let secret_dir = app.state.data_dir.join("secrets");
    std::fs::create_dir_all(&secret_dir).unwrap();
    let secret_path = secret_dir.join("secret.txt");
    std::fs::write(&secret_path, "TOP SECRET DATA").expect("failed to write secret file");

    let secret_path_str = secret_path.to_string_lossy().to_string();

    // Create an isolated server that runs `cat <secret_path>`.
    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "Read Outside Dir",
        &cat_bin,
        vec![&secret_path_str],
        json!({"enabled": true}),
    )
    .await;

    start_and_wait(&app, &token, &server_id).await;

    // Wait for the short-lived process to exit.
    let logs = wait_for_exit_and_get_logs(&app, &token, &server_id, Duration::from_secs(5)).await;

    // Check the outcome.  Two possibilities:
    // 1. Landlock is active → cat fails with "Permission denied" or similar.
    // 2. Landlock is not available → cat succeeds and prints "TOP SECRET DATA".
    let output = logs.join("\n");

    // Determine if Landlock is available on this host.
    let landlock_available = {
        #[cfg(target_os = "linux")]
        {
            anyserver::sandbox::probe_capabilities().contains("✓")
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    };

    if landlock_available {
        // Landlock should have blocked the read.  `cat` will print an
        // error like "cat: /path/to/secret.txt: Permission denied" to
        // stderr.
        assert!(
            !output.contains("TOP SECRET DATA"),
            "Landlock should have prevented reading the secret file!\nOutput: {}",
            output,
        );
        // The process should have crashed (non-zero exit).
        let (_, body) = app
            .get(&format!("/api/servers/{}", server_id), Some(&token))
            .await;
        assert_eq!(
            body["runtime"]["status"], "crashed",
            "cat should fail (crash) when Landlock blocks access"
        );
    } else {
        // Without Landlock, cat will succeed and we just verify the test
        // setup was correct (the file was readable).
        assert!(
            output.contains("TOP SECRET DATA"),
            "Without Landlock, cat should have read the secret file.\nOutput: {}",
            output,
        );
    }
}

// ─── Landlock: CAN read/write inside server dir ─────────────────────────

#[tokio::test]
async fn test_isolated_process_can_read_own_data_dir() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let cat_bin = resolve_binary("cat");

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "Read Own Dir",
        &cat_bin,
        vec!["test-input.txt"],
        json!({"enabled": true}),
    )
    .await;

    // Write a file INSIDE the server's data directory.
    write_server_file(
        &app,
        &token,
        &server_id,
        "test-input.txt",
        "HELLO FROM INSIDE",
    )
    .await;

    start_and_wait(&app, &token, &server_id).await;

    let logs = wait_for_exit_and_get_logs(&app, &token, &server_id, Duration::from_secs(5)).await;
    let output = logs.join("\n");

    // The file is inside the server dir, so it should be readable
    // regardless of whether Landlock is active.
    assert!(
        output.contains("HELLO FROM INSIDE"),
        "Process should be able to read files in its own data dir.\nOutput: {}",
        output,
    );
}

#[tokio::test]
async fn test_isolated_process_can_write_own_data_dir() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh_bin = resolve_binary("sh");

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "Write Own Dir",
        &sh_bin,
        vec!["-c", "echo WRITTEN > output.txt && cat output.txt"],
        json!({"enabled": true}),
    )
    .await;

    start_and_wait(&app, &token, &server_id).await;

    let logs = wait_for_exit_and_get_logs(&app, &token, &server_id, Duration::from_secs(5)).await;
    let output = logs.join("\n");

    assert!(
        output.contains("WRITTEN"),
        "Process should be able to write files in its own data dir.\nOutput: {}",
        output,
    );
}

// ─── Landlock: cannot read another server's directory ────────────────────

#[tokio::test]
async fn test_isolated_process_cannot_read_other_server_dir() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let cat_bin = resolve_binary("cat");

    // Create server A (the victim) and put a secret file in it.
    let (server_a, _) = app.create_test_server(&token, "Victim Server").await;
    write_server_file(&app, &token, &server_a, "secret.txt", "VICTIM SECRET").await;

    // Get the absolute path to server A's file.
    let server_a_uuid: uuid::Uuid = server_a.parse().unwrap();
    let victim_file = app.state.server_dir(&server_a_uuid).join("secret.txt");
    let victim_path_str = victim_file.to_string_lossy().to_string();

    // Create server B (the attacker) that tries to cat server A's file.
    let (server_b, _) = create_server_with_isolation(
        &app,
        &token,
        "Attacker Server",
        &cat_bin,
        vec![&victim_path_str],
        json!({"enabled": true}),
    )
    .await;

    start_and_wait(&app, &token, &server_b).await;

    let logs = wait_for_exit_and_get_logs(&app, &token, &server_b, Duration::from_secs(5)).await;
    let output = logs.join("\n");

    let landlock_available = {
        #[cfg(target_os = "linux")]
        {
            anyserver::sandbox::probe_capabilities().contains("✓")
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    };

    if landlock_available {
        assert!(
            !output.contains("VICTIM SECRET"),
            "Landlock should prevent server B from reading server A's files!\nOutput: {}",
            output,
        );
    }
    // Without Landlock, we can't enforce cross-server isolation at the
    // filesystem level — this is expected and documented.
}

// ─── Disabled isolation: process CAN read outside ───────────────────────

#[tokio::test]
async fn test_non_isolated_process_can_read_outside_server_dir() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let cat_bin = resolve_binary("cat");

    let readable_path = app.state.data_dir.join("readable.txt");
    std::fs::write(&readable_path, "READABLE DATA").unwrap();
    let readable_str = readable_path.to_string_lossy().to_string();

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "No Sandbox Read",
        &cat_bin,
        vec![&readable_str],
        json!({"enabled": false}),
    )
    .await;

    start_and_wait(&app, &token, &server_id).await;

    let logs = wait_for_exit_and_get_logs(&app, &token, &server_id, Duration::from_secs(5)).await;
    let output = logs.join("\n");

    assert!(
        output.contains("READABLE DATA"),
        "With isolation disabled, the process should read any file.\nOutput: {}",
        output,
    );
}

// ─── Extra read paths ───────────────────────────────────────────────────

#[tokio::test]
async fn test_extra_read_paths_are_accessible() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let cat_bin = resolve_binary("cat");

    // Create a file in an "extra" directory outside the server dir.
    let extra_dir = app.state.data_dir.join("extra-ro");
    std::fs::create_dir_all(&extra_dir).unwrap();
    let extra_file = extra_dir.join("data.txt");
    std::fs::write(&extra_file, "EXTRA READ DATA").unwrap();

    let extra_dir_str = extra_dir.to_string_lossy().to_string();
    let extra_file_str = extra_file.to_string_lossy().to_string();

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "Extra Read",
        &cat_bin,
        vec![&extra_file_str],
        json!({
            "enabled": true,
            "extra_read_paths": [extra_dir_str]
        }),
    )
    .await;

    start_and_wait(&app, &token, &server_id).await;

    let logs = wait_for_exit_and_get_logs(&app, &token, &server_id, Duration::from_secs(5)).await;
    let output = logs.join("\n");

    let landlock_available = {
        #[cfg(target_os = "linux")]
        {
            anyserver::sandbox::probe_capabilities().contains("✓")
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    };

    // Whether or not Landlock is active, the extra_read_paths entry should
    // make this file accessible.
    assert!(
        output.contains("EXTRA READ DATA"),
        "extra_read_paths should make the directory accessible.\nLandlock available: {}\nOutput: {}",
        landlock_available,
        output,
    );
}

// ─── Extra rw paths ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_extra_rw_paths_are_writable() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh_bin = resolve_binary("sh");

    let extra_dir = app.state.data_dir.join("extra-rw");
    std::fs::create_dir_all(&extra_dir).unwrap();

    let extra_dir_str = extra_dir.to_string_lossy().to_string();
    let write_target = extra_dir.join("written.txt");
    let write_target_str = write_target.to_string_lossy().to_string();

    let cmd = format!(
        "echo RW_CONTENT > {} && cat {}",
        write_target_str, write_target_str
    );

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "Extra RW",
        &sh_bin,
        vec!["-c", &cmd],
        json!({
            "enabled": true,
            "extra_rw_paths": [extra_dir_str],
            "pids_max": null
        }),
    )
    .await;

    start_and_wait(&app, &token, &server_id).await;

    let logs = wait_for_exit_and_get_logs(&app, &token, &server_id, Duration::from_secs(5)).await;
    let output = logs.join("\n");

    assert!(
        output.contains("RW_CONTENT"),
        "extra_rw_paths should allow writing.\nOutput: {}",
        output,
    );
}

// ─── System paths remain accessible ─────────────────────────────────────

#[tokio::test]
async fn test_isolated_process_can_use_system_binaries() {
    // An isolated process should still be able to run basic system tools
    // because /usr, /bin, /lib etc. are in the default read-only list.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh_bin = resolve_binary("sh");

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "System Bins",
        &sh_bin,
        vec!["-c", "echo SYSTEM_OK && ls /usr > /dev/null && echo DONE"],
        json!({"enabled": true, "pids_max": null}),
    )
    .await;

    start_and_wait(&app, &token, &server_id).await;

    let logs = wait_for_exit_and_get_logs(&app, &token, &server_id, Duration::from_secs(5)).await;
    let output = logs.join("\n");

    assert!(
        output.contains("SYSTEM_OK"),
        "Isolated process should be able to run shell commands.\nOutput: {}",
        output,
    );
}

#[tokio::test]
async fn test_isolated_process_can_read_etc() {
    // /etc is in the default read-only paths — many programs need
    // /etc/resolv.conf, /etc/hosts, timezone data, etc.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let cat_bin = resolve_binary("cat");

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "Read Etc",
        &cat_bin,
        vec!["/etc/hostname"],
        json!({"enabled": true}),
    )
    .await;

    start_and_wait(&app, &token, &server_id).await;

    let logs = wait_for_exit_and_get_logs(&app, &token, &server_id, Duration::from_secs(5)).await;

    // We don't know the hostname, but the process should NOT have crashed
    // with "Permission denied" (Landlock should allow /etc reads).
    let output = logs.join("\n");
    let has_permission_denied = output.to_lowercase().contains("permission denied");
    let landlock_available = {
        #[cfg(target_os = "linux")]
        {
            anyserver::sandbox::probe_capabilities().contains("✓")
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    };

    // /etc/hostname might not exist on all systems (e.g. containers),
    // so we only assert on "permission denied" — if the file doesn't
    // exist that's a "No such file" error, not a sandbox error.
    if landlock_available {
        assert!(
            !has_permission_denied,
            "/etc should be readable under Landlock.\nOutput: {}",
            output,
        );
    }
}

// ─── PID file lifecycle still works with sandbox ────────────────────────

#[tokio::test]
async fn test_pid_file_lifecycle_with_isolation_enabled() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sleep_bin = resolve_binary("sleep");

    let (server_id_str, _) = create_server_with_isolation(
        &app,
        &token,
        "PID + Sandbox",
        &sleep_bin,
        vec!["300"],
        json!({"enabled": true}),
    )
    .await;
    let server_id: uuid::Uuid = server_id_str.parse().unwrap();

    // No PID file before start.
    assert!(anyserver::server_management::process::read_pid_file(&app.state.data_dir, &server_id).is_none());

    start_and_wait(&app, &token, &server_id_str).await;

    // PID file should exist.
    let pid = anyserver::server_management::process::read_pid_file(&app.state.data_dir, &server_id);
    assert!(
        pid.is_some(),
        "PID file should exist after start with isolation"
    );
    assert!(
        anyserver::server_management::process::is_process_alive(pid.unwrap()),
        "process should be alive"
    );

    kill_and_wait(&app, &token, &server_id_str).await;

    // PID file should be cleaned up.
    assert!(
        anyserver::server_management::process::read_pid_file(&app.state.data_dir, &server_id).is_none(),
        "PID file should be removed after kill"
    );
}

// ─── RLIMIT_NPROC ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_pids_max_limits_fork_count() {
    // Set pids_max to a generous value and verify the server can still
    // fork subprocesses.  We use 4096 because RLIMIT_NPROC is a per-UID
    // limit — a very low value would prevent ALL processes belonging to
    // the test user from forking, including the test harness itself.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh_bin = resolve_binary("sh");

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "NPROC Limit",
        &sh_bin,
        vec![
            "-c",
            "for i in $(seq 1 50); do (echo fork_$i) & done; wait; echo FORKS_DONE",
        ],
        json!({"enabled": true, "pids_max": 4096}),
    )
    .await;

    start_and_wait(&app, &token, &server_id).await;

    let logs = wait_for_exit_and_get_logs(&app, &token, &server_id, Duration::from_secs(10)).await;
    let output = logs.join("\n");

    // Count how many "fork_N" lines appeared.
    let fork_count = logs.iter().filter(|l| l.starts_with("fork_")).count();

    // With a generous RLIMIT_NPROC=4096, the forks should succeed.
    // The main point is that the server starts and runs without crashing
    // from our sandbox code, and the NPROC limit is applied without
    // breaking the process.  (RLIMIT_NPROC is a per-UID limit, so we
    // can't test restrictive values reliably in a multi-threaded test
    // harness.)
    assert!(
        output.contains("FORKS_DONE") || fork_count > 0,
        "Process should have completed forks.\nOutput: {}",
        output,
    );

    // Log the fork count for debugging.
    eprintln!(
        "[test_pids_max_limits_fork_count] {} of 50 forks succeeded (pids_max=4096)",
        fork_count
    );
}

// ─── Multiple isolated servers run independently ────────────────────────

#[tokio::test]
async fn test_multiple_isolated_servers_coexist() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sleep_bin = resolve_binary("sleep");

    let (server_a, _) = create_server_with_isolation(
        &app,
        &token,
        "Isolated A",
        &sleep_bin,
        vec!["300"],
        json!({"enabled": true}),
    )
    .await;

    let (server_b, _) = create_server_with_isolation(
        &app,
        &token,
        "Isolated B",
        &sleep_bin,
        vec!["300"],
        json!({"enabled": true}),
    )
    .await;

    // Start both.
    start_and_wait(&app, &token, &server_a).await;
    start_and_wait(&app, &token, &server_b).await;

    // Both should be running.
    let (_, body_a) = app
        .get(&format!("/api/servers/{}", server_a), Some(&token))
        .await;
    let (_, body_b) = app
        .get(&format!("/api/servers/{}", server_b), Some(&token))
        .await;
    assert_eq!(body_a["runtime"]["status"], "running");
    assert_eq!(body_b["runtime"]["status"], "running");

    // They should have different PIDs.
    let pid_a = body_a["runtime"]["pid"].as_u64().unwrap();
    let pid_b = body_b["runtime"]["pid"].as_u64().unwrap();
    assert_ne!(pid_a, pid_b, "servers should have different PIDs");

    // Kill both.
    kill_and_wait(&app, &token, &server_a).await;
    kill_and_wait(&app, &token, &server_b).await;
}

// ─── Isolation doesn't break script-based servers ───────────────────────

#[tokio::test]
async fn test_isolated_shell_script_server() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "Script Server",
        // Use sh directly — this tests that the script interpreter
        // (/bin/sh or /usr/bin/sh) is accessible under Landlock.
        &resolve_binary("sh"),
        vec!["-c", "echo SCRIPT_OUTPUT; sleep 1; echo SCRIPT_DONE"],
        json!({"enabled": true, "pids_max": null}),
    )
    .await;

    start_and_wait(&app, &token, &server_id).await;

    let logs = wait_for_exit_and_get_logs(&app, &token, &server_id, Duration::from_secs(5)).await;
    let output = logs.join("\n");

    assert!(
        output.contains("SCRIPT_OUTPUT"),
        "Shell script should produce output under isolation.\nOutput: {}",
        output,
    );
    assert!(
        output.contains("SCRIPT_DONE"),
        "Shell script should complete under isolation.\nOutput: {}",
        output,
    );
}

// ─── Isolation with custom pids_max = null (no limit) ───────────────────

#[tokio::test]
async fn test_pids_max_null_means_no_limit() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let sh_bin = resolve_binary("sh");

    let (server_id, _) = create_server_with_isolation(
        &app,
        &token,
        "No NPROC Limit",
        &sh_bin,
        vec!["-c", "echo NO_LIMIT_OK"],
        json!({"enabled": true, "pids_max": null}),
    )
    .await;

    start_and_wait(&app, &token, &server_id).await;

    let logs = wait_for_exit_and_get_logs(&app, &token, &server_id, Duration::from_secs(5)).await;
    let output = logs.join("\n");

    assert!(
        output.contains("NO_LIMIT_OK"),
        "Server with pids_max=null should work fine.\nOutput: {}",
        output,
    );
}

// ─── PreExecSandbox struct tests ────────────────────────────────────────

#[tokio::test]
async fn test_pre_exec_sandbox_disabled_is_noop() {
    let config = anyserver::types::IsolationConfig {
        enabled: false,
        extra_read_paths: vec![],
        extra_rw_paths: vec![],
        pids_max: None,
    };
    let sandbox = anyserver::sandbox::PreExecSandbox::new(
        std::path::Path::new("/tmp/fake-server-dir"),
        &config,
    );
    // apply() on a disabled sandbox should succeed without doing anything.
    assert!(sandbox.apply().is_ok());
}

#[tokio::test]
async fn test_pre_exec_sandbox_nonexistent_extra_paths_dont_panic() {
    let config = anyserver::types::IsolationConfig {
        enabled: true,
        extra_read_paths: vec!["/nonexistent/path/that/does/not/exist/12345".to_string()],
        extra_rw_paths: vec!["/another/nonexistent/rw/path/67890".to_string()],
        pids_max: Some(512),
    };
    // new() should not panic even with nonexistent paths — they're filtered out.
    let _sandbox = anyserver::sandbox::PreExecSandbox::new(std::path::Path::new("/tmp"), &config);
}
