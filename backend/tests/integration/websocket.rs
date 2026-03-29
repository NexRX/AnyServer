use axum::http::StatusCode;
use serde_json::json;

use crate::common::TestApp;

#[tokio::test]
async fn test_ws_without_token_param_is_rejected() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "WS Test").await;

    // GET without ?token= — rejected during WebSocket upgrade or by our auth check.
    // Without a proper `Upgrade: websocket` header axum may return 400 (Bad Request)
    // instead of reaching our handler's auth logic. Either 400 or 401 is acceptable
    // since the request is invalid/unauthenticated either way.
    let (status, _) = app
        .get(&format!("/api/servers/{}/ws", server_id), None)
        .await;
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED,
        "expected 400 or 401, got {}",
        status
    );
}

#[tokio::test]
async fn test_ws_with_invalid_token_is_rejected() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "WS Test").await;

    // Without a proper WebSocket upgrade header, axum may reject with 400 before
    // our handler even runs. Both 400 and 401 are acceptable outcomes.
    let (status, _) = app
        .get(
            &format!("/api/servers/{}/ws?token=invalid.jwt.garbage", server_id),
            None,
        )
        .await;
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED,
        "expected 400 or 401, got {}",
        status
    );
}

// ─── WebSocket Ticket Authentication Tests ───

#[tokio::test]
async fn test_ws_ticket_generation() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Request a WebSocket ticket without scope.
    let (status, body) = app
        .post("/api/auth/ws-ticket", Some(&token), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);

    assert!(body.get("ticket").is_some());
    let ticket = body["ticket"].as_str().unwrap();
    assert!(!ticket.is_empty());

    // Ticket should be URL-safe base64 (no +, /, or =).
    assert!(ticket
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
}

#[tokio::test]
async fn test_ws_ticket_generation_with_scope() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Scoped Test").await;

    // Request a WebSocket ticket with scope.
    let scope = format!("/api/servers/{}/ws", server_id);
    let (status, body) = app
        .post(
            "/api/auth/ws-ticket",
            Some(&token),
            json!({ "scope": scope }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert!(body.get("ticket").is_some());
}

#[tokio::test]
async fn test_ws_ticket_requires_authentication() {
    let app = TestApp::new().await;

    // Request a WebSocket ticket without authentication.
    let (status, _) = app.post("/api/auth/ws-ticket", None, json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_ws_ticket_single_use() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Mint a ticket.
    let (status, body) = app
        .post("/api/auth/ws-ticket", Some(&token), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = body["ticket"].as_str().unwrap();

    // The ticket store is in memory, so we can't directly test redemption
    // without a WebSocket upgrade, but we can verify the ticket was minted.
    assert!(!ticket.is_empty());
}

#[tokio::test]
async fn test_ws_ticket_max_per_user() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Request the maximum number of tickets (10).
    for _ in 0..10 {
        let (status, _) = app
            .post("/api/auth/ws-ticket", Some(&token), json!({}))
            .await;
        assert_eq!(status, StatusCode::OK);
    }

    // The 11th request should fail with 429 Too Many Requests.
    let (status, body) = app
        .post("/api/auth/ws-ticket", Some(&token), json!({}))
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let error_msg = body["error"].as_str().unwrap_or("");
    assert!(error_msg.contains("Too many"));
}

#[tokio::test]
async fn test_ws_with_invalid_ticket_is_rejected() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Ticket Test").await;

    // Try to connect with an invalid ticket.
    let (status, _) = app
        .get(
            &format!("/api/servers/{}/ws?ticket=invalid_ticket_abc123", server_id),
            None,
        )
        .await;
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED,
        "expected 400 or 401, got {}",
        status
    );
}

#[tokio::test]
async fn test_global_events_ws_ticket_generation() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Request a WebSocket ticket for global events.
    let (status, body) = app
        .post(
            "/api/auth/ws-ticket",
            Some(&token),
            json!({ "scope": "/api/ws/events" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert!(body.get("ticket").is_some());
}

#[tokio::test]
async fn test_global_events_ws_without_auth_rejected() {
    let app = TestApp::new().await;

    // Try to connect to global events without any authentication.
    let (status, _) = app.get("/api/ws/events", None).await;
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED,
        "expected 400 or 401, got {}",
        status
    );
}

#[tokio::test]
async fn test_ws_ticket_different_users_independent() {
    let app = TestApp::new().await;

    // Setup two users.
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    app.enable_registration(&admin_token).await;
    let user_token = app.register_user("user1", "User1234").await;

    // Each user can mint their own tickets.
    let (status1, body1) = app
        .post("/api/auth/ws-ticket", Some(&admin_token), json!({}))
        .await;
    assert_eq!(status1, StatusCode::OK);

    let (status2, body2) = app
        .post("/api/auth/ws-ticket", Some(&user_token), json!({}))
        .await;
    assert_eq!(status2, StatusCode::OK);

    // Tickets should be different.
    assert_ne!(
        body1["ticket"].as_str().unwrap(),
        body2["ticket"].as_str().unwrap()
    );
}
