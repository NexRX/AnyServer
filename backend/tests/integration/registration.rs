use axum::http::StatusCode;
use serde_json::json;

use crate::common::TestApp;

#[tokio::test]
async fn test_register_disabled_by_default() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/auth/register",
            None,
            json!({ "username": "newuser", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body["error"].as_str().unwrap().contains("disabled"));
}

#[tokio::test]
async fn test_register_when_enabled() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    app.enable_registration(&admin_token).await;

    let (status, body) = app
        .post(
            "/api/auth/register",
            None,
            json!({ "username": "newuser", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["token"].is_string());
    assert_eq!(body["user"]["username"], "newuser");
    assert_eq!(body["user"]["role"], "user"); // not admin
}

#[tokio::test]
async fn test_register_duplicate_username() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    app.enable_registration(&admin_token).await;

    app.register_user("taken", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/auth/register",
            None,
            json!({ "username": "taken", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("taken"));
}

#[tokio::test]
async fn test_register_before_setup() {
    let app = TestApp::new().await;

    let (status, body) = app
        .post(
            "/api/auth/register",
            None,
            json!({ "username": "newuser", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("setup"));
}

#[tokio::test]
async fn test_register_invalid_username_too_short() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    app.enable_registration(&admin_token).await;

    let (status, _) = app
        .post(
            "/api/auth/register",
            None,
            json!({ "username": "ab", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_register_invalid_username_special_chars() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    app.enable_registration(&admin_token).await;

    let (status, _) = app
        .post(
            "/api/auth/register",
            None,
            json!({ "username": "user@name!", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
