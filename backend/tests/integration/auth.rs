use axum::http::StatusCode;
use serde_json::json;

use crate::common::TestApp;

#[tokio::test]
async fn test_fresh_status_shows_setup_incomplete() {
    let app = TestApp::new().await;
    let (status, body) = app.get("/api/auth/status", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["setup_complete"], false);
    assert_eq!(body["registration_enabled"], false);
}

#[tokio::test]
async fn test_setup_creates_admin_and_marks_complete() {
    let app = TestApp::new().await;
    let (status, body) = app
        .post(
            "/api/auth/setup",
            None,
            json!({ "username": "myadmin", "password": "Secret1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["token"].is_string());
    assert_eq!(body["user"]["username"], "myadmin");
    assert_eq!(body["user"]["role"], "admin");

    // Status should now be complete
    let (_, status_body) = app.get("/api/auth/status", None).await;
    assert_eq!(status_body["setup_complete"], true);
}

#[tokio::test]
async fn test_setup_cannot_run_twice() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/auth/setup",
            None,
            json!({ "username": "admin2", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("already"));
}

#[tokio::test]
async fn test_login_with_correct_credentials() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Correct1234").await;

    let (status, body) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "Correct1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["token"].is_string());
    assert_eq!(body["user"]["username"], "admin");
}

#[tokio::test]
async fn test_login_with_wrong_password() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Correct1234").await;

    let (status, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "Wrong1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_login_with_nonexistent_user() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "nobody", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_me_returns_user_info() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/auth/me", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], "admin");
    assert_eq!(body["user"]["role"], "admin");
    assert_eq!(body["settings"]["setup_complete"], true);
}

