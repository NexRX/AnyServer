use axum::http::StatusCode;
use serde_json::json;

use crate::common::TestApp;

// ─── Helper ────────────────────────────────────────────────────────────────────

/// Admin creates an invite code and returns (code_string, invite_id).
async fn create_invite(app: &TestApp, admin_token: &str) -> (String, String) {
    let (status, body) = app
        .post(
            "/api/admin/invite-codes",
            Some(admin_token),
            json!({
                "expiry": "seven_days",
                "assigned_role": "user",
                "assigned_permissions": [],
                "label": "test invite"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create invite failed: {:?}", body);
    let code = body["invite"]["code"]
        .as_str()
        .expect("missing code in response")
        .to_string();
    let id = body["invite"]["id"]
        .as_str()
        .expect("missing id in response")
        .to_string();
    (code, id)
}

/// Redeem an invite code with the given username/password. Returns (status, body).
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

/// Get the user id from the /api/auth/me endpoint.
async fn get_user_id(app: &TestApp, token: &str) -> String {
    let (status, body) = app.get("/api/auth/me", Some(token)).await;
    assert_eq!(status, StatusCode::OK, "get /auth/me failed: {:?}", body);
    body["user"]["id"]
        .as_str()
        .expect("missing user id in /auth/me response")
        .to_string()
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

/// Basic single-use enforcement: redeeming the same code twice should fail on the
/// second attempt (no user deletion involved).
#[tokio::test]
async fn invite_code_single_use() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (code, _invite_id) = create_invite(&app, &admin_token).await;

    // First redemption — should succeed.
    let (status, body) = redeem_invite(&app, &code, "usera", TestApp::TEST_PASSWORD).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "first redemption should succeed: {:?}",
        body
    );

    // Second redemption with a different username — should fail.
    let (status, body) = redeem_invite(&app, &code, "userb", TestApp::TEST_PASSWORD).await;
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::CONFLICT,
        "second redemption should fail, got {} with body: {:?}",
        status,
        body
    );
}

/// PRIMARY REGRESSION TEST.
///
/// An invite code that was redeemed must stay redeemed even after the user who
/// redeemed it is deleted by an admin. The root cause of the bug is that the
/// `ON DELETE SET NULL` FK on `redeemed_by` clears the only field the old code
/// checked, making the code appear unused again.
#[tokio::test]
async fn invite_code_not_reusable_after_user_deleted() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // 1. Create an invite code.
    let (code, _invite_id) = create_invite(&app, &admin_token).await;

    // 2. Redeem it with user A.
    let (status, body) = redeem_invite(&app, &code, "usera", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "redeem should succeed: {:?}", body);

    let user_a_token = body["token"].as_str().expect("missing token").to_string();
    let user_a_id = get_user_id(&app, &user_a_token).await;

    // 3. Admin deletes user A.
    let (status, body) = app
        .delete(
            &format!("/api/admin/users/{}", user_a_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "delete user should succeed: {:?}",
        body
    );

    // 4. Attempt to redeem the same code with user B — MUST fail.
    let (status, body) = redeem_invite(&app, &code, "userb", TestApp::TEST_PASSWORD).await;
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::CONFLICT,
        "BUG: invite code was reusable after user deletion! status={}, body={:?}",
        status,
        body
    );
}

