//! Tests for process reconciliation on startup (ticket 004).
//!
//! When AnyServer is killed externally while managing running server
//! processes, those child processes can survive as orphans.  On the next
//! startup, reconciliation should detect them via PID files and either
//! re-adopt them (if still alive) or clean up the stale PID file (if dead).

use std::process::Command as StdCommand;
use std::time::Duration;

use axum::http::StatusCode;
use serde_json::json;

use crate::common::{resolve_binary, TestApp};

use anyserver::server_management::process::{
    is_process_alive, pid_file_path, read_pid_file, reconcile_processes, remove_pid_file,
    write_pid_file,
};
use anyserver::types::ServerStatus;

/// Spawn a long-lived background process in its own session (via `setsid`),
/// exactly like AnyServer does in production with `pre_exec(|| { setsid(); })`.
///
/// This is critical for tests that later call `kill_server` / the kill API,
/// because those functions send signals to the **process group** (`kill(-pid)`).
/// Without `setsid` the PGID wouldn't equal the PID and the signal would
/// miss.
#[cfg(unix)]
fn spawn_orphan_process(binary: &str, args: &[&str]) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    let mut cmd = StdCommand::new(binary);
    cmd.args(args);
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn()
        .unwrap_or_else(|e| panic!("failed to spawn orphan process `{}`: {}", binary, e))
}

/// Kill a process by positive PID (cleanup helper).
#[cfg(unix)]
fn kill_process(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
    // Also try the process group in case setsid was used.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

// ─── PID file helper unit tests ─────────────────────────────────────────

#[tokio::test]
async fn test_write_and_read_pid_file() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id_str, _) = app.create_test_server(&token, "PID Read/Write").await;
    let server_id: uuid::Uuid = server_id_str.parse().unwrap();

    // No PID file yet.
    assert!(read_pid_file(&app.state.data_dir, &server_id).is_none());

    // Write and read back.
    write_pid_file(&app.state.data_dir, &server_id, 12345);
    assert_eq!(read_pid_file(&app.state.data_dir, &server_id), Some(12345));

    // Overwrite with a different PID.
    write_pid_file(&app.state.data_dir, &server_id, 99999);
    assert_eq!(read_pid_file(&app.state.data_dir, &server_id), Some(99999));

    // Remove.
    remove_pid_file(&app.state.data_dir, &server_id);
    assert!(read_pid_file(&app.state.data_dir, &server_id).is_none());
}

#[tokio::test]
async fn test_remove_pid_file_is_idempotent() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id_str, _) = app
        .create_test_server(&token, "PID Remove Idempotent")
        .await;
    let server_id: uuid::Uuid = server_id_str.parse().unwrap();

    // Removing a non-existent PID file should not panic or error.
    remove_pid_file(&app.state.data_dir, &server_id);
    remove_pid_file(&app.state.data_dir, &server_id);
}

#[tokio::test]
async fn test_pid_file_path_is_deterministic() {
    let app = TestApp::new().await;
    let server_id = uuid::Uuid::new_v4();

    let path1 = pid_file_path(&app.state.data_dir, &server_id);
    let path2 = pid_file_path(&app.state.data_dir, &server_id);
    assert_eq!(path1, path2);
    assert!(path1.ends_with(".anyserver.pid"));
    assert!(path1.to_string_lossy().contains(&server_id.to_string()));
}

// ─── is_process_alive ───────────────────────────────────────────────────

#[tokio::test]
async fn test_is_process_alive_for_current_process() {
    let my_pid = std::process::id();
    assert!(is_process_alive(my_pid), "our own process should be alive");
}

#[tokio::test]
async fn test_is_process_alive_for_dead_process() {
    // Spawn a process and wait for it to exit, then check that it's dead.
    let true_bin = resolve_binary("true"); // `true` exits immediately
    let mut child = StdCommand::new(&true_bin)
        .spawn()
        .expect("failed to spawn `true`");
    let pid = child.id();

    // Reap the child so the OS can recycle the PID.
    child.wait().expect("failed to wait for `true`");

    // Give the OS a moment to fully clean up.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // After wait() the zombie is reaped; the PID should no longer be alive.
    assert!(
        !is_process_alive(pid),
        "a reaped process should not be alive"
    );
}

// ─── Reconciliation with a live orphaned process ────────────────────────

