use axum::http::StatusCode;
use serde_json::json;

use crate::common::TestApp;

// ── Ticket 1-002: Root-level API routes must not exist ──────────────

#[tokio::test]
async fn test_root_auth_login_returns_404() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // POST /auth/login (without /api prefix) should 404
    let (status, _) = app
        .post(
            "/auth/login",
            None,
            json!({ "username": "admin", "password": TestApp::TEST_PASSWORD }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "/auth/login (no /api prefix) should return 404"
    );
}

#[tokio::test]
async fn test_root_auth_status_returns_404() {
    let app = TestApp::new().await;

    // GET /auth/status (without /api prefix) should 404
    let (status, _) = app.get("/auth/status", None).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "/auth/status (no /api prefix) should return 404"
    );
}

#[tokio::test]
async fn test_root_servers_returns_404() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // GET /servers (without /api prefix) should 404
    let (status, _) = app.get("/servers", Some(&token)).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "/servers (no /api prefix) should return 404"
    );
}

#[tokio::test]
async fn test_root_auth_setup_returns_404() {
    let app = TestApp::new().await;

    // POST /auth/setup (without /api prefix) should 404
    let (status, _) = app
        .post(
            "/auth/setup",
            None,
            json!({ "username": "admin", "password": TestApp::TEST_PASSWORD }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "/auth/setup (no /api prefix) should return 404"
    );
}

#[tokio::test]
async fn test_api_prefixed_auth_login_still_works() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // POST /api/auth/login should still work normally
    let (status, body) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": TestApp::TEST_PASSWORD }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "/api/auth/login should still work: {:?}",
        body
    );
    assert!(
        body["token"].as_str().is_some(),
        "Login response should contain a token"
    );
}

#[tokio::test]
async fn test_api_prefixed_auth_status_still_works() {
    let app = TestApp::new().await;

    // GET /api/auth/status should still work normally
    let (status, body) = app.get("/api/auth/status", None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "/api/auth/status should still work: {:?}",
        body
    );
    assert!(
        body.get("setup_complete").is_some(),
        "Status response should contain setup_complete field"
    );
}

#[tokio::test]
async fn test_rate_limit_cannot_be_bypassed_via_root_path() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Exhaust the credential rate-limit bucket via /api/auth/login
    for _ in 0..10 {
        let _ = app
            .post(
                "/api/auth/login",
                None,
                json!({ "username": "admin", "password": "WrongPass1" }),
            )
            .await;
    }

    // Trying /auth/login (root path) should return 404, NOT bypass the rate limit
    let (status, _) = app
        .post(
            "/auth/login",
            None,
            json!({ "username": "admin", "password": "WrongPass1" }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "/auth/login should return 404, not bypass rate limiting"
    );

    // And the /api/ prefixed route should still be rate-limited
    let resp = app
        .raw_request(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "username": "admin", "password": "WrongPass1" })),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "/api/auth/login should still be rate-limited after 10 requests"
    );
}
