use axum::http::StatusCode;
use serde_json::json;

use crate::common::TestApp;

/// Full integration scenario:
/// 1. Admin creates servers A, B
/// 2. User creates server C
/// 3. Admin sees A, B, C. User sees only C.
/// 4. Admin grants viewer on A to user → user sees A, C
/// 5. Admin grants manager on B to user → user sees A, B, C
/// 6. User can read files on A (viewer), write files on B (manager)
/// 7. User CANNOT write files on A (viewer-only)
/// 8. Admin revokes A → user sees only B, C
#[tokio::test]
async fn test_full_visibility_and_access_scenario() {
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

    // Step 1 & 2: Create servers
    let (server_a, _) = app.create_test_server(&admin_token, "Server A").await;
    let (server_b, _) = app.create_test_server(&admin_token, "Server B").await;
    let (_server_c, _) = app.create_test_server(&user_token, "Server C").await;

    // Step 3: Check visibility
    let (_, admin_list) = app.get("/api/servers", Some(&admin_token)).await;
    assert_eq!(admin_list["servers"].as_array().unwrap().len(), 3);

    let (_, user_list) = app.get("/api/servers", Some(&user_token)).await;
    let user_servers = user_list["servers"].as_array().unwrap();
    assert_eq!(user_servers.len(), 1);
    assert_eq!(user_servers[0]["server"]["config"]["name"], "Server C");

    // Step 4: Grant viewer on A
    app.post(
        &format!("/api/servers/{}/permissions", server_a),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    let (_, user_list) = app.get("/api/servers", Some(&user_token)).await;
    let user_servers = user_list["servers"].as_array().unwrap();
    assert_eq!(user_servers.len(), 2);
    let names: Vec<&str> = user_servers
        .iter()
        .map(|s| s["server"]["config"]["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Server A"));
    assert!(names.contains(&"Server C"));

    // Step 5: Grant manager on B
    app.post(
        &format!("/api/servers/{}/permissions", server_b),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "manager" }),
    )
    .await;

    let (_, user_list) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(user_list["servers"].as_array().unwrap().len(), 3);

    // Step 6: User can read files on A (viewer)
    // First, admin writes a file to server A
    app.post(
        &format!("/api/servers/{}/files/write", server_a),
        Some(&admin_token),
        json!({ "path": "info.txt", "content": "server A data" }),
    )
    .await;

    let (status, body) = app
        .get(
            &format!("/api/servers/{}/files/read?path=info.txt", server_a),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], "server A data");

    // User can write files on B (manager)
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_b),
            Some(&user_token),
            json!({ "path": "user_file.txt", "content": "from user" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Step 7: User CANNOT write files on A (viewer-only)
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_a),
            Some(&user_token),
            json!({ "path": "hacked.txt", "content": "evil" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Step 8: Admin revokes A
    app.post(
        &format!("/api/servers/{}/permissions/remove", server_a),
        Some(&admin_token),
        json!({ "user_id": user_id }),
    )
    .await;

    let (_, user_list) = app.get("/api/servers", Some(&user_token)).await;
    let user_servers = user_list["servers"].as_array().unwrap();
    assert_eq!(user_servers.len(), 2);
    let names: Vec<&str> = user_servers
        .iter()
        .map(|s| s["server"]["config"]["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Server B"));
    assert!(names.contains(&"Server C"));
    assert!(!names.contains(&"Server A"));
}

/// Verify that deleting a user also removes their server permissions.
#[tokio::test]
async fn test_deleting_user_cleans_up_permissions() {
    let app = TestApp::new().await;
    let (admin_token, _, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Cleanup").await;

    // Grant user viewer permission
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // Verify it exists in the permissions list
    let (_, body) = app
        .get(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
        )
        .await;
    let perms = body["permissions"].as_array().unwrap();
    assert!(
        perms.iter().any(|p| p["user"]["username"] == "regularuser"),
        "regularuser should be in permissions"
    );

    // Delete the user
    app.delete(&format!("/api/admin/users/{}", user_id), Some(&admin_token))
        .await;

    // Verify the permission is cleaned up
    let (_, body) = app
        .get(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
        )
        .await;
    let perms = body["permissions"].as_array().unwrap();
    assert!(
        !perms.iter().any(|p| p["user"]["username"] == "regularuser"),
        "regularuser should be removed from permissions after deletion"
    );
}

/// Verify that deleting a server also removes all associated permissions.
#[tokio::test]
async fn test_deleting_server_cleans_up_permissions() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Doomed Server").await;

    // Grant user viewer permission
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // User can see it
    let (_, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(body["servers"].as_array().unwrap().len(), 1);

    // Delete the server
    app.delete(&format!("/api/servers/{}", server_id), Some(&admin_token))
        .await;

    // User sees no servers (the permission should be gone)
    let (_, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(body["servers"].as_array().unwrap().len(), 0);
}

/// Verify the effective permission reported in the server response matches
/// the actual grant level for different user types.
#[tokio::test]
async fn test_effective_permission_in_server_response() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Perm Check").await;

    // Admin sees the server with owner-level effective permission (global admin)
    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&admin_token))
        .await;
    assert_eq!(body["permission"]["level"], "owner");
    assert_eq!(body["permission"]["is_global_admin"], true);

    // Grant operator to user
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "operator" }),
    )
    .await;

    // User sees operator-level effective permission
    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&user_token))
        .await;
    assert_eq!(body["permission"]["level"], "operator");
    assert_eq!(body["permission"]["is_global_admin"], false);
}

/// Verify that the server owner (non-admin) has owner-level permission.
#[tokio::test]
async fn test_owner_has_owner_level_permission() {
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

    // User creates a server — they are the owner
    let (server_id, _) = app.create_test_server(&user_token, "My Server").await;

    let (_, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&user_token))
        .await;
    assert_eq!(body["permission"]["level"], "owner");
    assert_eq!(body["permission"]["is_global_admin"], false);
}