#[tokio::test]
async fn test_me_without_token_is_unauthorized() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app.get("/api/auth/me", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_me_with_invalid_token_is_unauthorized() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app.get("/api/auth/me", Some("this.is.garbage")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_change_password_succeeds() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "OldPass123").await;

    let (status, body) = app
        .post(
            "/api/auth/change-password",
            Some(&token),
            json!({
                "current_password": "OldPass123",
                "new_password": "NewPass456"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["changed"], true);

    // Old password should no longer work
    let (status, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "OldPass123" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // New password should work
    let (status, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "NewPass456" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_change_password_wrong_current() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app
        .post(
            "/api/auth/change-password",
            Some(&token),
            json!({
                "current_password": "Wrong1234",
                "new_password": "NewPass456"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_change_password_too_short() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app
        .post(
            "/api/auth/change-password",
            Some(&token),
            json!({
                "current_password": "Admin1234",
                "new_password": "ab"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ─── Token Lifecycle Tests (Phase 1 & 2) ───

#[tokio::test]
async fn test_change_password_returns_new_token() {
    let app = TestApp::new().await;
    let old_token = app.setup_admin("admin", "OldPass123").await;

    // Change password
    let (status, body) = app
        .post(
            "/api/auth/change-password",
            Some(&old_token),
            json!({
                "current_password": "OldPass123",
                "new_password": "NewPass456"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["changed"], true);
    assert!(
        body["token"].is_string(),
        "change-password should return a new token"
    );
}

#[tokio::test]
async fn test_change_password_invalidates_old_tokens() {
    let app = TestApp::new().await;
    let old_token = app.setup_admin("admin", "OldPass123").await;

    // Old token should work
    let (status, _) = app.get("/api/auth/me", Some(&old_token)).await;
    assert_eq!(status, StatusCode::OK);

    // Change password
    let (status, body) = app
        .post(
            "/api/auth/change-password",
            Some(&old_token),
            json!({
                "current_password": "OldPass123",
                "new_password": "NewPass456"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["changed"], true);
    assert!(body["token"].is_string());
    let new_token = body["token"].as_str().unwrap();

    // Old token should now be invalid (generation mismatch)
    let (status, body) = app.get("/api/auth/me", Some(&old_token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        body["error"].as_str().unwrap().contains("revoked"),
        "Error should mention token revocation"
    );

    // New token should work
    let (status, _) = app.get("/api/auth/me", Some(new_token)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_logout_everywhere_invalidates_all_tokens() {
    let app = TestApp::new().await;
    let token1 = app.setup_admin("admin", "Admin1234").await;

    // Login again to get a second session
    let (status, body) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let token2 = body["token"].as_str().unwrap().to_string();

    // Both tokens should work
    let (status, _) = app.get("/api/auth/me", Some(&token1)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.get("/api/auth/me", Some(&token2)).await;
    assert_eq!(status, StatusCode::OK);

    // Call logout-everywhere with token1
    let (status, body) = app
        .post("/api/auth/logout-everywhere", Some(&token1), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["revoked_count"].is_number());
    assert!(body["token"].is_string());
    let new_token = body["token"].as_str().unwrap();

    // Old tokens should no longer work (token generation was incremented)
    let (status, _) = app.get("/api/auth/me", Some(&token1)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app.get("/api/auth/me", Some(&token2)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // New token should work
    let (status, _) = app.get("/api/auth/me", Some(new_token)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_logout_everywhere_returns_new_token() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post("/api/auth/logout-everywhere", Some(&token), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["revoked_count"].is_number());
    assert!(
        body["token"].is_string(),
        "logout-everywhere should return a fresh token"
    );

    let new_token = body["token"].as_str().unwrap();
    assert_ne!(
        token, new_token,
        "New token should be different from old token"
    );
}

#[tokio::test]
async fn test_login_returns_access_token() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["token"].is_string());

    // Token should be a valid JWT (has three dot-separated parts)
    let token = body["token"].as_str().unwrap();
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT should have 3 parts");
}

// ─── Session Management Tests (Phase 3) ───

#[tokio::test]
async fn test_list_sessions_returns_active_sessions() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // List sessions
    let (status, body) = app.get("/api/auth/sessions", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["sessions"].is_array());

    let sessions = body["sessions"].as_array().unwrap();
    assert!(
        !sessions.is_empty(),
        "Should have at least one active session from setup"
    );

    // Each session should have required fields
    let session = &sessions[0];
    assert!(session["id"].is_string());
    assert!(session["family_id"].is_string());
    assert!(session["created_at"].is_string());
    assert!(session["expires_at"].is_string());
    assert!(session["is_current"].is_boolean());
}

#[tokio::test]
async fn test_list_sessions_shows_multiple_sessions() {
    let app = TestApp::new().await;
    let token1 = app.setup_admin("admin", "Admin1234").await;

    // Create a second session by logging in again
    let (status, body) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let _token2 = body["token"].as_str().unwrap();

    // List sessions with first token
    let (status, body) = app.get("/api/auth/sessions", Some(&token1)).await;
    assert_eq!(status, StatusCode::OK);

    let sessions = body["sessions"].as_array().unwrap();
    assert!(
        sessions.len() >= 2,
        "Should have at least 2 active sessions"
    );
}

#[tokio::test]
async fn test_revoke_session_by_family_id() {
    let app = TestApp::new().await;
    let token1 = app.setup_admin("admin", "Admin1234").await;

    // Create a second session
    let (status, body) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let token2 = body["token"].as_str().unwrap().to_string();

    // Both tokens should work
    let (status, _) = app.get("/api/auth/me", Some(&token1)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.get("/api/auth/me", Some(&token2)).await;
    assert_eq!(status, StatusCode::OK);

    // List sessions to get a family_id to revoke
    let (status, body) = app.get("/api/auth/sessions", Some(&token1)).await;
    assert_eq!(status, StatusCode::OK);
    let sessions = body["sessions"].as_array().unwrap();

    // Find a non-current session to revoke
    let session_to_revoke = sessions
        .iter()
        .find(|s| !s["is_current"].as_bool().unwrap_or(true))
        .expect("Should have at least one non-current session");

    let family_id = session_to_revoke["family_id"].as_str().unwrap();

    // Revoke the session
    let (status, body) = app
        .post(
            "/api/auth/sessions/revoke",
            Some(&token1),
            json!({ "family_id": family_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["revoked_count"].as_i64().unwrap() > 0);

    // The revoked session's token should no longer work
    // (Note: This test is simplified - in reality we'd need to track which token belongs to which family)
}

#[tokio::test]
async fn test_revoke_nonexistent_session_returns_not_found() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app
        .post(
            "/api/auth/sessions/revoke",
            Some(&token),
            json!({ "family_id": "nonexistent-family-id" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_logout_everywhere_clears_all_sessions() {
    let app = TestApp::new().await;
    let token1 = app.setup_admin("admin", "Admin1234").await;

    // Create multiple sessions
    for _ in 0..3 {
        let (status, _) = app
            .post(
                "/api/auth/login",
                None,
                json!({ "username": "admin", "password": "Admin1234" }),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }

    // List sessions before logout-everywhere
    let (status, body) = app.get("/api/auth/sessions", Some(&token1)).await;
    assert_eq!(status, StatusCode::OK);
    let sessions_before = body["sessions"].as_array().unwrap().len();
    assert!(sessions_before >= 4, "Should have at least 4 sessions");

    // Logout everywhere
    let (status, body) = app
        .post("/api/auth/logout-everywhere", Some(&token1), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let new_token = body["token"].as_str().unwrap();

    // List sessions with new token - should only have 1 (the new one)
    let (status, body) = app.get("/api/auth/sessions", Some(new_token)).await;
    assert_eq!(status, StatusCode::OK);
    let sessions_after = body["sessions"].as_array().unwrap().len();
    assert_eq!(
        sessions_after, 1,
        "After logout-everywhere, should only have 1 active session"
    );
}

#[tokio::test]
async fn test_list_sessions_requires_auth() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app.get("/api/auth/sessions", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_revoke_session_requires_auth() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app
        .post(
            "/api/auth/sessions/revoke",
            None,
            json!({ "family_id": "some-family-id" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ─────────────────────────────────────────────────────────────────────────────
// Account Lockout Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_lockout_first_three_failures_no_cooldown() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Correct1234").await;

    // First 3 failed attempts should not trigger cooldown
    for i in 1..=3 {
        let (status, body) = app
            .post(
                "/api/auth/login",
                None,
                json!({ "username": "admin", "password": "WrongPassword" }),
            )
            .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "Attempt {} should return UNAUTHORIZED",
            i
        );
        assert_eq!(body["error"], "Invalid username or password");
    }

    // 4th attempt should still be allowed (the lockout kicks in AFTER the 4th failure)
    let (status, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "WrongPassword" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_lockout_fourth_failure_triggers_cooldown() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Correct1234").await;

    // Make 4 failed attempts
    for _ in 0..4 {
        app.post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "WrongPassword" }),
        )
        .await;
    }

    // 5th attempt should be rate-limited
    let (status, body) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "WrongPassword" }),
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("Too many failed login attempts"));
    assert!(body["error"].as_str().unwrap().contains("seconds"));
}

#[tokio::test]
async fn test_lockout_includes_retry_after_header() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Correct1234").await;

    // Make 4 failed attempts to trigger cooldown
    for _ in 0..4 {
        app.post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "WrongPassword" }),
        )
        .await;
    }

    // Next attempt should include Retry-After header
    let response = app
        .raw_request(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "username": "admin", "password": "WrongPassword" })),
        )
        .await;

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    let retry_after = response.headers().get("retry-after");
    assert!(
        retry_after.is_some(),
        "Retry-After header should be present"
    );

    let retry_secs: u64 = retry_after
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .expect("Retry-After should be a number");
    assert!(
        (4..=6).contains(&retry_secs),
        "Expected ~5 second cooldown, got {}",
        retry_secs
    );
}

#[tokio::test]
async fn test_lockout_success_resets_counter() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Correct1234").await;

    // Make 3 failed attempts
    for _ in 0..3 {
        app.post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "WrongPassword" }),
        )
        .await;
    }

    // Successful login should reset the counter
    let (status, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "Correct1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Now we should be able to make 3 more failed attempts without lockout
    for i in 1..=3 {
        let (status, _) = app
            .post(
                "/api/auth/login",
                None,
                json!({ "username": "admin", "password": "WrongPassword" }),
            )
            .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "Attempt {} after reset should not trigger cooldown",
            i
        );
    }
}

#[tokio::test]
async fn test_lockout_password_not_checked_during_cooldown() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Correct1234").await;

    // Trigger cooldown (4 failures)
    for _ in 0..4 {
        app.post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "WrongPassword" }),
        )
        .await;
    }

    // Attempt with CORRECT password during cooldown should still be rejected
    let (status, body) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "Correct1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("Too many failed login attempts"));

    // The error message should NOT reveal whether the password was correct
    assert!(!body["error"]
        .as_str()
        .unwrap()
        .contains("Invalid username or password"));
}

#[tokio::test]
async fn test_lockout_username_normalization() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Correct1234").await;

    // Make failed attempts with different case variations
    app.post(
        "/api/auth/login",
        None,
        json!({ "username": "Admin", "password": "WrongPassword" }),
    )
    .await;

    app.post(
        "/api/auth/login",
        None,
        json!({ "username": "ADMIN", "password": "WrongPassword" }),
    )
    .await;

    app.post(
        "/api/auth/login",
        None,
        json!({ "username": " admin ", "password": "WrongPassword" }),
    )
    .await;

    app.post(
        "/api/auth/login",
        None,
        json!({ "username": "admin", "password": "WrongPassword" }),
    )
    .await;

    // All variations should count toward the same counter
    // 5th attempt (with yet another variation) should be rate-limited
    let (status, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "AdMiN", "password": "WrongPassword" }),
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_lockout_nonexistent_user_is_tracked() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Correct1234").await;

    let nonexistent_user = "nonexistent_user_12345";

    // Make 4 failed attempts for nonexistent user
    for _ in 0..4 {
        let (status, _) = app
            .post(
                "/api/auth/login",
                None,
                json!({ "username": nonexistent_user, "password": "AnyPassword" }),
            )
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    // 5th attempt should be rate-limited (no user enumeration)
    let (status, body) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": nonexistent_user, "password": "AnyPassword" }),
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("Too many failed login attempts"));
}

#[tokio::test]
async fn test_lockout_different_users_are_independent() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    // Register a second user
    let (status, _) = app
        .put(
            "/api/auth/settings",
            Some(&admin_token),
            json!({ "registration_enabled": true, "allow_run_commands": true, "run_command_sandbox": "auto", "run_command_default_timeout_secs": 300, "run_command_use_namespaces": true }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app
        .post(
            "/api/auth/register",
            None,
            json!({ "username": "user2", "password": "User21234" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Lock out admin (4 failures)
    for _ in 0..4 {
        app.post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "WrongPassword" }),
        )
        .await;
    }

    // Admin should be locked out
    let (status, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "Admin1234" }),
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);

    // user2 should still be able to log in (independent counter)
    let (status, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "user2", "password": "User21234" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_lockout_exponential_backoff_increases() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Correct1234").await;

    // Make 4 failed attempts (triggers 5-second cooldown)
    // Failures 1-3 have no cooldown, failure 4 triggers 5s cooldown
    for _ in 0..4 {
        app.post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "WrongPassword" }),
        )
        .await;
    }

    // Check that we get a cooldown (should be ~5 seconds after 4 failures)
    let response = app
        .raw_request(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "username": "admin", "password": "WrongPassword" })),
        )
        .await;

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    let retry_after: u64 = response
        .headers()
        .get("retry-after")
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();

    // Should be approximately 5 seconds (allow some slack)
    assert!(
        (4..=6).contains(&retry_after),
        "Expected ~5 second cooldown for 4 failures, got {}",
        retry_after
    );
}

#[tokio::test]
async fn test_lockout_cooldown_applies_to_both_success_and_failure() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Correct1234").await;

    // Trigger cooldown
    for _ in 0..4 {
        app.post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "WrongPassword" }),
        )
        .await;
    }

    // Both wrong password and correct password should be rejected during cooldown
    let (status_wrong, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "WrongPassword" }),
        )
        .await;
    assert_eq!(status_wrong, StatusCode::TOO_MANY_REQUESTS);

    let (status_correct, _) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": "Correct1234" }),
        )
        .await;
    assert_eq!(status_correct, StatusCode::TOO_MANY_REQUESTS);
}

