use axum::http::StatusCode;
use serde_json::json;

use crate::common::{resolve_binary, TestApp};

#[tokio::test]
async fn test_setup_validates_username_format() {
    let app = TestApp::new().await;

    // Too short
    let (status, _) = app
        .post(
            "/api/auth/setup",
            None,
            json!({ "username": "ab", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Special characters
    let (status, _) = app
        .post(
            "/api/auth/setup",
            None,
            json!({ "username": "user@admin!", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_setup_validates_password_length() {
    let app = TestApp::new().await;

    let (status, _) = app
        .post(
            "/api/auth/setup",
            None,
            json!({ "username": "admin", "password": "short" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_username_case_insensitive() {
    let app = TestApp::new().await;
    app.setup_admin("Admin", "Admin1234").await;

    // Login with lowercase should work (username is normalized)
    let (status, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Login with uppercase should also work
    let (status, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "ADMIN", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_deleted_user_token_no_longer_valid_for_protected_routes() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;

    // User can access their info before deletion
    let (status, _) = app.get("/api/auth/me", Some(&user_token)).await;
    assert_eq!(status, StatusCode::OK);

    // Delete the user
    app.delete(&format!("/api/admin/users/{}", user_id), Some(&admin_token))
        .await;

    // The JWT is still valid cryptographically, but the user no longer exists.
    // /auth/me should reject since the user is gone.
    let (status, _) = app.get("/api/auth/me", Some(&user_token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_multiple_servers_independent_files() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (id_a, _) = app.create_test_server(&token, "Server A").await;
    let (id_b, _) = app.create_test_server(&token, "Server B").await;

    // Write to server A
    app.post(
        &format!("/api/servers/{}/files/write", id_a),
        Some(&token),
        json!({ "path": "data.txt", "content": "A data" }),
    )
    .await;

    // Write to server B
    app.post(
        &format!("/api/servers/{}/files/write", id_b),
        Some(&token),
        json!({ "path": "data.txt", "content": "B data" }),
    )
    .await;

    // Read from server A
    let (_, body) = app
        .get(
            &format!("/api/servers/{}/files/read?path=data.txt", id_a),
            Some(&token),
        )
        .await;
    assert_eq!(body["content"], "A data");

    // Read from server B
    let (_, body) = app
        .get(
            &format!("/api/servers/{}/files/read?path=data.txt", id_b),
            Some(&token),
        )
        .await;
    assert_eq!(body["content"], "B data");
}

#[tokio::test]
async fn test_user_without_permission_cannot_access_server_files() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Secret Server").await;

    // Write a file as admin
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&admin_token),
        json!({ "path": "secret.txt", "content": "classified" }),
    )
    .await;

    // User (no permission) tries to list files
    let (status, _) = app
        .get(
            &format!("/api/servers/{}/files", server_id),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // User tries to read the file
    let (status, _) = app
        .get(
            &format!("/api/servers/{}/files/read?path=secret.txt", server_id),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// Verify server creation returns runtime with stopped status and correct permission.
#[tokio::test]
async fn test_server_creation_returns_complete_response() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (_, body) = app.create_test_server(&token, "Complete").await;

    // Check runtime fields
    assert_eq!(body["runtime"]["status"], "stopped");
    assert!(body["runtime"]["pid"].is_null());
    assert!(body["runtime"]["started_at"].is_null());
    assert_eq!(body["runtime"]["restart_count"], 0);

    // Check permission fields (creator is owner)
    assert_eq!(body["permission"]["level"], "owner");

    // Check server metadata
    assert!(body["server"]["id"].is_string());
    assert!(body["server"]["created_at"].is_string());
    assert!(body["server"]["updated_at"].is_string());
    assert!(body["server"]["owner_id"].is_string());
}

/// Verify that the updated_at timestamp changes after an update.
#[tokio::test]
async fn test_updated_at_changes_on_update() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, create_body) = app.create_test_server(&token, "Timestamp Test").await;

    let created_at = create_body["server"]["created_at"]
        .as_str()
        .unwrap()
        .to_string();
    let original_updated = create_body["server"]["updated_at"]
        .as_str()
        .unwrap()
        .to_string();

    // Small delay
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Update
    let (_, update_body) = app
        .put(
            &format!("/api/servers/{}", server_id),
            Some(&token),
            json!({
                "config": {
                    "name": "Updated Name",
                    "binary": resolve_binary("echo"),
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

    // created_at should NOT change
    assert_eq!(
        update_body["server"]["created_at"].as_str().unwrap(),
        created_at
    );
    // updated_at SHOULD change
    assert_ne!(
        update_body["server"]["updated_at"].as_str().unwrap(),
        original_updated
    );
}

#[tokio::test]
async fn test_no_permission_user_gets_empty_server_list() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _) = app.setup_admin_and_user().await;

    // Admin creates many servers — user has no permissions on any
    app.create_test_server(&admin_token, "S1").await;
    app.create_test_server(&admin_token, "S2").await;
    app.create_test_server(&admin_token, "S3").await;

    let (status, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["servers"].as_array().unwrap().len(), 0);
}

/// Stress-test: Create many servers and verify listing works correctly.
#[tokio::test]
async fn test_many_servers_list() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    for i in 0..20 {
        app.create_test_server(&token, &format!("Server {}", i))
            .await;
    }

    let (status, body) = app.get("/api/servers", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["servers"].as_array().unwrap().len(), 20);
}

/// Overwriting a file replaces its content completely.
#[tokio::test]
async fn test_overwrite_file() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Overwrite Test").await;

    // Write original
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&token),
        json!({ "path": "config.yml", "content": "version: 1\nname: old" }),
    )
    .await;

    // Overwrite
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&token),
        json!({ "path": "config.yml", "content": "version: 2\nname: new" }),
    )
    .await;

    // Read back — should be the new content
    let (_, body) = app
        .get(
            &format!("/api/servers/{}/files/read?path=config.yml", server_id),
            Some(&token),
        )
        .await;
    assert_eq!(body["content"], "version: 2\nname: new");
}

#[tokio::test]
async fn test_delete_nonexistent_path_returns_not_found() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Del Test").await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/delete", server_id),
            Some(&token),
            json!({ "path": "ghost.txt" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
