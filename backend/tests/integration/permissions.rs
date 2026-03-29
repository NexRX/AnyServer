use axum::http::StatusCode;
use serde_json::json;

use crate::common::TestApp;

#[tokio::test]
async fn test_grant_permission_to_user() {
    let app = TestApp::new().await;
    let (admin_token, _, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Test").await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
            json!({ "user_id": user_id, "level": "operator" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["level"], "operator");
    assert_eq!(body["user"]["username"], "regularuser");
}

#[tokio::test]
async fn test_list_permissions() {
    let app = TestApp::new().await;
    let (admin_token, _, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Test").await;

    // Grant viewer
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    let (status, body) = app
        .get(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let perms = body["permissions"].as_array().unwrap();
    // Should have at least the owner + the viewer we granted
    assert!(perms.len() >= 2, "expected >=2, got {:?}", perms);

    // The owner should be present with owner level
    let owner = perms.iter().find(|p| p["level"] == "owner");
    assert!(owner.is_some(), "owner not found in permissions list");

    // Our granted user should be present with viewer level
    let viewer = perms
        .iter()
        .find(|p| p["user"]["username"] == "regularuser");
    assert!(viewer.is_some());
    assert_eq!(viewer.unwrap()["level"], "viewer");
}

#[tokio::test]
async fn test_remove_permission() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Test").await;

    // Grant
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // User can see the server now
    let (status, _) = app
        .get(&format!("/api/servers/{}", server_id), Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::OK);

    // Remove
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions/remove", server_id),
            Some(&admin_token),
            json!({ "user_id": user_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed"], true);

    // User can NO LONGER see the server
    let (status, _) = app
        .get(&format!("/api/servers/{}", server_id), Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_cannot_change_owner_permission() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Test").await;

    // Get admin's user ID (the owner)
    let (_, me) = app.get("/api/auth/me", Some(&admin_token)).await;
    let admin_id = me["user"]["id"].as_str().unwrap();

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
            json!({ "user_id": admin_id, "level": "viewer" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("owner"));
}

#[tokio::test]
async fn test_cannot_change_own_permission() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Test").await;

    // Grant user admin-level on this server
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "admin" }),
    )
    .await;

    // User tries to change their own permission
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&user_token),
            json!({ "user_id": user_id, "level": "owner" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("own"));
}

#[tokio::test]
async fn test_server_admin_cannot_grant_owner_level() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Test").await;

    // Enable reg and create a third user
    let third_token = app.register_user("thirduser", "Admin1234").await;
    let (_, third_me) = app.get("/api/auth/me", Some(&third_token)).await;
    let third_id = third_me["user"]["id"].as_str().unwrap();

    // Grant user admin-level on this server (non-global-admin)
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "admin" }),
    )
    .await;

    // Server-level admin tries to grant owner level — should fail
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&user_token),
            json!({ "user_id": third_id, "level": "owner" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body["error"].as_str().unwrap().contains("global admin"));
}

#[tokio::test]
async fn test_global_admin_can_grant_owner_level() {
    let app = TestApp::new().await;
    let (admin_token, _, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Test").await;

    // Global admin grants owner level — should succeed
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
            json!({ "user_id": user_id, "level": "owner" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["level"], "owner");
}

#[tokio::test]
async fn test_viewer_cannot_manage_permissions() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Test").await;

    // Grant viewer only
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // Viewer tries to list permissions — should fail (needs Admin)
    let (status, _) = app
        .get(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_permission_grants_access() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Private Server").await;

    // Before grant — user can't see the server
    let (_, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(body["servers"].as_array().unwrap().len(), 0);

    // Grant viewer
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // After grant — user CAN see the server
    let (_, body) = app.get("/api/servers", Some(&user_token)).await;
    let servers = body["servers"].as_array().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["server"]["config"]["name"], "Private Server");
}

#[tokio::test]
async fn test_remove_permission_removes_access() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Private").await;

    // Grant
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // User can see it
    let (status, _) = app
        .get(&format!("/api/servers/{}", server_id), Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::OK);

    // Remove
    app.post(
        &format!("/api/servers/{}/permissions/remove", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id }),
    )
    .await;

    // User can NOT see it anymore
    let (status, _) = app
        .get(&format!("/api/servers/{}", server_id), Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Also gone from the list
    let (_, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(body["servers"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_cannot_remove_owner_permission() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Test").await;

    let (_, me) = app.get("/api/auth/me", Some(&admin_token)).await;
    let admin_id = me["user"]["id"].as_str().unwrap();

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions/remove", server_id),
            Some(&admin_token),
            json!({ "user_id": admin_id }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("owner"));
}

#[tokio::test]
async fn test_cannot_remove_own_permission() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Test").await;

    // Grant admin level
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "admin" }),
    )
    .await;

    // User tries to remove their own permission
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions/remove", server_id),
            Some(&user_token),
            json!({ "user_id": user_id }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("own"));
}

#[tokio::test]
async fn test_server_admin_cannot_remove_higher_permission() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Test").await;

    // Create a third user and give them owner level via global admin
    let third_token = app.register_user("thirduser", "Admin1234").await;
    let (_, third_me) = app.get("/api/auth/me", Some(&third_token)).await;
    let third_id = third_me["user"]["id"].as_str().unwrap();

    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": third_id, "level": "owner" }),
    )
    .await;

    // Give user admin-level on the server
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "admin" }),
    )
    .await;

    // Server admin tries to remove the owner-level user — should fail
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/permissions/remove", server_id),
            Some(&user_token),
            json!({ "user_id": third_id }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_permission_upgrade() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Upgrade Test").await;

    // Grant viewer
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // Viewer tries to write a file — should fail
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(&user_token),
            json!({ "path": "test.txt", "content": "hi" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Upgrade to manager
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "manager" }),
    )
    .await;

    // Manager CAN write a file
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(&user_token),
            json!({ "path": "test.txt", "content": "hi" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}
