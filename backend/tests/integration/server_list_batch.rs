//! Tests for the batch-permission enrichment in `GET /api/servers`.
//!
//! These verify that the N+1 query fix works correctly: permissions are
//! pre-fetched in a single batch rather than one query per server.

use axum::http::StatusCode;
use serde_json::json;

use crate::common::{resolve_binary, TestApp};

/// Admin user sees all servers with Owner permission and `is_global_admin: true`.
#[tokio::test]
async fn test_admin_sees_all_servers_with_owner_permission() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create several servers
    for i in 0..5 {
        app.create_test_server(&admin_token, &format!("Server {}", i))
            .await;
    }

    let (status, body) = app.get("/api/servers", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);

    let servers = body["servers"].as_array().unwrap();
    assert_eq!(servers.len(), 5);

    for s in servers {
        assert_eq!(
            s["permission"]["level"], "owner",
            "Admin should have owner-level on every server"
        );
        assert_eq!(
            s["permission"]["is_global_admin"], true,
            "Admin should be
 flagged as global admin"
        );
    }
}

/// Non-admin user with no permissions sees an empty server list (not an error).
#[tokio::test]
async fn test_non_admin_no_permissions_gets_empty_list() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _user_id) = app.setup_admin_and_user().await;

    // Admin creates servers but grants no permissions to the user
    for i in 0..3 {
        app.create_test_server(&admin_token, &format!("Private {}", i))
            .await;
    }

    let (status, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(status, StatusCode::OK);

    let servers = body["servers"].as_array().unwrap();
    assert_eq!(
        servers.len(),
        0,
        "User without permissions should see zero servers"
    );
}

/// Server owner sees their server with Owner permission and `is_global_admin: false`.
#[tokio::test]
async fn test_owner_sees_own_server_without_explicit_permission_row() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;

    // Grant create_servers capability so the user can create their own server
    let (cap_status, cap_body) = app
        .put(
            &format!("/api/admin/users/{}/capabilities", user_id),
            Some(&admin_token),
            json!({ "global_capabilities": ["create_servers"] }),
        )
        .await;
    assert_eq!(
        cap_status,
        StatusCode::OK,
        "granting capability failed: {:?}",
        cap_body
    );

    // The regular user creates their own server
    let echo = resolve_binary("echo");
    let (status, _body) = app
        .post(
            "/api/servers",
            Some(&user_token),
            json!({
                "config": {
                    "name": "My Own Server",
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
    assert_eq!(status, StatusCode::OK, "server creation failed");

    // Also create an admin-owned server that the user has no access to
    app.create_test_server(&admin_token, "Admin Private Server")
        .await;

    let (status, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(status, StatusCode::OK);

    let servers = body["servers"].as_array().unwrap();
    assert_eq!(
        servers.len(),
        1,
        "Owner should see exactly their own server"
    );
    assert_eq!(servers[0]["server"]["config"]["name"], "My Own Server");
    assert_eq!(servers[0]["permission"]["level"], "owner");
    assert_eq!(
        servers[0]["permission"]["is_global_admin"], false,
        "Non-admin owner should not be flagged as global admin"
    );
}

/// Non-admin user with explicit permissions on several servers at different
/// levels sees the correct list with correct permission levels.
#[tokio::test]
async fn test_non_admin_with_mixed_permissions() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;

    // Create servers with different permission grants
    let (id_viewer, _) = app.create_test_server(&admin_token, "Viewer Server").await;
    let (id_operator, _) = app
        .create_test_server(&admin_token, "Operator Server")
        .await;
    let (id_manager, _) = app.create_test_server(&admin_token, "Manager Server").await;
    let (id_admin, _) = app.create_test_server(&admin_token, "Admin Server").await;
    let (_id_noaccess, _) = app
        .create_test_server(&admin_token, "No Access Server")
        .await;

    // Grant various permission levels
    app.post(
        &format!("/api/servers/{}/permissions", id_viewer),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;
    app.post(
        &format!("/api/servers/{}/permissions", id_operator),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "operator" }),
    )
    .await;
    app.post(
        &format!("/api/servers/{}/permissions", id_manager),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "manager" }),
    )
    .await;
    app.post(
        &format!("/api/servers/{}/permissions", id_admin),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "admin" }),
    )
    .await;

    let (status, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(status, StatusCode::OK);

    let servers = body["servers"].as_array().unwrap();
    assert_eq!(
        servers.len(),
        4,
        "User should see 4 servers (not the one with no access)"
    );

    // Build a map of server name → permission level
    let perm_map: std::collections::HashMap<String, String> = servers
        .iter()
        .map(|s| {
            (
                s["server"]["config"]["name"].as_str().unwrap().to_string(),
                s["permission"]["level"].as_str().unwrap().to_string(),
            )
        })
        .collect();

    assert_eq!(perm_map.get("Viewer Server").unwrap(), "viewer");
    assert_eq!(perm_map.get("Operator Server").unwrap(), "operator");
    assert_eq!(perm_map.get("Manager Server").unwrap(), "manager");
    assert_eq!(perm_map.get("Admin Server").unwrap(), "admin");
    assert!(
        !perm_map.contains_key("No Access Server"),
        "Server without permission should not appear"
    );

    // All should have is_global_admin: false
    for s in servers {
        assert_eq!(s["permission"]["is_global_admin"], false);
    }
}