#[tokio::test]
#[allow(clippy::zombie_processes)]
async fn test_reconcile_alive_process_re_adopts_as_running() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Reconcile Alive",
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
    let server_id: uuid::Uuid = body["server"]["id"].as_str().unwrap().parse().unwrap();

    // Spawn a real background process (with setsid, like AnyServer does).
    let child = spawn_orphan_process(&sleep_bin, &["300"]);
    let pid = child.id();

    // Manually write a PID file — simulating what AnyServer would have done
    // before it was killed.
    write_pid_file(&app.state.data_dir, &server_id, pid);

    // Confirm no handle exists yet.
    let rt = app.state.process_manager.get_runtime(&server_id);
    assert_eq!(rt.status, ServerStatus::Stopped);

    // Run reconciliation.
    reconcile_processes(&app.state).await;

    // The server should now be reported as Running with the correct PID.
    let rt = app.state.process_manager.get_runtime(&server_id);
    assert_eq!(
        rt.status,
        ServerStatus::Running,
        "reconciled server should be Running"
    );
    assert_eq!(rt.pid, Some(pid), "reconciled PID should match");

    // The log buffer should contain the informational message.
    let logs = app.state.process_manager.get_log_buffer(&server_id);
    assert!(
        logs.iter()
            .any(|l| l.line.contains("already running") && l.line.contains(&pid.to_string())),
        "expected informational log about orphaned process, got: {:?}",
        logs,
    );

    // Clean up: kill the orphan we spawned.
    kill_process(pid);
}

#[tokio::test]
#[allow(clippy::zombie_processes)]
async fn test_reconcile_alive_process_is_visible_via_api() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Reconcile API Visible",
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
    let server_id_str = body["server"]["id"].as_str().unwrap().to_string();
    let server_id: uuid::Uuid = server_id_str.parse().unwrap();

    // Spawn a real process (with setsid) and write a PID file.
    let child = spawn_orphan_process(&sleep_bin, &["300"]);
    let pid = child.id();
    write_pid_file(&app.state.data_dir, &server_id, pid);

    // Reconcile.
    reconcile_processes(&app.state).await;

    // The GET /api/servers/:id endpoint should reflect Running status.
    let (status, body) = app
        .get(&format!("/api/servers/{}", server_id_str), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["runtime"]["status"], "running");
    assert_eq!(body["runtime"]["pid"], pid);

    // Clean up.
    kill_process(pid);
}

// ─── Reconciliation with a dead (stale) PID ─────────────────────────────

#[tokio::test]
async fn test_reconcile_dead_process_cleans_up_pid_file() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Reconcile Dead",
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
    let server_id: uuid::Uuid = body["server"]["id"].as_str().unwrap().parse().unwrap();

    // Spawn a short-lived process, wait for it to die, then write its PID.
    let true_bin = resolve_binary("true");
    let mut child = StdCommand::new(&true_bin)
        .spawn()
        .expect("failed to spawn `true`");
    let pid = child.id();
    child.wait().expect("failed to wait for `true`");

    // Give the OS a moment to fully reap the process.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Write a stale PID file.
    write_pid_file(&app.state.data_dir, &server_id, pid);
    assert!(read_pid_file(&app.state.data_dir, &server_id).is_some());

    // Reconcile.
    reconcile_processes(&app.state).await;

    // The server should remain Stopped (default when no handle or handle is Stopped).
    let rt = app.state.process_manager.get_runtime(&server_id);
    assert_eq!(
        rt.status,
        ServerStatus::Stopped,
        "server with dead PID should be Stopped after reconciliation"
    );

    // The PID file should be removed.
    assert!(
        read_pid_file(&app.state.data_dir, &server_id).is_none(),
        "stale PID file should be cleaned up"
    );
}

// ─── PID file lifecycle during normal start/stop ────────────────────────

#[tokio::test]
async fn test_pid_file_written_on_start_and_removed_on_stop() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "PID Lifecycle",
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
    let server_id_str = body["server"]["id"].as_str().unwrap().to_string();
    let server_id: uuid::Uuid = server_id_str.parse().unwrap();

    // No PID file before start.
    assert!(read_pid_file(&app.state.data_dir, &server_id).is_none());

    // Start the server.
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id_str),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "running");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // PID file should now exist and contain the correct PID.
    let pid_from_api = body["pid"].as_u64().unwrap() as u32;
    let pid_from_file = read_pid_file(&app.state.data_dir, &server_id);
    assert_eq!(
        pid_from_file,
        Some(pid_from_api),
        "PID file should contain the same PID as the API response"
    );

    // Stop the server.
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/stop", server_id_str),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Wait for the process to fully exit and the monitor task to clean up.
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // PID file should be gone.
    assert!(
        read_pid_file(&app.state.data_dir, &server_id).is_none(),
        "PID file should be removed after stop"
    );
}

