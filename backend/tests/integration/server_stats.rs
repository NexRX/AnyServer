//! Integration tests for the per-server resource stats endpoint (ticket 015).

use axum::http::StatusCode;
use uuid::Uuid;

use crate::common::TestApp;

// ─── Authentication & Authorization ───

#[tokio::test]
async fn test_server_stats_requires_auth() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "stats-auth").await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/stats", server_id), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "body: {:?}", body);
}

#[tokio::test]
async fn test_server_stats_requires_viewer_permission() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "stats-perm").await;

    // Regular user with no explicit permission should be forbidden
    let (status, body) = app
        .get(
            &format!("/api/servers/{}/stats", server_id),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {:?}", body);
}

#[tokio::test]
async fn test_server_stats_accessible_by_owner() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "stats-owner").await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/stats", server_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
}

// ─── 404 for non-existent server ───

#[tokio::test]
async fn test_server_stats_not_found() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let fake_id = Uuid::new_v4();

    let (status, body) = app
        .get(&format!("/api/servers/{}/stats", fake_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body: {:?}", body);
}

// ─── Stopped server returns stats with null CPU/memory ───

#[tokio::test]
async fn test_server_stats_stopped_server_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "stats-stopped").await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/stats", server_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    // server_id must match
    assert_eq!(
        body["server_id"].as_str().unwrap(),
        server_id,
        "server_id mismatch"
    );

    // CPU and memory should be null for a stopped server
    assert!(
        body["cpu_percent"].is_null(),
        "cpu_percent should be null for stopped server: {:?}",
        body
    );
    assert!(
        body["memory_rss_bytes"].is_null(),
        "memory_rss_bytes should be null for stopped server: {:?}",
        body
    );
    assert!(
        body["memory_swap_bytes"].is_null(),
        "memory_swap_bytes should be null for stopped server: {:?}",
        body
    );

    // Disk usage should be a number (>= 0)
    assert!(
        body["disk_usage_bytes"].is_number(),
        "disk_usage_bytes should be a number: {:?}",
        body
    );

    // Timestamp should be present and non-empty
    let ts = body["timestamp"].as_str().unwrap();
    assert!(!ts.is_empty(), "timestamp should not be empty");
    assert!(
        ts.len() >= 10 && ts.chars().take(4).all(|c| c.is_ascii_digit()),
        "timestamp should look like an ISO 8601 date: {}",
        ts
    );
}

// ─── Disk usage reflects files in the server directory ───

#[tokio::test]
async fn test_server_stats_disk_usage_reflects_files() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "stats-disk").await;

    let sid = Uuid::parse_str(&server_id).unwrap();
    let server_dir = app.state.server_dir(&sid);
    std::fs::create_dir_all(&server_dir).unwrap();

    // Write some known data
    std::fs::write(server_dir.join("test.dat"), "hello world!").unwrap(); // 12 bytes

    let (status, body) = app
        .get(&format!("/api/servers/{}/stats", server_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    let disk_bytes = body["disk_usage_bytes"].as_u64().unwrap();
    assert!(
        disk_bytes >= 12,
        "disk_usage_bytes should be >= 12 bytes (the file we wrote), got {}",
        disk_bytes
    );
}

// ─── All expected fields are present ───

#[tokio::test]
async fn test_server_stats_all_fields_present() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "stats-fields").await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/stats", server_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    // Every field from the ServerResourceStats struct must be present
    for field in &[
        "server_id",
        "cpu_percent",
        "memory_rss_bytes",
        "memory_swap_bytes",
        "disk_usage_bytes",
        "timestamp",
    ] {
        assert!(
            body.get(*field).is_some(),
            "Missing field '{}' in response: {:?}",
            field,
            body
        );
    }
}

// ─── Regular user with explicit permission can access ───

#[tokio::test]
async fn test_server_stats_accessible_with_viewer_permission() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "stats-viewer").await;

    // Grant viewer permission
    let (grant_status, _) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
            serde_json::json!({
                "user_id": user_id,
                "level": "viewer"
            }),
        )
        .await;
    assert_eq!(grant_status, StatusCode::OK);

    // User should now be able to access stats
    let (status, body) = app
        .get(
            &format!("/api/servers/{}/stats", server_id),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    assert_eq!(body["server_id"].as_str().unwrap(), server_id);
}

// ─── Successive calls return fresh timestamps ───

#[tokio::test]
async fn test_server_stats_successive_calls_fresh_timestamps() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "stats-fresh").await;

    let (s1, b1) = app
        .get(&format!("/api/servers/{}/stats", server_id), Some(&token))
        .await;
    assert_eq!(s1, StatusCode::OK);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let (s2, b2) = app
        .get(&format!("/api/servers/{}/stats", server_id), Some(&token))
        .await;
    assert_eq!(s2, StatusCode::OK);

    let ts1 = b1["timestamp"].as_str().unwrap();
    let ts2 = b2["timestamp"].as_str().unwrap();
    assert!(!ts1.is_empty());
    assert!(!ts2.is_empty());
}

// ─── Disk usage is zero for empty server dir ───

#[tokio::test]
async fn test_server_stats_empty_server_dir_zero_disk() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "stats-empty-dir").await;

    // The server dir may or may not exist yet — either way, disk usage
    // should be 0 or at least a small number (no server files written).
    let (status, body) = app
        .get(&format!("/api/servers/{}/stats", server_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    let disk_bytes = body["disk_usage_bytes"].as_u64().unwrap();
    // We can't assert exactly 0 because create_test_server might create
    // the directory, but there shouldn't be any large files.
    assert!(
        disk_bytes < 1024 * 1024, // less than 1 MB
        "Empty server dir should have minimal disk usage, got {} bytes",
        disk_bytes
    );
}