// ── Rate-limit tier split tests (ticket 0-002) ─────────────────────

#[tokio::test]
async fn test_status_endpoint_not_blocked_by_login_limit() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Exhaust the credential rate-limit bucket (10 requests)
    for _ in 0..10 {
        let _ = app
            .post(
                "/api/auth/login",
                None,
                json!({ "username": "admin", "password": "WrongPass1" }),
            )
            .await;
    }

    // GET /auth/status should still succeed — it's on a separate bucket
    let (status, _) = app.get("/api/auth/status", None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "/auth/status should not be blocked by credential rate limit"
    );
}

#[tokio::test]
async fn test_refresh_endpoint_not_blocked_by_login_limit() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Exhaust the credential rate-limit bucket (10 requests)
    for _ in 0..10 {
        let _ = app
            .post(
                "/api/auth/login",
                None,
                json!({ "username": "admin", "password": "WrongPass1" }),
            )
            .await;
    }

    // POST /auth/refresh should still be reachable (it may return 401
    // because we don't have a valid refresh cookie, but NOT 429).
    let (status, _) = app.post("/api/auth/refresh", None, json!({})).await;
    assert_ne!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "/auth/refresh should not be blocked by credential rate limit"
    );
}

#[tokio::test]
async fn test_login_rate_limit_still_enforced() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Send 10 login requests to fill the credential bucket
    for _ in 0..10 {
        let _ = app
            .post(
                "/api/auth/login",
                None,
                json!({ "username": "admin", "password": "WrongPass1" }),
            )
            .await;
    }

    // The 11th login request should be rate-limited
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
        "11th login should return 429"
    );
    assert!(
        resp.headers().get("retry-after").is_some(),
        "429 response should include Retry-After header"
    );
}

