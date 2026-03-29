use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::common::TestApp;

// ── Ticket 1-005: CSRF protection on /api/auth/refresh ──────────────

/// Helper: send a POST to /api/auth/refresh with optional custom headers.
async fn refresh_request(
    app: &TestApp,
    cookie: Option<&str>,
    x_requested_with: Option<&str>,
) -> (StatusCode, Value, axum::response::Response<Body>) {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/refresh")
        .header(header::CONTENT_TYPE, "application/json");

    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }

    if let Some(xrw) = x_requested_with {
        builder = builder.header("x-requested-with", xrw);
    }

    let req = builder
        .body(Body::from("{}"))
        .expect("failed to build request");

    let resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed");

    let status = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();

    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).to_string()))
    };

    // We already consumed the body, build a minimal response for the status
    let fake_resp = axum::response::Response::builder()
        .status(status)
        .body(Body::empty())
        .unwrap();

    (status, value, fake_resp)
}

/// Helper: do a full login and extract the Set-Cookie header to get the
/// refresh cookie value.
#[allow(dead_code)]
async fn login_and_get_refresh_cookie(app: &TestApp) -> Option<String> {
    let resp = app
        .raw_request(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({
                "username": "admin",
                "password": TestApp::TEST_PASSWORD
            })),
        )
        .await;

    assert_eq!(resp.status(), StatusCode::OK);

    // Extract the Set-Cookie header that contains the refresh token
    for value in resp.headers().get_all(header::SET_COOKIE).iter() {
        let cookie_str = value.to_str().unwrap_or("");
        if cookie_str.contains("anyserver_refresh=") {
            return Some(cookie_str.to_string());
        }
    }
    None
}

#[tokio::test]
async fn test_refresh_without_csrf_header_returns_403() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Send a refresh request WITHOUT X-Requested-With header
    let (status, body, _) = refresh_request(&app, None, None).await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "Refresh without X-Requested-With should return 403, got: {:?}",
        body
    );

    let error_msg = body["error"].as_str().unwrap_or("");
    assert!(
        error_msg.contains("CSRF") || error_msg.contains("X-Requested-With"),
        "Error message should mention CSRF or X-Requested-With, got: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_refresh_with_csrf_header_proceeds() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Send a refresh request WITH X-Requested-With header but no cookie
    // — should get past the CSRF check and fail on missing cookie (401),
    // NOT on CSRF (403).
    let (status, body, _) = refresh_request(&app, None, Some("AnyServer")).await;

    assert_ne!(
        status,
        StatusCode::FORBIDDEN,
        "Refresh WITH X-Requested-With should not return 403. Got: {:?}",
        body
    );

    // Without a valid cookie, it should be 401 Unauthorized
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "Refresh with header but no cookie should return 401, got: {:?}",
        body
    );
}

#[tokio::test]
async fn test_refresh_with_any_x_requested_with_value_passes_csrf() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // The header value doesn't matter — just its presence
    let (status, _, _) = refresh_request(&app, None, Some("XMLHttpRequest")).await;

    assert_ne!(
        status,
        StatusCode::FORBIDDEN,
        "Any X-Requested-With value should pass CSRF check"
    );
}

#[tokio::test]
async fn test_login_sets_refresh_cookie_with_correct_attributes() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let resp = app
        .raw_request(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({
                "username": "admin",
                "password": TestApp::TEST_PASSWORD
            })),
        )
        .await;

    assert_eq!(resp.status(), StatusCode::OK);

    let mut found_refresh_cookie = false;
    for value in resp.headers().get_all(header::SET_COOKIE).iter() {
        let cookie_str = value.to_str().unwrap_or("");
        if cookie_str.contains("anyserver_refresh=") {
            found_refresh_cookie = true;

            // Verify HttpOnly
            assert!(
                cookie_str.to_lowercase().contains("httponly"),
                "Refresh cookie should be HttpOnly. Got: {}",
                cookie_str
            );

            // Verify SameSite=Lax
            assert!(
                cookie_str.contains("SameSite=Lax"),
                "Refresh cookie should have SameSite=Lax. Got: {}",
                cookie_str
            );

            // Verify Path is scoped to /api/auth/refresh
            assert!(
                cookie_str.contains("Path=/api/auth/refresh"),
                "Refresh cookie Path should be /api/auth/refresh. Got: {}",
                cookie_str
            );

            // Verify Max-Age is set (7 days = 604800 seconds)
            assert!(
                cookie_str.contains("Max-Age="),
                "Refresh cookie should have Max-Age set. Got: {}",
                cookie_str
            );
        }
    }

    assert!(
        found_refresh_cookie,
        "Login response should set an anyserver_refresh cookie"
    );
}

#[tokio::test]
async fn test_logout_clears_cookie_with_matching_path() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let resp = app
        .raw_request("POST", "/api/auth/logout", Some(&token), Some(json!({})))
        .await;

    assert_eq!(resp.status(), StatusCode::OK);

    let mut found_clear_cookie = false;
    for value in resp.headers().get_all(header::SET_COOKIE).iter() {
        let cookie_str = value.to_str().unwrap_or("");
        if cookie_str.contains("anyserver_refresh=") {
            found_clear_cookie = true;

            // The clear cookie must use the same Path as the set cookie,
            // otherwise the browser won't clear it.
            assert!(
                cookie_str.contains("Path=/api/auth/refresh"),
                "Logout cookie Path should match the refresh endpoint path. Got: {}",
                cookie_str
            );

            // SameSite should also be set
            assert!(
                cookie_str.contains("SameSite=Lax"),
                "Logout cookie should have SameSite=Lax. Got: {}",
                cookie_str
            );
        }
    }

    assert!(
        found_clear_cookie,
        "Logout response should clear the anyserver_refresh cookie"
    );
}

#[tokio::test]
async fn test_csrf_rejection_returns_json_error() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // The 403 response should be a JSON error, not a bare status
    let (status, body, _) = refresh_request(&app, None, None).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        body.get("error").is_some(),
        "403 response should be a JSON object with an 'error' field. Got: {:?}",
        body
    );
}