#[tokio::test]
async fn test_pid_file_removed_on_kill() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "PID Kill Cleanup",
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
    let server_id_str = body["server"]["id"].as_str().unwrap().to_string();
    let server_id: uuid::Uuid = server_id_str.parse().unwrap();

    // Start.
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/start", server_id_str),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        read_pid_file(&app.state.data_dir, &server_id).is_some(),
        "PID file should exist while running"
    );

    // Kill.
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/kill", server_id_str),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    tokio::time::sleep(Duration::from_millis(1000)).await;

    assert!(
        read_pid_file(&app.state.data_dir, &server_id).is_none(),
        "PID file should be removed after kill"
    );
}

// ─── Auto-start doesn't double-launch a reconciled server ───────────────

#[tokio::test]
#[allow(clippy::zombie_processes)]
async fn test_auto_start_skips_already_running_reconciled_server() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");

    // Create a server with auto_start enabled.
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Auto Start Reconcile",
                    "binary": sleep_bin,
                    "args": ["300"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": true,
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
    let server_id: uuid::Uuid = body["server"]["id"].as_str().unwrap().parse().unwrap();

    // Spawn a background process (with setsid) and write its PID file.
    let child = spawn_orphan_process(&sleep_bin, &["300"]);
    let orphan_pid = child.id();
    write_pid_file(&app.state.data_dir, &server_id, orphan_pid);

    // Run reconciliation.
    reconcile_processes(&app.state).await;

    // The server should now show as Running.
    let rt = app.state.process_manager.get_runtime(&server_id);
    assert_eq!(rt.status, ServerStatus::Running);
    assert_eq!(rt.pid, Some(orphan_pid));

    // Now try to start the server — this simulates what auto-start would do.
    // It should fail with a Conflict because it's already running.
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "starting an already-running reconciled server should be rejected: {:?}",
        body,
    );

    // Verify the original PID is still the one tracked.
    let rt = app.state.process_manager.get_runtime(&server_id);
    assert_eq!(
        rt.pid,
        Some(orphan_pid),
        "PID should not have changed — the original orphan should still be tracked"
    );

    // Clean up.
    kill_process(orphan_pid);
}

// ─── Reconciliation with no PID file is a no-op ────────────────────────

#[tokio::test]
async fn test_reconcile_no_pid_file_leaves_server_stopped() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "No PID File",
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
    let server_id: uuid::Uuid = body["server"]["id"].as_str().unwrap().parse().unwrap();

    // No PID file — reconciliation should be a no-op.
    reconcile_processes(&app.state).await;

    let rt = app.state.process_manager.get_runtime(&server_id);
    assert_eq!(
        rt.status,
        ServerStatus::Stopped,
        "server without PID file should remain Stopped"
    );
}

// ─── Multiple servers reconcile independently ───────────────────────────

