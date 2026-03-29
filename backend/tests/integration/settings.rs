use axum::http::StatusCode;
use serde_json::json;

use crate::common::TestApp;

#[tokio::test]
async fn test_admin_update_settings() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    // Enable registration
    let (status, body) = app
        .put(
            "/api/auth/settings",
            Some(&admin_token),
            json!({
                "registration_enabled": true,
                "allow_run_commands": false,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["registration_enabled"], true);

    // Verify via status
    let (_, status_body) = app.get("/api/auth/status", None).await;
    assert_eq!(status_body["registration_enabled"], true);

    // Disable again
    let (status, body) = app
        .put(
            "/api/auth/settings",
            Some(&admin_token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": false,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["registration_enabled"], false);
}

#[tokio::test]
async fn test_non_admin_cannot_update_settings() {
    let app = TestApp::new().await;
    let (_, user_token, _) = app.setup_admin_and_user().await;

    let (status, _) = app
        .put(
            "/api/auth/settings",
            Some(&user_token),
            json!({
                "registration_enabled": true,
                "allow_run_commands": false,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_unauthenticated_cannot_update_settings() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app
        .put(
            "/api/auth/settings",
            None,
            json!({
                "registration_enabled": true,
                "allow_run_commands": false,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_update_allow_run_commands() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    // Check default (should be false for new installations)
    let (_, status_body) = app.get("/api/auth/status", None).await;
    assert_eq!(status_body["allow_run_commands"], false);

    // Enable RunCommand
    let (status, body) = app
        .put(
            "/api/auth/settings",
            Some(&admin_token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": true,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["allow_run_commands"], true);

    // Verify via status
    let (_, status_body) = app.get("/api/auth/status", None).await;
    assert_eq!(status_body["allow_run_commands"], true);

    // Disable again
    let (status, body) = app
        .put(
            "/api/auth/settings",
            Some(&admin_token),
            json!({
                "registration_enabled": false,
                "allow_run_commands": false,
                "run_command_sandbox": "auto",
                "run_command_default_timeout_secs": 300,
                "run_command_use_namespaces": true
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["allow_run_commands"], false);

    // Verify via status
    let (_, status_body) = app.get("/api/auth/status", None).await;
    assert_eq!(status_body["allow_run_commands"], false);
}
