use axum::http::StatusCode;
use serde_json::json;

use crate::common::TestApp;

#[tokio::test]
async fn test_admin_list_users() {
    let app = TestApp::new().await;
    let (admin_token, _, _) = app.setup_admin_and_user().await;

    let (status, body) = app.get("/api/admin/users", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["users"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_non_admin_cannot_list_users() {
    let app = TestApp::new().await;
    let (_, user_token, _) = app.setup_admin_and_user().await;

    let (status, _) = app.get("/api/admin/users", Some(&user_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_unauthenticated_cannot_list_users() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app.get("/api/admin/users", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_get_single_user() {
    let app = TestApp::new().await;
    let (admin_token, _, user_id) = app.setup_admin_and_user().await;

    let (status, body) = app
        .get(&format!("/api/admin/users/{}", user_id), Some(&admin_token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["username"], "regularuser");
}

#[tokio::test]
async fn test_non_admin_cannot_get_user() {
    let app = TestApp::new().await;
    let (_, user_token, user_id) = app.setup_admin_and_user().await;

    let (status, _) = app
        .get(&format!("/api/admin/users/{}", user_id), Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_admin_update_user_role_to_admin() {
    let app = TestApp::new().await;
    let (admin_token, _, user_id) = app.setup_admin_and_user().await;

    let (status, body) = app
        .put(
            &format!("/api/admin/users/{}/role", user_id),
            Some(&admin_token),
            json!({ "role": "admin" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["role"], "admin");
}

#[tokio::test]
async fn test_admin_cannot_demote_self() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    // Get admin's own user ID
    let (_, me) = app.get("/api/auth/me", Some(&admin_token)).await;
    let admin_id = me["user"]["id"].as_str().unwrap();

    let (status, body) = app
        .put(
            &format!("/api/admin/users/{}/role", admin_id),
            Some(&admin_token),
            json!({ "role": "user" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("demote"));
}

#[tokio::test]
async fn test_admin_delete_user() {
    let app = TestApp::new().await;
    let (admin_token, _, user_id) = app.setup_admin_and_user().await;

    let (status, body) = app
        .delete(&format!("/api/admin/users/{}", user_id), Some(&admin_token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted"], true);

    // Verify user is gone
    let (status, _) = app
        .get(&format!("/api/admin/users/{}", user_id), Some(&admin_token))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_cannot_delete_self() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    let (_, me) = app.get("/api/auth/me", Some(&admin_token)).await;
    let admin_id = me["user"]["id"].as_str().unwrap();

    let (status, _) = app
        .delete(
            &format!("/api/admin/users/{}", admin_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_cannot_delete_last_admin() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    // Create another user (not admin)
    app.enable_registration(&admin_token).await;
    let user_token = app.register_user("regular", "Admin1234").await;
    let (_, user_me) = app.get("/api/auth/me", Some(&user_token)).await;
    let _user_id = user_me["user"]["id"].as_str().unwrap();

    // The admin is the only admin — get their ID
    let (_, admin_me) = app.get("/api/auth/me", Some(&admin_token)).await;
    let admin_id = admin_me["user"]["id"].as_str().unwrap();

    // Another admin tries to delete the only admin — but there's only one admin.
    // First promote the regular user to admin so we have a second admin who can try.
    let (_, user_me_body) = app.get("/api/auth/me", Some(&user_token)).await;
    let user_id = user_me_body["user"]["id"].as_str().unwrap();

    app.put(
        &format!("/api/admin/users/{}/role", user_id),
        Some(&admin_token),
        json!({ "role": "admin" }),
    )
    .await;

    // Now both are admins. Delete the second admin from the first.
    let (status, _) = app
        .delete(&format!("/api/admin/users/{}", user_id), Some(&admin_token))
        .await;
    assert_eq!(status, StatusCode::OK);

    // Now there's only one admin left — they can't be deleted by themselves
    let (status, _) = app
        .delete(
            &format!("/api/admin/users/{}", admin_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_non_admin_cannot_delete_users() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _) = app.setup_admin_and_user().await;

    let (_, admin_me) = app.get("/api/auth/me", Some(&admin_token)).await;
    let admin_id = admin_me["user"]["id"].as_str().unwrap();

    let (status, _) = app
        .delete(&format!("/api/admin/users/{}", admin_id), Some(&user_token))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_non_admin_cannot_update_roles() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _) = app.setup_admin_and_user().await;

    let (_, admin_me) = app.get("/api/auth/me", Some(&admin_token)).await;
    let admin_id = admin_me["user"]["id"].as_str().unwrap();

    let (status, _) = app
        .put(
            &format!("/api/admin/users/{}/role", admin_id),
            Some(&user_token),
            json!({ "role": "user" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
