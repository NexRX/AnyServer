//! Tests for the orphan-process listing and killing flow (ticket 007).
//!
//! These tests exercise the two-step "list then kill" API:
//!   - `GET  /api/servers/:id/directory-processes`
//!   - `POST /api/servers/:id/kill-directory-processes`

use std::process::Command as StdCommand;
use std::time::Duration;

use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

use crate::common::{resolve_binary, TestApp};

// ─── Helpers ──────────────────────────────────────────────────────────

/// Spawn a long-lived background process whose cwd is set to `dir`.
/// Uses `setsid` so the process group can be killed cleanly.
#[cfg(unix)]
fn spawn_in_dir(binary: &str, args: &[&str], dir: &std::path::Path) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    let mut cmd = StdCommand::new(binary);
    cmd.args(args);
    cmd.current_dir(dir);
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn()
        .unwrap_or_else(|e| panic!("failed to spawn `{}` in {:?}: {}", binary, dir, e))
}

/// Kill a process by PID (cleanup helper).
#[cfg(unix)]
fn cleanup_pid(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

/// Create a server via the API and return its UUID string.
async fn create_server(app: &TestApp, token: &str, name: &str) -> String {
    let echo = resolve_binary("echo");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(token),
            json!({
                "config": {
                    "name": name,
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
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    body["server"]["id"].as_str().unwrap().to_string()
}

// ─── List endpoint — empty directory ─────────────────────────────────

#[tokio::test]
async fn test_list_directory_processes_empty() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let id = create_server(&app, &token, "Empty Server").await;

    let (status, body) = app
        .get(
            &format!("/api/servers/{}/directory-processes", id),
            Some(&token),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 0);
    assert!(body["processes"].as_array().unwrap().is_empty());
}

// ─── List endpoint — detects a process whose cwd is inside the dir ───

#[tokio::test]
async fn test_list_directory_processes_finds_orphan() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let id = create_server(&app, &token, "Orphan Server").await;

    // Ensure the server directory exists
    let server_dir = app.state.server_dir(&uuid::Uuid::parse_str(&id).unwrap());
    std::fs::create_dir_all(&server_dir).unwrap();

    let sleep_bin = resolve_binary("sleep");
    let mut child = spawn_in_dir(&sleep_bin, &["300"], &server_dir);
    let child_pid = child.id();

    // Give the process a moment to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, body) = app
        .get(
            &format!("/api/servers/{}/directory-processes", id),
            Some(&token),
        )
        .await;

    // Clean up regardless of assertion outcome
    cleanup_pid(child_pid);
    let _ = child.wait();

    assert_eq!(status, StatusCode::OK);
    assert!(
        body["count"].as_u64().unwrap() >= 1,
        "expected at least 1 process, got: {:?}",
        body
    );

    let processes = body["processes"].as_array().unwrap();
    let found = processes.iter().any(|p| p["pid"] == child_pid);
    assert!(
        found,
        "expected to find PID {} in process list: {:?}",
        child_pid, processes
    );

    // Verify the response includes command and args fields
    let entry = processes.iter().find(|p| p["pid"] == child_pid).unwrap();
    assert!(
        entry["command"].as_str().is_some(),
        "process entry should have a command field"
    );
    assert!(
        entry["args"].as_array().is_some(),
        "process entry should have an args field"
    );
}

// ─── List endpoint returns command name and args ─────────────────────

#[tokio::test]
async fn test_list_directory_processes_includes_args() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let id = create_server(&app, &token, "Args Server").await;

    let server_dir = app.state.server_dir(&uuid::Uuid::parse_str(&id).unwrap());
    std::fs::create_dir_all(&server_dir).unwrap();

    let sleep_bin = resolve_binary("sleep");
    let mut child = spawn_in_dir(&sleep_bin, &["999"], &server_dir);
    let child_pid = child.id();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, body) = app
        .get(
            &format!("/api/servers/{}/directory-processes", id),
            Some(&token),
        )
        .await;

    cleanup_pid(child_pid);
    let _ = child.wait();

    assert_eq!(status, StatusCode::OK);
    let processes = body["processes"].as_array().unwrap();
    let entry = processes
        .iter()
        .find(|p| p["pid"] == child_pid)
        .expect("should find the spawned process");

    // The command field should be a non-empty string
    let command = entry["command"].as_str().unwrap();
    assert!(
        !command.is_empty(),
        "command should be non-empty, got: {:?}",
        command
    );

    // The args should contain "999" (and typically the binary path too)
    let args = entry["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(
        args.iter().any(|a| a == "999"),
        "args should contain '999', got: {:?}",
        args
    );
}