#[tokio::test]
#[allow(clippy::zombie_processes)]
async fn test_reconcile_multiple_servers_mixed_alive_and_dead() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");
    let true_bin = resolve_binary("true");

    // Server A — will have a live orphan.
    let (_, body_a) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Multi Alive",
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
    let server_a: uuid::Uuid = body_a["server"]["id"].as_str().unwrap().parse().unwrap();

    // Server B — will have a stale PID.
    let (_, body_b) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Multi Dead",
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
    let server_b: uuid::Uuid = body_b["server"]["id"].as_str().unwrap().parse().unwrap();

    // Server C — no PID file at all.
    let (_, body_c) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Multi No PID",
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
    let server_c: uuid::Uuid = body_c["server"]["id"].as_str().unwrap().parse().unwrap();

    // Server A: spawn a live process (with setsid).
    let child_a = spawn_orphan_process(&sleep_bin, &["300"]);
    let pid_a = child_a.id();
    write_pid_file(&app.state.data_dir, &server_a, pid_a);

    // Server B: spawn and immediately wait for exit, then write stale PID.
    let mut child_b = StdCommand::new(&true_bin)
        .spawn()
        .expect("failed to spawn true for server B");
    let pid_b = child_b.id();
    child_b.wait().unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    write_pid_file(&app.state.data_dir, &server_b, pid_b);

    // Server C: no PID file.

    // Reconcile.
    reconcile_processes(&app.state).await;

    // Server A: Running.
    let rt_a = app.state.process_manager.get_runtime(&server_a);
    assert_eq!(
        rt_a.status,
        ServerStatus::Running,
        "Server A should be Running"
    );
    assert_eq!(rt_a.pid, Some(pid_a));

    // Server B: Stopped, PID file cleaned up.
    let rt_b = app.state.process_manager.get_runtime(&server_b);
    assert_eq!(
        rt_b.status,
        ServerStatus::Stopped,
        "Server B should be Stopped"
    );
    assert!(
        read_pid_file(&app.state.data_dir, &server_b).is_none(),
        "Server B stale PID file should be removed"
    );

    // Server C: Stopped, no PID file.
    let rt_c = app.state.process_manager.get_runtime(&server_c);
    assert_eq!(
        rt_c.status,
        ServerStatus::Stopped,
        "Server C should be Stopped"
    );
    assert!(read_pid_file(&app.state.data_dir, &server_c).is_none());

    // Clean up.
    kill_process(pid_a);
}

// ─── Reconciled server can be stopped/killed via API ────────────────────

#[tokio::test]
async fn test_reconciled_server_can_be_killed() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");
    let (_, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Reconcile Then Kill",
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
    let server_id_str = body["server"]["id"].as_str().unwrap().to_string();
    let server_id: uuid::Uuid = server_id_str.parse().unwrap();

    // Spawn a live process (with setsid, matching production) and write PID file.
    // We keep the `Child` handle so we can reap the zombie after the kill —
    // in production the orphan is reparented to init which reaps it, but in
    // tests *we* are the parent so we must call wait() ourselves.
    let mut child = spawn_orphan_process(&sleep_bin, &["300"]);
    let pid = child.id();
    write_pid_file(&app.state.data_dir, &server_id, pid);

    // Reconcile.
    reconcile_processes(&app.state).await;

    let rt = app.state.process_manager.get_runtime(&server_id);
    assert_eq!(rt.status, ServerStatus::Running);

    // Kill via API — this sends SIGKILL to the process group (-pid).
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/kill", server_id_str),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let rt = app.state.process_manager.get_runtime(&server_id);
    assert!(
        rt.status == ServerStatus::Stopped || rt.status == ServerStatus::Crashed,
        "expected Stopped or Crashed after kill, got: {:?}",
        rt.status,
    );

    // Reap the child so it transitions from zombie to fully dead.
    // In production, init/systemd does this automatically because the
    // orphan was reparented when the old AnyServer process died.
    let _ = child.wait();

    // The process should actually be dead now.
    assert!(!is_process_alive(pid), "process should be dead after kill");
}

// ─── PID file written on start survives into the file system ────────────

#[tokio::test]
async fn test_pid_file_contains_correct_pid_after_start() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let sleep_bin = resolve_binary("sleep");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "PID Content Check",
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
    let server_id_str = body["server"]["id"].as_str().unwrap().to_string();
    let server_id: uuid::Uuid = server_id_str.parse().unwrap();

    // Start the server.
    let (status, start_body) = app
        .post(
            &format!("/api/servers/{}/start", server_id_str),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Read the raw PID file and compare to the API-reported PID.
    let pid_path = pid_file_path(&app.state.data_dir, &server_id);
    let raw_contents =
        std::fs::read_to_string(&pid_path).expect("PID file should exist after start");
    let file_pid: u32 = raw_contents
        .trim()
        .parse()
        .expect("PID file should contain a number");
    let api_pid = start_body["pid"].as_u64().unwrap() as u32;
    assert_eq!(
        file_pid, api_pid,
        "PID file content should match API-reported PID"
    );

    // Verify the process is actually alive.
    assert!(is_process_alive(file_pid));

    // Clean up.
    let _ = app
        .post(
            &format!("/api/servers/{}/kill", server_id_str),
            Some(&token),
            json!({}),
        )
        .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
}
