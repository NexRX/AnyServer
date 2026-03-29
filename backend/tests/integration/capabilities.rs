use axum::http::StatusCode;
use serde_json::json;

use crate::common::{resolve_binary, TestApp};

// ─── Helpers (user search) ─────────────────────────────────────────────────────

/// Create a user via invite with given capabilities. Returns (token, user_id).
async fn create_user_via_invite(
    app: &TestApp,
    admin_token: &str,
    username: &str,
    caps: &[&str],
) -> (String, String) {
    let (code, _) = create_invite_with_caps(app, admin_token, caps).await;
    let (status, body) = redeem_invite(app, &code, username, TestApp::TEST_PASSWORD).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "redeem failed for {}: {:?}",
        username,
        body
    );
    let token = body["token"].as_str().unwrap().to_string();
    let id = get_user_id(app, &token).await;
    (token, id)
}

// ─── Helpers ───────────────────────────────────────────────────────────────────

/// Create an invite code with specific capabilities. Returns (code, invite_id).
async fn create_invite_with_caps(
    app: &TestApp,
    admin_token: &str,
    capabilities: &[&str],
) -> (String, String) {
    let (status, body) = app
        .post(
            "/api/admin/invite-codes",
            Some(admin_token),
            json!({
                "expiry": "seven_days",
                "assigned_role": "user",
                "assigned_permissions": [],
                "assigned_capabilities": capabilities,
                "label": "cap test invite"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create invite failed: {:?}", body);
    let code = body["invite"]["code"].as_str().unwrap().to_string();
    let id = body["invite"]["id"].as_str().unwrap().to_string();
    (code, id)
}

/// Redeem an invite code. Returns (status, body).
async fn redeem_invite(
    app: &TestApp,
    code: &str,
    username: &str,
    password: &str,
) -> (StatusCode, serde_json::Value) {
    app.post(
        "/api/auth/redeem-invite",
        None,
        json!({
            "code": code,
            "username": username,
            "password": password
        }),
    )
    .await
}

/// Get the user id from /api/auth/me.
async fn get_user_id(app: &TestApp, token: &str) -> String {
    let (status, body) = app.get("/api/auth/me", Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    body["user"]["id"].as_str().unwrap().to_string()
}

/// Get the user's capabilities from /api/auth/me.
async fn get_user_capabilities(app: &TestApp, token: &str) -> Vec<String> {
    let (status, body) = app.get("/api/auth/me", Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    body["user"]["global_capabilities"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

// ─── CreateServers capability ──────────────────────────────────────────────────

#[tokio::test]
async fn user_without_create_servers_cap_cannot_create_server() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create an invite with NO capabilities
    let (code, _) = create_invite_with_caps(&app, &admin_token, &[]).await;

    // Redeem to create a user with no capabilities
    let (status, body) = redeem_invite(&app, &code, "nocaps", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "redeem failed: {:?}", body);
    let user_token = body["token"].as_str().unwrap().to_string();

    // Verify user has no capabilities
    let caps = get_user_capabilities(&app, &user_token).await;
    assert!(caps.is_empty(), "user should have no capabilities");

    // Try to create a server — should be FORBIDDEN
    let echo = resolve_binary("echo");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&user_token),
            json!({
                "config": {
                    "name": "forbidden-server",
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
        StatusCode::FORBIDDEN,
        "user without CreateServers should get 403: {:?}",
        body
    );
    assert!(
        body["error"]
            .as_str()
            .unwrap_or("")
            .contains("CreateServers"),
        "error should mention CreateServers capability: {:?}",
        body
    );
}

#[tokio::test]
async fn user_with_create_servers_cap_can_create_server() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create an invite WITH CreateServers capability
    let (code, _) = create_invite_with_caps(&app, &admin_token, &["create_servers"]).await;

    let (status, body) = redeem_invite(&app, &code, "cancreate", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let user_token = body["token"].as_str().unwrap().to_string();

    // Verify user has the capability
    let caps = get_user_capabilities(&app, &user_token).await;
    assert!(
        caps.contains(&"create_servers".to_string()),
        "user should have create_servers capability, got: {:?}",
        caps
    );

    // Create a server — should succeed
    let (_, _server_body) = app.create_test_server(&user_token, "allowed-server").await;
}

#[tokio::test]
async fn admin_can_always_create_server() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Admin should be able to create a server without any explicit capability
    let (server_id, _) = app.create_test_server(&admin_token, "admin-server").await;
    assert!(!server_id.is_empty());
}

// ─── ManageTemplates capability ────────────────────────────────────────────────

#[tokio::test]
async fn user_without_manage_templates_cap_cannot_create_template() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create user with NO capabilities
    let (code, _) = create_invite_with_caps(&app, &admin_token, &[]).await;
    let (status, body) = redeem_invite(&app, &code, "notempl", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let user_token = body["token"].as_str().unwrap().to_string();

    let echo = resolve_binary("echo");

    // Try to create a template — should fail
    let (status, body) = app
        .post(
            "/api/templates",
            Some(&user_token),
            json!({
                "name": "My Template",
                "description": "test",
                "config": {
                    "name": "template-server",
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
        StatusCode::FORBIDDEN,
        "user without ManageTemplates should get 403: {:?}",
        body
    );
}

#[tokio::test]
async fn user_with_manage_templates_cap_can_create_template() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create user WITH ManageTemplates
    let (code, _) = create_invite_with_caps(&app, &admin_token, &["manage_templates"]).await;
    let (status, body) = redeem_invite(&app, &code, "templmgr", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let user_token = body["token"].as_str().unwrap().to_string();

    let echo = resolve_binary("echo");

    // Create a template — should succeed
    let (status, body) = app
        .post(
            "/api/templates",
            Some(&user_token),
            json!({
                "name": "User Template",
                "description": "test template",
                "config": {
                    "name": "template-server",
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
        "user with ManageTemplates should succeed: {:?}",
        body
    );
}

// ─── Invite code grants capabilities ───────────────────────────────────────────

#[tokio::test]
async fn invite_code_grants_capabilities() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create invite with both capabilities
    let (code, _) =
        create_invite_with_caps(&app, &admin_token, &["create_servers", "manage_templates"]).await;

    let (status, body) = redeem_invite(&app, &code, "fullcaps", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let user_token = body["token"].as_str().unwrap().to_string();

    let caps = get_user_capabilities(&app, &user_token).await;
    assert!(
        caps.contains(&"create_servers".to_string()),
        "should have create_servers"
    );
    assert!(
        caps.contains(&"manage_templates".to_string()),
        "should have manage_templates"
    );

    // Verify the user can actually create a server
    let (server_id, _) = app
        .create_test_server(&user_token, "cap-granted-server")
        .await;
    assert!(!server_id.is_empty());
}

#[tokio::test]
async fn invite_code_without_capabilities_creates_unprivileged_user() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (code, _) = create_invite_with_caps(&app, &admin_token, &[]).await;

    let (status, body) = redeem_invite(&app, &code, "unpriv", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let user_token = body["token"].as_str().unwrap().to_string();

    let caps = get_user_capabilities(&app, &user_token).await;
    assert!(
        caps.is_empty(),
        "user from invite with no caps should have empty capabilities"
    );
}

// ─── Admin capability management ───────────────────────────────────────────────

#[tokio::test]
async fn admin_can_manage_user_capabilities() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create a user with no capabilities
    let (code, _) = create_invite_with_caps(&app, &admin_token, &[]).await;
    let (status, body) = redeem_invite(&app, &code, "managed", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let user_token = body["token"].as_str().unwrap().to_string();
    let user_id = get_user_id(&app, &user_token).await;

    // Verify user can't create servers yet
    let echo = resolve_binary("echo");
    let (status, _) = app
        .post(
            "/api/servers",
            Some(&user_token),
            json!({
                "config": {
                    "name": "should-fail",
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
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Admin grants CreateServers capability
    let (status, body) = app
        .put(
            &format!("/api/admin/users/{}/capabilities", user_id),
            Some(&admin_token),
            json!({
                "global_capabilities": ["create_servers"]
            }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "admin should be able to update capabilities: {:?}",
        body
    );

    // Verify capabilities were updated
    let caps = get_user_capabilities(&app, &user_token).await;
    assert!(
        caps.contains(&"create_servers".to_string()),
        "user should now have create_servers"
    );

    // Verify user CAN now create servers
    let (server_id, _) = app
        .create_test_server(&user_token, "now-allowed-server")
        .await;
    assert!(!server_id.is_empty());
}

#[tokio::test]
async fn admin_can_revoke_user_capabilities() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create a user WITH create_servers
    let (code, _) = create_invite_with_caps(&app, &admin_token, &["create_servers"]).await;
    let (status, body) = redeem_invite(&app, &code, "revokee", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let user_token = body["token"].as_str().unwrap().to_string();
    let user_id = get_user_id(&app, &user_token).await;

    // User can create a server initially
    let (server_id, _) = app.create_test_server(&user_token, "before-revoke").await;
    assert!(!server_id.is_empty());

    // Admin revokes ALL capabilities
    let (status, _) = app
        .put(
            &format!("/api/admin/users/{}/capabilities", user_id),
            Some(&admin_token),
            json!({
                "global_capabilities": []
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // User can no longer create servers
    let echo = resolve_binary("echo");
    let (status, _) = app
        .post(
            "/api/servers",
            Some(&user_token),
            json!({
                "config": {
                    "name": "after-revoke",
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
        StatusCode::FORBIDDEN,
        "user should no longer be able to create servers after revocation"
    );
}

#[tokio::test]
async fn non_admin_cannot_manage_capabilities() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create two regular users
    let (code1, _) = create_invite_with_caps(&app, &admin_token, &[]).await;
    let (status, body) = redeem_invite(&app, &code1, "userx", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let user_x_token = body["token"].as_str().unwrap().to_string();

    let (code2, _) = create_invite_with_caps(&app, &admin_token, &[]).await;
    let (status, body) = redeem_invite(&app, &code2, "usery", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let user_y_id = get_user_id(&app, body["token"].as_str().unwrap()).await;

    // User X tries to grant capabilities to User Y — should fail
    let (status, _) = app
        .put(
            &format!("/api/admin/users/{}/capabilities", user_y_id),
            Some(&user_x_token),
            json!({
                "global_capabilities": ["create_servers"]
            }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "non-admin should not be able to manage capabilities"
    );
}

#[tokio::test]
async fn update_capabilities_on_admin_user_returns_error() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Get admin's own user ID
    let admin_id = get_user_id(&app, &admin_token).await;

    // Try to update capabilities on an admin — should fail gracefully
    let (status, body) = app
        .put(
            &format!("/api/admin/users/{}/capabilities", admin_id),
            Some(&admin_token),
            json!({
                "global_capabilities": ["create_servers"]
            }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "updating capabilities on an admin should return 400: {:?}",
        body
    );
}

// ─── Capabilities survive in user list ─────────────────────────────────────────

#[tokio::test]
async fn user_list_includes_capabilities() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create user with specific capabilities
    let (code, _) = create_invite_with_caps(
        &app,
        &admin_token,
        &["create_servers", "view_system_health"],
    )
    .await;
    let (status, _) = redeem_invite(&app, &code, "capuser", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);

    // List users
    let (status, body) = app.get("/api/admin/users", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);

    let users = body["users"].as_array().unwrap();
    let cap_user = users
        .iter()
        .find(|u| u["username"] == "capuser")
        .expect("capuser should be in user list");

    let caps: Vec<String> = cap_user["global_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert!(caps.contains(&"create_servers".to_string()));
    assert!(caps.contains(&"view_system_health".to_string()));
}

// ─── Invite code capabilities visible in admin list ────────────────────────────

#[tokio::test]
async fn invite_code_includes_assigned_capabilities() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (_, invite_id) =
        create_invite_with_caps(&app, &admin_token, &["create_servers", "manage_templates"]).await;

    // Get the invite code by ID
    let (status, body) = app
        .get(
            &format!("/api/admin/invite-codes/{}", invite_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let caps: Vec<String> = body["assigned_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert!(caps.contains(&"create_servers".to_string()));
    assert!(caps.contains(&"manage_templates".to_string()));
}

// ─── Registration without invite code creates user with no capabilities ────────

#[tokio::test]
async fn registered_user_has_no_capabilities() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;
    app.enable_registration(&admin_token).await;

    let user_token = app.register_user("reguser", TestApp::TEST_PASSWORD).await;

    let caps = get_user_capabilities(&app, &user_token).await;
    assert!(
        caps.is_empty(),
        "registered user (not via invite) should have no capabilities"
    );

    // Registered user should NOT be able to create servers
    let echo = resolve_binary("echo");
    let (status, _) = app
        .post(
            "/api/servers",
            Some(&user_token),
            json!({
                "config": {
                    "name": "reg-server",
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
        StatusCode::FORBIDDEN,
        "registered user without capabilities should not create servers"
    );
}

// ─── Multiple capabilities work independently ──────────────────────────────────

#[tokio::test]
async fn capabilities_are_independent() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // User with only ManageTemplates — can manage templates but NOT create servers
    let (code, _) = create_invite_with_caps(&app, &admin_token, &["manage_templates"]).await;
    let (status, body) = redeem_invite(&app, &code, "templonly", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let user_token = body["token"].as_str().unwrap().to_string();

    // Should NOT be able to create a server
    let echo = resolve_binary("echo");
    let (status, _) = app
        .post(
            "/api/servers",
            Some(&user_token),
            json!({
                "config": {
                    "name": "no-create",
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
        StatusCode::FORBIDDEN,
        "user with only ManageTemplates should not create servers"
    );

    // SHOULD be able to create a template
    let (status, _) = app
        .post(
            "/api/templates",
            Some(&user_token),
            json!({
                "name": "Template by templonly",
                "description": "test",
                "config": {
                    "name": "t",
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
        "user with ManageTemplates should be able to create templates"
    );
}

// ─── Capability check is fresh from DB (not stale JWT) ─────────────────────────

#[tokio::test]
async fn capability_check_uses_fresh_db_state() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create user with CreateServers
    let (code, _) = create_invite_with_caps(&app, &admin_token, &["create_servers"]).await;
    let (status, body) = redeem_invite(&app, &code, "freshcheck", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let user_token = body["token"].as_str().unwrap().to_string();
    let user_id = get_user_id(&app, &user_token).await;

    // User can create a server (capabilities are in DB)
    let (server_id, _) = app
        .create_test_server(&user_token, "before-db-change")
        .await;
    assert!(!server_id.is_empty());

    // Admin revokes the capability in the DB
    let (status, _) = app
        .put(
            &format!("/api/admin/users/{}/capabilities", user_id),
            Some(&admin_token),
            json!({ "global_capabilities": [] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Same JWT token, but capability was revoked in DB — should now fail
    let echo = resolve_binary("echo");
    let (status, _) = app
        .post(
            "/api/servers",
            Some(&user_token),
            json!({
                "config": {
                    "name": "after-db-change",
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
        StatusCode::FORBIDDEN,
        "capability revocation should take effect immediately (fresh DB check)"
    );
}

// ─── User search endpoint ──────────────────────────────────────────────────────

#[tokio::test]
async fn user_search_returns_matching_users() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (_alice_token, _) = create_user_via_invite(&app, &admin_token, "alice", &[]).await;
    let (_bob_token, _) = create_user_via_invite(&app, &admin_token, "bob", &[]).await;
    let (_alex_token, _) = create_user_via_invite(&app, &admin_token, "alex", &[]).await;

    // Search for "al" — should match alice and alex but not bob
    let (status, body) = app.get("/api/users/search?q=al", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    let users = body["users"].as_array().unwrap();
    assert_eq!(users.len(), 2, "expected 2 matches for 'al': {:?}", users);
    let names: Vec<&str> = users
        .iter()
        .map(|u| u["username"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"alice"), "should contain alice");
    assert!(names.contains(&"alex"), "should contain alex");
}

#[tokio::test]
async fn user_search_accessible_to_non_admin() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (user_token, _) = create_user_via_invite(&app, &admin_token, "searcher", &[]).await;
    let (_other_token, _) = create_user_via_invite(&app, &admin_token, "findme", &[]).await;

    // Non-admin can search
    let (status, body) = app.get("/api/users/search?q=find", Some(&user_token)).await;
    assert_eq!(status, StatusCode::OK);
    let users = body["users"].as_array().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["username"], "findme");
}

#[tokio::test]
async fn user_search_empty_query_returns_all() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (_u1, _) = create_user_via_invite(&app, &admin_token, "userone", &[]).await;
    let (_u2, _) = create_user_via_invite(&app, &admin_token, "usertwo", &[]).await;

    let (status, body) = app.get("/api/users/search?q=", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    let users = body["users"].as_array().unwrap();
    // admin + userone + usertwo = 3
    assert!(
        users.len() >= 3,
        "expected at least 3 users, got {}",
        users.len()
    );
}

#[tokio::test]
async fn user_search_requires_authentication() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (status, _) = app.get("/api/users/search?q=admin", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ─── Server owner permission management ────────────────────────────────────────

#[tokio::test]
async fn server_owner_can_grant_viewer_access() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create owner with CreateServers capability
    let (owner_token, _owner_id) =
        create_user_via_invite(&app, &admin_token, "owner", &["create_servers"]).await;

    // Create a second user to grant access to
    let (viewer_token, viewer_id) = create_user_via_invite(&app, &admin_token, "viewer", &[]).await;

    // Owner creates a server
    let (server_id, _) = app.create_test_server(&owner_token, "Owner's Server").await;

    // Viewer cannot see the server yet
    let (status, _) = app
        .get(&format!("/api/servers/{}", server_id), Some(&viewer_token))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Owner grants viewer access
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&owner_token),
            json!({ "user_id": viewer_id, "level": "viewer" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "owner grant failed: {:?}", body);
    assert_eq!(body["level"], "viewer");

    // Viewer CAN now see the server
    let (status, _) = app
        .get(&format!("/api/servers/{}", server_id), Some(&viewer_token))
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn server_owner_can_revoke_access() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (owner_token, _) =
        create_user_via_invite(&app, &admin_token, "owner", &["create_servers"]).await;
    let (viewer_token, viewer_id) = create_user_via_invite(&app, &admin_token, "viewer", &[]).await;

    let (server_id, _) = app.create_test_server(&owner_token, "Revoke Test").await;

    // Grant then revoke
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&owner_token),
        json!({ "user_id": viewer_id, "level": "viewer" }),
    )
    .await;

    // Verify access
    let (status, _) = app
        .get(&format!("/api/servers/{}", server_id), Some(&viewer_token))
        .await;
    assert_eq!(status, StatusCode::OK);

    // Owner revokes
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions/remove", server_id),
            Some(&owner_token),
            json!({ "user_id": viewer_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "revoke failed: {:?}", body);
    assert_eq!(body["removed"], true);

    // Viewer can NO LONGER see the server
    let (status, _) = app
        .get(&format!("/api/servers/{}", server_id), Some(&viewer_token))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn server_owner_can_list_permissions() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (owner_token, _) =
        create_user_via_invite(&app, &admin_token, "owner", &["create_servers"]).await;
    let (_viewer_token, viewer_id) =
        create_user_via_invite(&app, &admin_token, "viewer", &[]).await;

    let (server_id, _) = app.create_test_server(&owner_token, "List Perms").await;

    // Grant viewer
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&owner_token),
        json!({ "user_id": viewer_id, "level": "operator" }),
    )
    .await;

    // Owner can list permissions
    let (status, body) = app
        .get(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&owner_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let perms = body["permissions"].as_array().unwrap();
    assert!(
        perms.len() >= 2,
        "expected owner + operator, got {:?}",
        perms
    );

    let owner_entry = perms.iter().find(|p| p["level"] == "owner");
    assert!(owner_entry.is_some(), "owner should appear in list");

    let viewer_entry = perms.iter().find(|p| p["user"]["username"] == "viewer");
    assert!(viewer_entry.is_some(), "viewer should appear in list");
    assert_eq!(viewer_entry.unwrap()["level"], "operator");
}

#[tokio::test]
async fn server_admin_cannot_exceed_own_level() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (owner_token, _) =
        create_user_via_invite(&app, &admin_token, "owner", &["create_servers"]).await;
    let (server_admin_token, server_admin_id) =
        create_user_via_invite(&app, &admin_token, "serveradmin", &[]).await;
    let (_third_token, third_id) =
        create_user_via_invite(&app, &admin_token, "thirduser", &[]).await;

    let (server_id, _) = app.create_test_server(&owner_token, "Level Test").await;

    // Owner grants "admin" level to serveradmin
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&owner_token),
        json!({ "user_id": server_admin_id, "level": "admin" }),
    )
    .await;

    // Server admin tries to grant "owner" level to third user — should fail
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&server_admin_token),
            json!({ "user_id": third_id, "level": "owner" }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "server admin should not be able to grant owner: {:?}",
        body
    );

    // Server admin CAN grant up to manager (below their own level)
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&server_admin_token),
            json!({ "user_id": third_id, "level": "manager" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "grant manager failed: {:?}", body);
    assert_eq!(body["level"], "manager");
}

#[tokio::test]
async fn unprivileged_user_sees_only_permitted_servers() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (owner_token, _) =
        create_user_via_invite(&app, &admin_token, "owner", &["create_servers"]).await;
    let (user_token, user_id) =
        create_user_via_invite(&app, &admin_token, "limiteduser", &[]).await;

    // Owner creates two servers
    let (server_a, _) = app.create_test_server(&owner_token, "Server A").await;
    let (server_b, _) = app.create_test_server(&owner_token, "Server B").await;

    // Grant viewer on A only
    app.post(
        &format!("/api/servers/{}/permissions", server_a),
        Some(&owner_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // User lists servers — should see only A
    let (status, body) = app.get("/api/servers", Some(&user_token)).await;
    assert_eq!(status, StatusCode::OK);

    let servers = body["servers"].as_array().unwrap();
    assert_eq!(
        servers.len(),
        1,
        "expected exactly 1 server, got {:?}",
        servers
    );
    assert_eq!(servers[0]["server"]["config"]["name"], "Server A");

    // User cannot access B directly
    let (status, _) = app
        .get(&format!("/api/servers/{}", server_b), Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn server_owner_can_promote_and_demote_users() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (owner_token, _) =
        create_user_via_invite(&app, &admin_token, "owner", &["create_servers"]).await;
    let (user_token, user_id) = create_user_via_invite(&app, &admin_token, "promoted", &[]).await;

    let (server_id, _) = app.create_test_server(&owner_token, "Promote Test").await;

    // Grant viewer
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&owner_token),
            json!({ "user_id": user_id, "level": "viewer" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // User cannot write files as viewer
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(&user_token),
            json!({ "path": "test.txt", "content": "hello" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Owner promotes to manager
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&owner_token),
            json!({ "user_id": user_id, "level": "manager" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["level"], "manager");

    // User CAN now write files as manager
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(&user_token),
            json!({ "path": "test.txt", "content": "hello" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Owner demotes back to viewer
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&owner_token),
            json!({ "user_id": user_id, "level": "viewer" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // User can no longer write files
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(&user_token),
            json!({ "path": "test2.txt", "content": "nope" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn non_admin_server_viewer_cannot_manage_permissions() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (owner_token, _) =
        create_user_via_invite(&app, &admin_token, "owner", &["create_servers"]).await;
    let (viewer_token, _viewer_id) =
        create_user_via_invite(&app, &admin_token, "justviewer", &[]).await;
    let (_other_token, other_id) = create_user_via_invite(&app, &admin_token, "other", &[]).await;

    let (server_id, _) = app.create_test_server(&owner_token, "NoPerms Test").await;

    // Grant viewer to justviewer
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&owner_token),
        json!({ "user_id": _viewer_id, "level": "viewer" }),
    )
    .await;

    // Viewer tries to grant access — should fail (needs Admin level)
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&viewer_token),
            json!({ "user_id": other_id, "level": "viewer" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Viewer tries to list permissions — should fail
    let (status, _) = app
        .get(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&viewer_token),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