/// After redeeming an invite and then deleting the redeemer, the admin invite
/// code list must still show the code as **not active** (is_active == false).
#[tokio::test]
async fn invite_code_admin_list_shows_redeemed_after_user_deleted() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (code, invite_id) = create_invite(&app, &admin_token).await;

    // Redeem.
    let (status, body) = redeem_invite(&app, &code, "usera", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "redeem failed: {:?}", body);

    let user_a_token = body["token"].as_str().unwrap().to_string();
    let user_a_id = get_user_id(&app, &user_a_token).await;

    // Delete the redeemer.
    let (status, _) = app
        .delete(
            &format!("/api/admin/users/{}", user_a_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Fetch the invite code via the admin detail endpoint.
    let (status, body) = app
        .get(
            &format!("/api/admin/invite-codes/{}", invite_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "get invite failed: {:?}", body);

    assert_eq!(
        body["is_active"], false,
        "BUG: invite code appears active after redeemer was deleted. body={:?}",
        body
    );
}

/// The `redeemed_at` timestamp must survive user deletion (it lives outside the
/// FK cascade). This is the field the fix relies on.
#[tokio::test]
async fn invite_code_redeemed_at_survives_user_deletion() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (code, invite_id) = create_invite(&app, &admin_token).await;

    // Redeem.
    let (status, body) = redeem_invite(&app, &code, "usera", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "redeem failed: {:?}", body);

    let user_a_token = body["token"].as_str().unwrap().to_string();
    let user_a_id = get_user_id(&app, &user_a_token).await;

    // Delete the redeemer.
    let (status, _) = app
        .delete(
            &format!("/api/admin/users/{}", user_a_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Fetch the invite code — redeemed_at must still be present.
    let (status, body) = app
        .get(
            &format!("/api/admin/invite-codes/{}", invite_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        !body["redeemed_at"].is_null(),
        "redeemed_at should survive user deletion, but it was null. body={:?}",
        body
    );
}

/// Concurrent redemption race condition: fire multiple redeem requests at the
/// same code simultaneously. Exactly one should succeed; the rest should fail,
/// and no orphaned user accounts should be left behind.
#[tokio::test]
async fn invite_code_concurrent_redemption() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let (code, _invite_id) = create_invite(&app, &admin_token).await;

    // Fire 5 concurrent redeem requests.
    let mut handles = Vec::new();
    for i in 0..5 {
        let code = code.clone();
        let username = format!("concurrent{}", i);
        // We need to clone the router/state for each task.
        let router = app.router.clone();
        handles.push(tokio::spawn(async move {
            use axum::body::Body;
            use axum::http::{header, Method, Request};
            use tower::ServiceExt;

            let body_json = serde_json::json!({
                "code": code,
                "username": username,
                "password": TestApp::TEST_PASSWORD,
            });

            let req = Request::builder()
                .method(Method::POST)
                .uri("/api/auth/redeem-invite")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body_json.to_string()))
                .expect("failed to build request");

            let resp = router.oneshot(req).await.expect("request failed");
            resp.status()
        }));
    }

    let mut results = Vec::new();
    for h in handles {
        results.push(h.await.expect("task panicked"));
    }

    let successes = results.iter().filter(|s| **s == StatusCode::OK).count();
    let failures = results
        .iter()
        .filter(|s| {
            **s == StatusCode::BAD_REQUEST
                || **s == StatusCode::CONFLICT
                || **s == StatusCode::TOO_MANY_REQUESTS
        })
        .count();

    assert_eq!(
        successes, 1,
        "exactly 1 concurrent redemption should succeed, got {}. statuses: {:?}",
        successes, results
    );
    assert_eq!(
        failures,
        results.len() - 1,
        "all other concurrent redemptions should fail. statuses: {:?}",
        results
    );

    // Verify that only one new user was created (admin + 1 redeemer = 2 total).
    let (status, body) = app.get("/api/admin/users", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    let user_count = body["users"].as_array().unwrap().len();
    assert_eq!(
        user_count, 2,
        "expected exactly 2 users (admin + 1 redeemer), got {}",
        user_count
    );
}

/// Attempting to redeem an invite code with a username that already exists must
/// fail with 409 Conflict, regardless of the code's validity.
#[tokio::test]
async fn test_redeem_duplicate_username_fails() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create two invite codes.
    let (code1, _) = create_invite(&app, &admin_token).await;
    let (code2, _) = create_invite(&app, &admin_token).await;

    // Redeem the first one.
    let (status, _) = redeem_invite(&app, &code1, "usera", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);

    // Try to redeem the second one with the same username.
    let (status, body) = redeem_invite(&app, &code2, "usera", TestApp::TEST_PASSWORD).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "duplicate username should fail: {:?}",
        body
    );
}

