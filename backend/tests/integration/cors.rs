use axum::http::{header, Method, Request};
use tower::ServiceExt;

use crate::common::TestApp;

/// Helper: send a preflight (OPTIONS) request with the given Origin header
/// and return the raw response so we can inspect CORS headers.
async fn preflight(app: &TestApp, origin: &str, method: &str) -> axum::response::Response {
    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/auth/status")
        .header(header::ORIGIN, origin)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, method)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::empty())
        .expect("failed to build preflight request");

    app.router
        .clone()
        .oneshot(req)
        .await
        .expect("preflight request failed")
}

/// Helper: send a simple GET with an Origin header and return the raw response.
async fn get_with_origin(app: &TestApp, uri: &str, origin: &str) -> axum::response::Response {
    let req = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::ORIGIN, origin)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::empty())
        .expect("failed to build request");

    app.router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed")
}

// ── Dev-mode CORS defaults to http://localhost:3000 ─────────────────

#[tokio::test]
async fn test_cors_allows_localhost_3000_origin() {
    let app = TestApp::new().await;

    let resp = get_with_origin(&app, "/api/auth/status", "http://localhost:3000").await;

    // The response should include the allowed origin header
    let acao = resp
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .map(|v| v.to_str().unwrap_or(""));

    assert_eq!(
        acao,
        Some("http://localhost:3000"),
        "Dev mode should allow http://localhost:3000"
    );
}

#[tokio::test]
async fn test_cors_allows_credentials_for_localhost() {
    let app = TestApp::new().await;

    let resp = get_with_origin(&app, "/api/auth/status", "http://localhost:3000").await;

    let acac = resp
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
        .map(|v| v.to_str().unwrap_or(""));

    assert_eq!(
        acac,
        Some("true"),
        "Dev mode should set Access-Control-Allow-Credentials: true for localhost:3000"
    );
}

#[tokio::test]
async fn test_cors_rejects_evil_origin() {
    let app = TestApp::new().await;

    let resp = get_with_origin(&app, "/api/auth/status", "http://evil.example.com").await;

    // For a non-matching origin, tower-http's CorsLayer does NOT set the
    // Access-Control-Allow-Origin header at all (the response still goes
    // through, but the browser will block the cross-origin read).
    let acao = resp
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .map(|v| v.to_str().unwrap_or(""));

    assert!(
        acao.is_none() || acao != Some("http://evil.example.com"),
        "Dev mode should NOT reflect an evil origin. Got: {:?}",
        acao
    );
}

#[tokio::test]
async fn test_cors_preflight_localhost_returns_allow_methods() {
    let app = TestApp::new().await;

    let resp = preflight(&app, "http://localhost:3000", "POST").await;

    let acao = resp
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .map(|v| v.to_str().unwrap_or(""));

    assert_eq!(
        acao,
        Some("http://localhost:3000"),
        "Preflight for localhost:3000 should be allowed"
    );

    // Should list allowed methods
    let acam = resp
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_METHODS)
        .map(|v| v.to_str().unwrap_or(""));

    assert!(
        acam.is_some(),
        "Preflight should include Access-Control-Allow-Methods"
    );

    let methods_str = acam.unwrap();
    // Should include common methods
    assert!(
        methods_str.contains("GET") || methods_str.contains("get"),
        "Allowed methods should include GET, got: {}",
        methods_str
    );
    assert!(
        methods_str.contains("POST") || methods_str.contains("post"),
        "Allowed methods should include POST, got: {}",
        methods_str
    );
}

#[tokio::test]
async fn test_cors_preflight_evil_origin_not_allowed() {
    let app = TestApp::new().await;

    let resp = preflight(&app, "http://evil.example.com", "POST").await;

    let acao = resp
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .map(|v| v.to_str().unwrap_or(""));

    assert!(
        acao.is_none() || acao != Some("http://evil.example.com"),
        "Preflight for evil origin should NOT be allowed. Got: {:?}",
        acao
    );
}

#[tokio::test]
async fn test_cors_does_not_use_wildcard_origin() {
    let app = TestApp::new().await;

    // Send a request with localhost origin
    let resp = get_with_origin(&app, "/api/auth/status", "http://localhost:3000").await;

    let acao = resp
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .map(|v| v.to_str().unwrap_or(""));

    // Should be the exact origin, NOT "*"
    assert_ne!(
        acao,
        Some("*"),
        "Dev mode should NOT use wildcard origin — should be explicit http://localhost:3000"
    );
    assert_eq!(acao, Some("http://localhost:3000"));
}