/// Paginated variant also uses batch permissions correctly.
#[tokio::test]
async fn test_paginated_list_uses_batch_permissions() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;

    // Create 5 servers, grant permission on 3
    let mut granted_ids = Vec::new();
    for i in 0..5 {
        let (id, _) = app
            .create_test_server(&admin_token, &format!("PagServer {}", i))
            .await;
        if i < 3 {
            app.post(
                &format!("/api/servers/{}/permissions", id),
                Some(&admin_token),
                json!({ "user_id": user_id, "level": "viewer" }),
            )
            .await;
            granted_ids.push(id);
        }
    }

    // Request page 1 with per_page=2
    let (status, body) = app
        .get("/api/servers?page=1&per_page=2", Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::OK);

    let servers = body["servers"].as_array().unwrap();
    // Paginated result should have at most 2 entries
    assert!(
        servers.len() <= 2,
        "Page should have at most 2 servers, got {}",
        servers.len()
    );

    // All returned servers should have correct permissions
    for s in servers {
        assert_eq!(s["permission"]["level"], "viewer");
        assert_eq!(s["permission"]["is_global_admin"], false);
    }
}

/// Verify that `list_permissions_for_user_batch` returns correct HashMap
/// when the user has permissions on multiple servers at different levels.
#[tokio::test]
async fn test_list_permissions_for_user_batch_returns_correct_map() {
    let app = TestApp::new().await;
    let (admin_token, _user_token, user_id) = app.setup_admin_and_user().await;

    let (id1, _) = app.create_test_server(&admin_token, "Batch A").await;
    let (id2, _) = app.create_test_server(&admin_token, "Batch B").await;
    let (id3, _) = app.create_test_server(&admin_token, "Batch C").await;

    // Grant different levels
    app.post(
        &format!("/api/servers/{}/permissions", id1),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;
    app.post(
        &format!("/api/servers/{}/permissions", id2),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "operator" }),
    )
    .await;
    app.post(
        &format!("/api/servers/{}/permissions", id3),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "manager" }),
    )
    .await;

    // Call the batch method directly
    let uid = uuid::Uuid::parse_str(&user_id).unwrap();
    let map = app
        .state
        .db
        .list_permissions_for_user_batch(&uid)
        .await
        .unwrap();

    assert_eq!(map.len(), 3);

    use anyserver::types::PermissionLevel;

    let sid1 = uuid::Uuid::parse_str(&id1).unwrap();
    let sid2 = uuid::Uuid::parse_str(&id2).unwrap();
    let sid3 = uuid::Uuid::parse_str(&id3).unwrap();

    assert_eq!(map.get(&sid1), Some(&PermissionLevel::Viewer));
    assert_eq!(map.get(&sid2), Some(&PermissionLevel::Operator));
    assert_eq!(map.get(&sid3), Some(&PermissionLevel::Manager));
}

/// `list_permissions_for_user_batch` returns an empty map for a user with no permissions.
#[tokio::test]
async fn test_list_permissions_for_user_batch_empty_for_no_perms() {
    let app = TestApp::new().await;
    let (_admin_token, _user_token, user_id) = app.setup_admin_and_user().await;

    let uid = uuid::Uuid::parse_str(&user_id).unwrap();
    let map = app
        .state
        .db
        .list_permissions_for_user_batch(&uid)
        .await
        .unwrap();

    assert!(
        map.is_empty(),
        "User with no permissions should have empty map"
    );
}

/// Granting and then revoking a permission is reflected in the server list.
#[tokio::test]
async fn test_revoked_permission_removes_server_from_list() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;

    let (server_id, _) = app.create_test_server(&admin_token, "Revoke Test").await;

    // Grant viewer
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // User can see it
    let (status, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["servers"].as_array().unwrap().len(), 1);

    // Revoke
    app.post(
        &format!("/api/servers/{}/permissions/remove", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id }),
    )
    .await;

    // User can no longer see it
    let (status, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["servers"].as_array().unwrap().len(),
        0,
        "Revoked permission should hide the server"
    );
}

