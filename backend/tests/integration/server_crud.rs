use axum::http::StatusCode;
use serde_json::json;

use crate::common::{resolve_binary, TestApp};

#[tokio::test]
async fn test_create_server() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = resolve_binary("echo");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
            "config": {
                "name": "Test Server",
                "binary": echo,
                "args": ["hello", "world"],
                "env": { "FOO": "bar" },
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": true,
                    "max_restart_attempts": 3,
                    "restart_delay_secs": 5,
                    "stop_command": "stop",
                    "stop_timeout_secs": 10,
                    "sftp_username": "testsftp",
                    "sftp_password": "sftppass"
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["server"]["id"].is_string());
    assert_eq!(body["server"]["config"]["name"], "Test Server");
    assert!(body["server"]["config"]["binary"]
        .as_str()
        .unwrap()
        .contains("echo"));
    assert_eq!(body["server"]["config"]["auto_restart"], true);
    assert_eq!(body["server"]["config"]["max_restart_attempts"], 3);
    assert_eq!(body["server"]["config"]["stop_command"], "stop");
    assert_eq!(body["server"]["config"]["sftp_username"], "testsftp");
    assert_eq!(body["runtime"]["status"], "stopped");
}

#[tokio::test]
async fn test_create_server_missing_name() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "",
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
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_server_missing_binary() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Test",
                    "binary": "",
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
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_servers_shows_owned() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create two servers
    app.create_test_server(&token, "Server A").await;
    app.create_test_server(&token, "Server B").await;

    let (status, body) = app.get("/api/servers", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["servers"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_admin_sees_all_servers() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;

    // Grant CreateServers capability so the regular user can create servers
    let (status, _) = app
        .put(
            &format!("/api/admin/users/{}/capabilities", user_id),
            Some(&admin_token),
            json!({ "global_capabilities": ["create_servers"] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // User creates a server
    app.create_test_server(&user_token, "User Server").await;

    // Admin should see it
    let (_, body) = app.get("/api/servers", Some(&admin_token)).await;
    let servers = body["servers"].as_array().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["server"]["config"]["name"], "User Server");
}

#[tokio::test]
async fn test_regular_user_cannot_see_others_servers() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _) = app.setup_admin_and_user().await;

    // Admin creates a server
    app.create_test_server(&admin_token, "Admin Server").await;

    // User should NOT see it (no permission)
    let (_, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(body["servers"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_get_server() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "My Server").await;

    let (status, body) = app.get(&format!("/api/servers/{}", id), Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["server"]["config"]["name"], "My Server");
    assert_eq!(body["server"]["id"], id);
}

#[tokio::test]
async fn test_get_nonexistent_server() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let fake_id = "00000000-0000-0000-0000-000000000000";
    let (status, _) = app
        .get(&format!("/api/servers/{}", fake_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_server() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "Old Name").await;

    let cat = resolve_binary("cat");
    let (status, body) = app
        .put(
            &format!("/api/servers/{}", id),
            Some(&token),
            json!({
                "config": {
                    "name": "New Name",
                    "binary": cat,
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": true,
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
    assert_eq!(body["server"]["config"]["name"], "New Name");
    assert!(body["server"]["config"]["binary"]
        .as_str()
        .unwrap()
        .contains("cat"));
    assert_eq!(body["server"]["config"]["auto_start"], true);
}

#[tokio::test]
async fn test_delete_server() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (id, _) = app.create_test_server(&token, "Doomed").await;

    let (status, body) = app
        .delete(&format!("/api/servers/{}", id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted"], true);

    // Verify gone
    let (status, _) = app.get(&format!("/api/servers/{}", id), Some(&token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_viewer_cannot_edit_server() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;

    let (server_id, _) = app.create_test_server(&admin_token, "Admin Server").await;

    // Grant viewer permission
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // User tries to update — should fail (needs Manager)
    let (status, _) = app
        .put(
            &format!("/api/servers/{}", server_id),
            Some(&user_token),
            json!({
                "config": {
                    "name": "Hacked Name",
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
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_viewer_cannot_delete_server() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;

    let (server_id, _) = app.create_test_server(&admin_token, "Admin Server").await;

    // Grant viewer permission
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    let (status, _) = app
        .delete(&format!("/api/servers/{}", server_id), Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_operator_cannot_delete_server() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Admin Server").await;

    // Grant operator permission
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "operator" }),
    )
    .await;

    let (status, _) = app
        .delete(&format!("/api/servers/{}", server_id), Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_manager_cannot_delete_server() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Admin Server").await;

    // Grant manager permission
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "manager" }),
    )
    .await;

    let (status, _) = app
        .delete(&format!("/api/servers/{}", server_id), Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_server_admin_can_delete_server() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Admin Server").await;

    // Grant admin-level permission on the server
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "admin" }),
    )
    .await;

    let (status, body) = app
        .delete(&format!("/api/servers/{}", server_id), Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted"], true);
}

#[tokio::test]
async fn test_unauthenticated_cannot_access_servers() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    app.create_test_server(&token, "Secret").await;

    let (status, _) = app.get("/api/servers", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_unauthenticated_cannot_create_server() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app
        .post(
            "/api/servers",
            None,
            json!({
                "config": {
                    "name": "X",
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
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