// ─── Kill endpoint — no processes ────────────────────────────────────

#[tokio::test]
async fn test_kill_directory_processes_empty() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let id = create_server(&app, &token, "Kill Empty").await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/kill-directory-processes", id),
            Some(&token),
            json!({}),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["killed"], 0);
    assert_eq!(body["failed"], 0);
    assert!(body["processes"].as_array().unwrap().is_empty());
}

// ─── Kill endpoint — kills an orphan and reports success per PID ─────

#[tokio::test]
async fn test_kill_directory_processes_kills_orphan() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let id = create_server(&app, &token, "Kill Orphan").await;

    let server_dir = app.state.server_dir(&uuid::Uuid::parse_str(&id).unwrap());
    std::fs::create_dir_all(&server_dir).unwrap();

    let sleep_bin = resolve_binary("sleep");
    let mut child = spawn_in_dir(&sleep_bin, &["300"], &server_dir);
    let child_pid = child.id();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify the process is listed first
    let (_, list_body) = app
        .get(
            &format!("/api/servers/{}/directory-processes", id),
            Some(&token),
        )
        .await;
    assert!(
        list_body["count"].as_u64().unwrap() >= 1,
        "pre-condition: process should be listed"
    );

    // Now kill
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/kill-directory-processes", id),
            Some(&token),
            json!({}),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body["killed"].as_u64().unwrap() >= 1,
        "should have killed at least 1 process"
    );

    // Check per-process result
    let processes = body["processes"].as_array().unwrap();
    let entry = processes.iter().find(|p| p["pid"] == child_pid);
    assert!(entry.is_some(), "result should include the killed PID");
    assert_eq!(
        entry.unwrap()["success"],
        true,
        "kill should be reported as successful"
    );

    // Wait for the child to actually die
    let _ = child.wait();

    // After killing, the list endpoint should show the directory is clean
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (_, list_after) = app
        .get(
            &format!("/api/servers/{}/directory-processes", id),
            Some(&token),
        )
        .await;
    assert_eq!(
        list_after["count"], 0,
        "directory should be clean after kill"
    );
}

// ─── Kill endpoint reports per-PID results with command names ────────

#[tokio::test]
async fn test_kill_directory_processes_reports_command_names() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let id = create_server(&app, &token, "Command Names").await;

    let server_dir = app.state.server_dir(&uuid::Uuid::parse_str(&id).unwrap());
    std::fs::create_dir_all(&server_dir).unwrap();

    let sleep_bin = resolve_binary("sleep");
    let mut child = spawn_in_dir(&sleep_bin, &["300"], &server_dir);
    let child_pid = child.id();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/kill-directory-processes", id),
            Some(&token),
            json!({}),
        )
        .await;

    cleanup_pid(child_pid);
    let _ = child.wait();

    assert_eq!(status, StatusCode::OK);
    let processes = body["processes"].as_array().unwrap();
    let entry = processes
        .iter()
        .find(|p| p["pid"] == child_pid)
        .expect("should find the killed PID in the results");

    // The command field should be present and non-empty
    let command = entry["command"].as_str().unwrap();
    assert!(
        !command.is_empty(),
        "command name should be reported, got empty string"
    );
    assert!(
        entry.get("success").is_some(),
        "success field should be present"
    );
}

// ─── Multiple processes are all listed and killed ────────────────────