/// Upgrading a permission level is reflected correctly in the server list.
#[tokio::test]
async fn test_permission_upgrade_reflected_in_list() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;

    let (server_id, _) = app
        .create_test_server(&admin_token, "Upgrade List Test")
        .await;

    // Grant viewer
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    let (_, body) = app.get("/api/servers", Some(&user_token)).await;
    let servers = body["servers"].as_array().unwrap();
    assert_eq!(servers[0]["permission"]["level"], "viewer");

    // Upgrade to manager
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "manager" }),
    )
    .await;

    let (_, body) = app.get("/api/servers", Some(&user_token)).await;
    let servers = body["servers"].as_array().unwrap();
    assert_eq!(servers[0]["permission"]["level"], "manager");
}

/// Admin sees all servers even when some have permission rows for other users.
#[tokio::test]
async fn test_admin_sees_all_servers_regardless_of_permission_rows() {
    let app = TestApp::new().await;
    let (admin_token, _user_token, user_id) = app.setup_admin_and_user().await;

    // Create servers — some with permission rows for the regular user, some without
    let (id1, _) = app.create_test_server(&admin_token, "With Perm").await;
    let (_id2, _) = app.create_test_server(&admin_token, "Without Perm").await;

    app.post(
        &format!("/api/servers/{}/permissions", id1),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    let (status, body) = app.get("/api/servers", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);

    let servers = body["servers"].as_array().unwrap();
    assert_eq!(
        servers.len(),
        2,
        "Admin should see all servers regardless of permission rows"
    );

    for s in servers {
        assert_eq!(s["permission"]["level"], "owner");
        assert_eq!(s["permission"]["is_global_admin"], true);
    }
}

/// SFTP password is never leaked in the server list response.
#[tokio::test]
async fn test_sftp_password_not_leaked_in_list() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let echo = resolve_binary("echo");
    let (status, _) = app
        .post(
            "/api/servers",
            Some(&admin_token),
            json!({
                "config": {
                    "name": "SFTP Test",
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
                    "sftp_username": "testsftp",
                    "sftp_password": "secret123"
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = app.get("/api/servers", Some(&admin_token)).await;
    let servers = body["servers"].as_array().unwrap();
    assert_eq!(servers.len(), 1);
    assert!(
        servers[0]["server"]["config"]["sftp_password"].is_null(),
        "sftp_password should be null in list response"
    );
}

/// Mixed scenario: user owns some servers AND has explicit permissions on others.
#[tokio::test]
async fn test_owner_plus_explicit_permissions_combined() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;

    // Grant create_servers capability so the user can create their own
    let (cap_status, cap_body) = app
        .put(
            &format!("/api/admin/users/{}/capabilities", user_id),
            Some(&admin_token),
            json!({ "global_capabilities": ["create_servers"] }),
        )
        .await;
    assert_eq!(
        cap_status,
        StatusCode::OK,
        "granting capability failed: {:?}",
        cap_body
    );

    // User creates their own server
    let echo = resolve_binary("echo");
    let (status, own_body) = app
        .post(
            "/api/servers",
            Some(&user_token),
            json!({
                "config": {
                    "name": "User Owned",
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
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "server creation failed: {:?}",
        own_body
    );
    let _own_id = own_body["server"]["id"].as_str().unwrap().to_string();

    // Admin creates a server and grants viewer to user
    let (admin_server_id, _) = app.create_test_server(&admin_token, "Admin Shared").await;
    app.post(
        &format!("/api/servers/{}/permissions", admin_server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "operator" }),
    )
    .await;

    // Admin creates another server with no access for user
    app.create_test_server(&admin_token, "Admin Private").await;

    let (status, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(status, StatusCode::OK);

    let servers = body["servers"].as_array().unwrap();
    assert_eq!(
        servers.len(),
        2,
        "User should see owned server + shared server"
    );

    let perm_map: std::collections::HashMap<String, (String, bool)> = servers
        .iter()
        .map(|s| {
            (
                s["server"]["config"]["name"].as_str().unwrap().to_string(),
                (
                    s["permission"]["level"].as_str().unwrap().to_string(),
                    s["permission"]["is_global_admin"].as_bool().unwrap(),
                ),
            )
        })
        .collect();

    let (level, is_admin) = perm_map.get("User Owned").expect("should see owned server");
    assert_eq!(level, "owner");
    assert!(!is_admin);

    let (level, is_admin) = perm_map
        .get("Admin Shared")
        .expect("should see shared server");
    assert_eq!(level, "operator");
    assert!(!is_admin);

    assert!(
        !perm_map.contains_key("Admin Private"),
        "Should not see server without access"
    );
}