/// An expired invite code must not be redeemable.
#[tokio::test]
async fn test_redeem_expired_invite_code_fails() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create an invite code with the shortest expiry (30 minutes).
    // We can't easily fast-forward time, so instead we'll manipulate the DB
    // directly to set an already-expired timestamp.
    let (code, _invite_id) = create_invite(&app, &admin_token).await;

    // Expire the invite by updating its expires_at to the past.
    let past = chrono::Utc::now() - chrono::Duration::hours(1);
    let past_str = past.to_rfc3339();
    sqlx::query("UPDATE invite_codes SET expires_at = ? WHERE code = ?")
        .bind(&past_str)
        .bind(&code)
        .execute(app.state.db.pool())
        .await
        .expect("failed to expire invite code");

    // Attempt to redeem — should fail.
    let (status, body) = redeem_invite(&app, &code, "usera", TestApp::TEST_PASSWORD).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "expired invite should not be redeemable: {:?}",
        body
    );
}

/// An invalid (non-existent) invite code must fail.
#[tokio::test]
async fn test_redeem_invalid_invite_code_fails() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;
    // Ensure setup is complete so we pass that check.
    let _ = admin_token;

    let (status, body) = redeem_invite(&app, "000000", "usera", TestApp::TEST_PASSWORD).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "invalid code should fail: {:?}",
        body
    );
}

/// Admin can create, list, get, and delete invite codes.
#[tokio::test]
async fn test_invite_code_crud() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create.
    let (code, invite_id) = create_invite(&app, &admin_token).await;
    assert_eq!(code.len(), 8, "code should be 8 alphanumeric characters");

    // List.
    let (status, body) = app.get("/api/admin/invite-codes", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    let invites = body["invites"].as_array().unwrap();
    assert_eq!(invites.len(), 1);
    assert_eq!(invites[0]["code"].as_str().unwrap(), code);
    assert_eq!(invites[0]["is_active"], true);

    // Get by ID.
    let (status, body) = app
        .get(
            &format!("/api/admin/invite-codes/{}", invite_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"].as_str().unwrap(), code);

    // Delete.
    let (status, body) = app
        .delete(
            &format!("/api/admin/invite-codes/{}", invite_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted"], true);

    // Verify deleted.
    let (status, _) = app
        .get(
            &format!("/api/admin/invite-codes/{}", invite_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Non-admin users must not be able to create or list invite codes.
#[tokio::test]
async fn test_non_admin_cannot_manage_invite_codes() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _) = app.setup_admin_and_user().await;

    // Create — should fail for regular user.
    let (status, _) = app
        .post(
            "/api/admin/invite-codes",
            Some(&user_token),
            json!({
                "expiry": "seven_days",
                "assigned_role": "user",
                "assigned_permissions": [],
                "label": null
            }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // List — should fail for regular user.
    let (status, _) = app.get("/api/admin/invite-codes", Some(&user_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Verify admin can still do it (sanity).
    let (status, _) = app.get("/api/admin/invite-codes", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
}

/// Redeeming an invite code before setup is complete should fail.
#[tokio::test]
async fn test_redeem_before_setup_fails() {
    let app = TestApp::new().await;
    // Do NOT call setup_admin — setup is not complete.

    let (status, body) = redeem_invite(&app, "123456", "usera", TestApp::TEST_PASSWORD).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "redeem before setup should fail: {:?}",
        body
    );
}

/// Invite code with assigned role "admin" should create an admin user.
#[tokio::test]
async fn test_invite_code_assigns_role() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create an invite code that assigns admin role.
    let (status, body) = app
        .post(
            "/api/admin/invite-codes",
            Some(&admin_token),
            json!({
                "expiry": "seven_days",
                "assigned_role": "admin",
                "assigned_permissions": [],
                "label": "admin invite"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let code = body["invite"]["code"].as_str().unwrap().to_string();

    // Redeem.
    let (status, body) = redeem_invite(&app, &code, "newadmin", TestApp::TEST_PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "redeem failed: {:?}", body);

    // Verify the new user is an admin.
    let new_token = body["token"].as_str().unwrap();
    let (status, body) = app.get("/api/auth/me", Some(new_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["user"]["role"], "admin",
        "redeemed user should be admin"
    );
}