#[tokio::test]
async fn test_multiple_processes_listed_and_killed() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let id = create_server(&app, &token, "Multi Process").await;

    let server_dir = app.state.server_dir(&uuid::Uuid::parse_str(&id).unwrap());
    std::fs::create_dir_all(&server_dir).unwrap();

    let sleep_bin = resolve_binary("sleep");
    let mut child1 = spawn_in_dir(&sleep_bin, &["301"], &server_dir);
    let mut child2 = spawn_in_dir(&sleep_bin, &["302"], &server_dir);
    let pid1 = child1.id();
    let pid2 = child2.id();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // List should show both
    let (status, body) = app
        .get(
            &format!("/api/servers/{}/directory-processes", id),
            Some(&token),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body["count"].as_u64().unwrap() >= 2,
        "should find at least 2 processes, got: {:?}",
        body
    );

    let pids: Vec<u64> = body["processes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["pid"].as_u64().unwrap())
        .collect();
    assert!(pids.contains(&(pid1 as u64)), "should contain PID {}", pid1);
    assert!(pids.contains(&(pid2 as u64)), "should contain PID {}", pid2);

    // Kill should report both
    let (status, kill_body) = app
        .post(
            &format!("/api/servers/{}/kill-directory-processes", id),
            Some(&token),
            json!({}),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        kill_body["killed"].as_u64().unwrap() >= 2,
        "should have killed at least 2"
    );

    let killed_pids: Vec<u64> = kill_body["processes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["pid"].as_u64().unwrap())
        .collect();
    assert!(
        killed_pids.contains(&(pid1 as u64)),
        "kill result should contain PID {}",
        pid1
    );
    assert!(
        killed_pids.contains(&(pid2 as u64)),
        "kill result should contain PID {}",
        pid2
    );

    let _ = child1.wait();
    let _ = child2.wait();
}

// ─── Auth: unauthenticated requests are rejected ─────────────────────

#[tokio::test]
async fn test_list_directory_processes_requires_auth() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let id = create_server(&app, &token, "Auth Test").await;

    let (status, _) = app
        .get(&format!("/api/servers/{}/directory-processes", id), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_kill_directory_processes_requires_auth() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let id = create_server(&app, &token, "Auth Kill Test").await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/kill-directory-processes", id),
            None,
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ─── Auth: non-admin users are forbidden ─────────────────────────────

#[tokio::test]
async fn test_list_directory_processes_requires_admin() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _) = app.setup_admin_and_user().await;
    let id = create_server(&app, &admin_token, "Admin Only List").await;

    let (status, _) = app
        .get(
            &format!("/api/servers/{}/directory-processes", id),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_kill_directory_processes_requires_admin() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _) = app.setup_admin_and_user().await;
    let id = create_server(&app, &admin_token, "Admin Only Kill").await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/kill-directory-processes", id),
            Some(&user_token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ─── 404 for nonexistent server ──────────────────────────────────────

#[tokio::test]
async fn test_list_directory_processes_not_found() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let fake_id = Uuid::new_v4();

    let (status, _) = app
        .get(
            &format!("/api/servers/{}/directory-processes", fake_id),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_kill_directory_processes_not_found() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let fake_id = Uuid::new_v4();

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/kill-directory-processes", fake_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── Processes in other server dirs are not listed ────────────────────

#[tokio::test]
async fn test_processes_isolated_between_servers() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let id_a = create_server(&app, &token, "Server A").await;
    let id_b = create_server(&app, &token, "Server B").await;

    let dir_a = app.state.server_dir(&uuid::Uuid::parse_str(&id_a).unwrap());
    let dir_b = app.state.server_dir(&uuid::Uuid::parse_str(&id_b).unwrap());
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let sleep_bin = resolve_binary("sleep");
    let mut child_a = spawn_in_dir(&sleep_bin, &["300"], &dir_a);
    let pid_a = child_a.id();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Listing Server B should NOT show the process from Server A
    let (status, body) = app
        .get(
            &format!("/api/servers/{}/directory-processes", id_b),
            Some(&token),
        )
        .await;

    cleanup_pid(pid_a);
    let _ = child_a.wait();

    assert_eq!(status, StatusCode::OK);
    let pids: Vec<u64> = body["processes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["pid"].as_u64().unwrap())
        .collect();
    assert!(
        !pids.contains(&(pid_a as u64)),
        "Server B should not list Server A's process (PID {}), but got: {:?}",
        pid_a,
        pids
    );
}
