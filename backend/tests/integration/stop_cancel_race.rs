//! Tests for stop/cancel/restart race conditions (ticket 3-002).
//!
//! These verify that:
//! - Stale cancellation tokens from a previous stop attempt do not affect new stop attempts.
//! - `start_server()` is rejected while the server is in `Stopping` state.
//! - `cancel_stop_server()` returns an error if the server is not stopping.
//! - CancellationToken-based lifecycle is correct.
//!
//! To keep the server in `stopping` state long enough for tests to observe,
//! we configure `stop_steps` with a `sleep` action.  This causes `stop_server`
//! to spend several seconds executing stop steps before entering the grace
//! period, giving tests a reliable window to interact with the lifecycle.
//!
//! Because the stop API endpoint blocks until `stop_server()` completes, we
//! spawn the stop request in a background task so the test can observe and
//! interact with the intermediate `stopping` state.

use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use serde_json::json;

use crate::common::{resolve_binary, TestApp};

/// Helper: create a long-running server (`sleep 300`) whose stop pipeline
/// includes a 2-second Sleep step, keeping it in `stopping` state long
/// enough for tests to observe and interact with the lifecycle.
async fn create_slow_stop_server(app: &TestApp, token: &str) -> String {
    let sleep_bin = resolve_binary("sleep");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(token),
            json!({
                "config": {
                    "name": "Race Test Server",
                    "binary": sleep_bin,
                    "args": ["300"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 1,
                    "stop_command": null,
                    "stop_timeout_secs": 5,
                    "stop_steps": [
                        {
                            "name": "Wait before kill",
                            "action": { "type": "sleep", "seconds": 2 }
                        }
                    ],
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    body["server"]["id"].as_str().unwrap().to_string()
}

/// Helper: create a server with a fast stop (no stop_steps, short timeout)
/// for tests that just need start/stop to complete quickly.
async fn create_fast_stop_server(app: &TestApp, token: &str) -> String {
    let sleep_bin = resolve_binary("sleep");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(token),
            json!({
                "config": {
                    "name": "Fast Stop Server",
                    "binary": sleep_bin,
                    "args": ["300"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 1,
                    "stop_command": null,
                    "stop_timeout_secs": 2,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    body["server"]["id"].as_str().unwrap().to_string()
}

/// Helper: start the server and wait briefly for it to be Running.
async fn start_and_wait(app: &TestApp, token: &str, server_id: &str) {
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(token),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "start failed: {:?}", body);

    // Poll until running (max 5s).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let (_, body) = app
            .get(&format!("/api/servers/{}", server_id), Some(token))
            .await;
        if body["runtime"]["status"].as_str() == Some("running") {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "Server did not reach Running within 5s. Last status: {:?}",
                body["runtime"]["status"]
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Helper: spawn a stop request in a background task so the test can observe
/// the intermediate `stopping` state while the stop endpoint blocks.
///
/// Returns a JoinHandle that resolves to the stop request's StatusCode.
fn spawn_stop(
    app: Arc<TestApp>,
    token: String,
    server_id: String,
) -> tokio::task::JoinHandle<StatusCode> {
    tokio::spawn(async move {
        let (status, _) = app
            .post(
                &format!("/api/servers/{}/stop", server_id),
                Some(&token),
                json!({}),
            )
            .await;
        status
    })
}

/// Helper: cancel a stop.
async fn request_cancel_stop(
    app: &TestApp,
    token: &str,
    server_id: &str,
) -> (StatusCode, serde_json::Value) {
    app.post(
        &format!("/api/servers/{}/cancel-stop", server_id),
        Some(token),
        json!({}),
    )
    .await
}

/// Helper: poll until the server reaches a specific status (or timeout).
async fn wait_for_status(app: &TestApp, token: &str, server_id: &str, target: &str, secs: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let (_, body) = app
            .get(&format!("/api/servers/{}", server_id), Some(token))
            .await;
        if body["runtime"]["status"].as_str() == Some(target) {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "Server did not reach '{}' within {}s. Last body: {:?}",
                target, secs, body["runtime"]
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Helper: kill the server quickly for cleanup.
async fn kill_server(app: &TestApp, token: &str, server_id: &str) {
    let _ = app
        .post(
            &format!("/api/servers/{}/kill", server_id),
            Some(token),
            json!({}),
        )
        .await;
    // Best-effort wait for stopped.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let (_, body) = app
            .get(&format!("/api/servers/{}", server_id), Some(token))
            .await;
        let s = body["runtime"]["status"].as_str().unwrap_or("");
        if s == "stopped" || s == "crashed" {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            return; // give up on cleanup
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Wrap TestApp in an Arc so it can be shared with spawned tasks.
/// The tests create the TestApp, wrap it, and pass the Arc to helpers.
struct SharedTestApp {
    inner: Arc<TestApp>,
}

impl SharedTestApp {
    async fn new() -> Self {
        Self {
            inner: Arc::new(TestApp::new().await),
        }
    }

    fn app(&self) -> &TestApp {
        &self.inner
    }

    fn arc(&self) -> Arc<TestApp> {
        Arc::clone(&self.inner)
    }
}

/// cancel_stop when the server is not stopping should return 409 Conflict.
#[tokio::test]
async fn test_cancel_stop_when_not_stopping_returns_conflict() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;
    let server_id = create_fast_stop_server(&app, &token).await;

    // Server hasn't been started — cancel stop should fail.
    let (status, body) = request_cancel_stop(&app, &token, &server_id).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "cancel-stop on non-started server should be 409: {:?}",
        body
    );

    // Start and wait for Running.
    start_and_wait(&app, &token, &server_id).await;

    // Server is Running — cancel stop should also fail.
    let (status, body) = request_cancel_stop(&app, &token, &server_id).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "cancel-stop on running server should be 409: {:?}",
        body
    );

    // Clean up.
    kill_server(&app, &token, &server_id).await;
}

/// Starting a server while it is in Stopping state should return 409 Conflict.
#[tokio::test]
async fn test_start_while_stopping_returns_conflict() {
    let sta = SharedTestApp::new().await;
    let token = sta.app().setup_admin("admin", TestApp::TEST_PASSWORD).await;
    let server_id = create_slow_stop_server(sta.app(), &token).await;

    start_and_wait(sta.app(), &token, &server_id).await;

    // Spawn stop in background so we can observe `stopping` state.
    let stop_handle = spawn_stop(sta.arc(), token.clone(), server_id.clone());

    // Wait for the server to enter Stopping state.
    wait_for_status(sta.app(), &token, &server_id, "stopping", 5).await;

    // Now try to start while still stopping — should be rejected.
    let (status, body) = sta
        .app()
        .post(
            &format!("/api/servers/{}/start", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "start-while-stopping should be 409: {:?}",
        body
    );
    assert!(
        body["error"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("stopping"),
        "Error message should mention 'stopping': {:?}",
        body
    );

    // Cancel the stop so we don't wait for the full sleep step.
    let (cancel_status, _) = request_cancel_stop(sta.app(), &token, &server_id).await;
    assert_eq!(cancel_status, StatusCode::OK);

    // Wait for the background stop task to finish.
    let _ = stop_handle.await;

    // Final cleanup.
    kill_server(sta.app(), &token, &server_id).await;
}

/// Stop → Cancel → Stop should work correctly: the second stop should
/// execute normally and not be falsely cancelled by a stale token.
#[tokio::test]
async fn test_stop_cancel_stop_no_stale_cancellation() {
    let sta = SharedTestApp::new().await;
    let token = sta.app().setup_admin("admin", TestApp::TEST_PASSWORD).await;
    let server_id = create_slow_stop_server(sta.app(), &token).await;

    start_and_wait(sta.app(), &token, &server_id).await;

    // ── First stop ──
    let stop1 = spawn_stop(sta.arc(), token.clone(), server_id.clone());
    wait_for_status(sta.app(), &token, &server_id, "stopping", 5).await;

    // Cancel the first stop.
    let (cancel_status, _) = request_cancel_stop(sta.app(), &token, &server_id).await;
    assert_eq!(cancel_status, StatusCode::OK);

    // Wait for the background stop task to finish (it returns after cancellation).
    let _ = stop1.await;

    // Wait for the server to revert to Running after cancellation.
    wait_for_status(sta.app(), &token, &server_id, "running", 5).await;

    // ── Second stop — should NOT be falsely cancelled by the stale token ──
    let stop2 = spawn_stop(sta.arc(), token.clone(), server_id.clone());

    // Verify the server enters Stopping again.
    wait_for_status(sta.app(), &token, &server_id, "stopping", 5).await;

    // Give it a moment — if the stale token fired, the server would revert
    // to Running within this window.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check that the server is STILL stopping (not falsely cancelled).
    let (_, body) = sta
        .app()
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    let status_str = body["runtime"]["status"].as_str().unwrap_or("");
    assert!(
        status_str == "stopping" || status_str == "stopped",
        "Server should still be stopping or stopped (not running). Got: {}",
        status_str
    );

    // Cancel the second stop so we don't wait for the full sleep.
    if status_str == "stopping" {
        let _ = request_cancel_stop(sta.app(), &token, &server_id).await;
    }

    // Wait for stop2 to finish.
    let _ = stop2.await;

    // Final cleanup.
    kill_server(sta.app(), &token, &server_id).await;
}

/// After a completed stop (server is Stopped), starting a new instance
/// should work without any interference from the previous stop's token.
#[tokio::test]
async fn test_start_after_completed_stop_works() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;
    let server_id = create_fast_stop_server(&app, &token).await;

    // Start → Stop → wait for Stopped.
    start_and_wait(&app, &token, &server_id).await;

    // For fast-stop servers, the stop endpoint blocks and returns when done.
    let (stop_status, _) = app
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(stop_status, StatusCode::OK);
    wait_for_status(&app, &token, &server_id, "stopped", 15).await;

    // Start again — should succeed cleanly.
    start_and_wait(&app, &token, &server_id).await;

    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;
    assert_eq!(body["runtime"]["status"].as_str(), Some("running"));

    // Clean up.
    kill_server(&app, &token, &server_id).await;
}

/// Double-stop (calling stop while already stopping) should return a conflict.
#[tokio::test]
async fn test_double_stop_returns_conflict() {
    let sta = SharedTestApp::new().await;
    let token = sta.app().setup_admin("admin", TestApp::TEST_PASSWORD).await;
    let server_id = create_slow_stop_server(sta.app(), &token).await;

    start_and_wait(sta.app(), &token, &server_id).await;

    // First stop in background.
    let stop1 = spawn_stop(sta.arc(), token.clone(), server_id.clone());
    wait_for_status(sta.app(), &token, &server_id, "stopping", 5).await;

    // Second stop while the first is still in progress.
    let (status, body) = sta
        .app()
        .post(
            &format!("/api/servers/{}/stop", server_id),
            Some(&token),
            json!({}),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "double-stop should be 409: {:?}",
        body
    );

    // Cancel so we don't wait forever.
    let _ = request_cancel_stop(sta.app(), &token, &server_id).await;
    let _ = stop1.await;

    // Final cleanup.
    kill_server(sta.app(), &token, &server_id).await;
}

/// Cancelling a stop twice should return an error on the second cancel
/// (server is no longer in Stopping state after the first cancel).
#[tokio::test]
async fn test_double_cancel_stop_returns_conflict() {
    let sta = SharedTestApp::new().await;
    let token = sta.app().setup_admin("admin", TestApp::TEST_PASSWORD).await;
    let server_id = create_slow_stop_server(sta.app(), &token).await;

    start_and_wait(sta.app(), &token, &server_id).await;

    // Stop in background.
    let stop1 = spawn_stop(sta.arc(), token.clone(), server_id.clone());
    wait_for_status(sta.app(), &token, &server_id, "stopping", 5).await;

    // First cancel — should succeed.
    let (cancel1_status, _) = request_cancel_stop(sta.app(), &token, &server_id).await;
    assert_eq!(cancel1_status, StatusCode::OK);

    // Wait for stop1 to finish and server to revert.
    let _ = stop1.await;
    wait_for_status(sta.app(), &token, &server_id, "running", 5).await;

    // Second cancel — server is now Running, not Stopping.
    let (cancel2_status, body) = request_cancel_stop(sta.app(), &token, &server_id).await;
    assert_eq!(
        cancel2_status,
        StatusCode::CONFLICT,
        "second cancel-stop should be 409: {:?}",
        body
    );

    // Clean up.
    kill_server(sta.app(), &token, &server_id).await;
}