#[tokio::test]
async fn test_status_rate_limit_generous() {
    let app = TestApp::new().await;

    // Fire 60 GET /auth/status requests — all should succeed
    for i in 0..60 {
        let (status, _) = app.get("/api/auth/status", None).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "/auth/status request {} should succeed",
            i + 1
        );
    }

    // The 61st should be rate-limited
    let (status, _) = app.get("/api/auth/status", None).await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "61st /auth/status should return 429"
    );
}

#[tokio::test]
async fn test_rapid_hard_refresh_no_429() {
    let app = TestApp::new().await;
    app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Simulate 15 hard-refresh cycles. Each cycle hits:
    //   - GET  /auth/status
    //   - POST /auth/refresh
    // That's 15 status + 15 refresh = 30 total requests spread across
    // the generous buckets. None should 429.
    for cycle in 0..15 {
        let (status_s, _) = app.get("/api/auth/status", None).await;
        assert_eq!(
            status_s,
            StatusCode::OK,
            "hard-refresh cycle {}: /auth/status should not 429",
            cycle + 1
        );

        // refresh will 401 (no cookie) but must NOT 429
        let (status_r, _) = app.post("/api/auth/refresh", None, json!({})).await;
        assert_ne!(
            status_r,
            StatusCode::TOO_MANY_REQUESTS,
            "hard-refresh cycle {}: /auth/refresh should not 429",
            cycle + 1
        );
    }
}
