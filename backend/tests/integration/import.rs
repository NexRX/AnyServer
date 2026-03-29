use axum::http::StatusCode;
use serde_json::json;

use crate::common::TestApp;

#[tokio::test]
async fn test_import_url_requires_auth() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app
        .post(
            "/api/import/url",
            None,
            json!({ "url": "https://example.com/config.json" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_import_url_empty_url() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post("/api/import/url", Some(&token), json!({ "url": "" }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("required"));
}

#[tokio::test]
async fn test_import_url_invalid_protocol() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/import/url",
            Some(&token),
            json!({ "url": "ftp://evil.com/config.json" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("http"));
}

#[tokio::test]
async fn test_import_folder_requires_auth() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app
        .post(
            "/api/import/folder",
            None,
            json!({ "url": "https://github.com/user/repo/tree/main/configs" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_import_folder_empty_url() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post("/api/import/folder", Some(&token), json!({ "url": "" }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("required"));
}
