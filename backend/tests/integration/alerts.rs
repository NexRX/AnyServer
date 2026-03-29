use axum::http::StatusCode;
use serde_json::json;

use crate::common::TestApp;

// ═══════════════════════════════════════════════════════════════════════
//  SMTP Configuration
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_get_smtp_config_returns_null_when_not_configured() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/admin/smtp", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_null(), "Expected null when SMTP is not configured");
}

#[tokio::test]
async fn test_save_and_get_smtp_config() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .put(
            "/api/admin/smtp",
            Some(&admin_token),
            json!({
                "host": "smtp.example.com",
                "port": 587,
                "tls": true,
                "username": "alerts@example.com",
                "password": "supersecret",
                "from_address": "AnyServer <alerts@example.com>"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["host"], "smtp.example.com");
    assert_eq!(body["port"], 587);
    assert_eq!(body["tls"], true);
    assert_eq!(body["username"], "alerts@example.com");
    assert_eq!(body["from_address"], "AnyServer <alerts@example.com>");
    // Password must NEVER be returned
    assert!(
        body.get("password").is_none(),
        "Password must not be returned"
    );
    assert_eq!(body["password_set"], true);

    // GET should return the same (without password)
    let (status, body) = app.get("/api/admin/smtp", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["host"], "smtp.example.com");
    assert_eq!(body["port"], 587);
    assert_eq!(body["password_set"], true);
}

#[tokio::test]
async fn test_save_smtp_config_keeps_existing_password_when_null() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    // Set initial config with a password
    let (status, _) = app
        .put(
            "/api/admin/smtp",
            Some(&admin_token),
            json!({
                "host": "smtp.example.com",
                "port": 587,
                "tls": true,
                "username": "alerts@example.com",
                "password": "supersecret",
                "from_address": "alerts@example.com"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Update without providing password (null/omitted) — should keep existing
    let (status, body) = app
        .put(
            "/api/admin/smtp",
            Some(&admin_token),
            json!({
                "host": "new-smtp.example.com",
                "port": 465,
                "tls": true,
                "username": "alerts@example.com",
                "from_address": "alerts@example.com"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["host"], "new-smtp.example.com");
    assert_eq!(body["port"], 465);
    assert_eq!(
        body["password_set"], true,
        "Existing password should be preserved"
    );
}

#[tokio::test]
async fn test_save_smtp_config_empty_host_rejected() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .put(
            "/api/admin/smtp",
            Some(&admin_token),
            json!({
                "host": "",
                "port": 587,
                "tls": true,
                "username": "user",
                "password": "pass",
                "from_address": "test@example.com"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"].as_str().unwrap().contains("host"),
        "Error should mention host"
    );
}

#[tokio::test]
async fn test_save_smtp_config_empty_from_address_rejected() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .put(
            "/api/admin/smtp",
            Some(&admin_token),
            json!({
                "host": "smtp.example.com",
                "port": 587,
                "tls": true,
                "username": "user",
                "password": "pass",
                "from_address": ""
            }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"].as_str().unwrap().contains("From address"),
        "Error should mention from address"
    );
}

#[tokio::test]
async fn test_delete_smtp_config() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    // Set config first
    let (status, _) = app
        .put(
            "/api/admin/smtp",
            Some(&admin_token),
            json!({
                "host": "smtp.example.com",
                "port": 587,
                "tls": true,
                "username": "user",
                "password": "pass",
                "from_address": "test@example.com"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Delete it
    let (status, body) = app.delete("/api/admin/smtp", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted"], true);

    // Should be null now
    let (status, body) = app.get("/api/admin/smtp", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_null());
}

#[tokio::test]
async fn test_smtp_endpoints_require_admin() {
    let app = TestApp::new().await;
    let (_, user_token, _) = app.setup_admin_and_user().await;

    // GET
    let (status, _) = app.get("/api/admin/smtp", Some(&user_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // PUT
    let (status, _) = app
        .put(
            "/api/admin/smtp",
            Some(&user_token),
            json!({
                "host": "smtp.example.com",
                "port": 587,
                "tls": true,
                "username": "user",
                "password": "pass",
                "from_address": "test@example.com"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // DELETE
    let (status, _) = app.delete("/api/admin/smtp", Some(&user_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_smtp_endpoints_require_auth() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app.get("/api/admin/smtp", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = app
        .put(
            "/api/admin/smtp",
            None,
            json!({
                "host": "smtp.example.com",
                "port": 587,
                "tls": true,
                "username": "user",
                "password": "pass",
                "from_address": "test@example.com"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = app.delete("/api/admin/smtp", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_send_test_email_without_smtp_config() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/admin/smtp/test",
            Some(&admin_token),
            json!({ "recipient": "test@example.com" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"].as_str().unwrap().contains("not configured"),
        "Should indicate SMTP is not configured"
    );
}

#[tokio::test]
async fn test_send_test_email_empty_recipient_rejected() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    // Configure SMTP first (so we get past that check)
    app.put(
        "/api/admin/smtp",
        Some(&admin_token),
        json!({
            "host": "smtp.example.com",
            "port": 587,
            "tls": true,
            "username": "user",
            "password": "pass",
            "from_address": "test@example.com"
        }),
    )
    .await;

    let (status, body) = app
        .post(
            "/api/admin/smtp/test",
            Some(&admin_token),
            json!({ "recipient": "" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"].as_str().unwrap().contains("Recipient"),
        "Should indicate recipient is required"
    );
}

#[tokio::test]
async fn test_send_test_email_non_admin_forbidden() {
    let app = TestApp::new().await;
    let (_, user_token, _) = app.setup_admin_and_user().await;

    let (status, _) = app
        .post(
            "/api/admin/smtp/test",
            Some(&user_token),
            json!({ "recipient": "test@example.com" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ═══════════════════════════════════════════════════════════════════════
//  Alert Configuration
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_get_alert_config_returns_defaults() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/admin/alerts", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);

    // Default values
    assert_eq!(body["enabled"], false);
    assert_eq!(body["recipients"].as_array().unwrap().len(), 0);
    assert_eq!(body["cooldown_secs"], 300);

    // Default triggers
    assert_eq!(body["triggers"]["server_crashed"], true);
    assert_eq!(body["triggers"]["restart_exhausted"], true);
    assert_eq!(body["triggers"]["server_down"], false);
    assert_eq!(body["triggers"]["high_memory"], false);
    assert_eq!(body["triggers"]["high_cpu"], false);
    assert_eq!(body["triggers"]["low_disk"], false);
}

#[tokio::test]
async fn test_save_and_get_alert_config() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .put(
            "/api/admin/alerts",
            Some(&admin_token),
            json!({
                "enabled": true,
                "recipients": ["admin@example.com", "ops@example.com"],
                "base_url": "https://my.server.com:3001",
                "cooldown_secs": 600,
                "triggers": {
                    "server_crashed": true,
                    "restart_exhausted": true,
                    "server_down": true,
                    "down_threshold_mins": 5,
                    "high_memory": true,
                    "memory_threshold_percent": 85.0,
                    "high_cpu": true,
                    "cpu_threshold_percent": 90.0,
                    "low_disk": true,
                    "disk_threshold_mb": 2048
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], true);
    assert_eq!(body["recipients"].as_array().unwrap().len(), 2);
    assert_eq!(body["base_url"], "https://my.server.com:3001");
    assert_eq!(body["cooldown_secs"], 600);
    assert_eq!(body["triggers"]["server_down"], true);
    assert_eq!(body["triggers"]["down_threshold_mins"], 5);
    assert_eq!(body["triggers"]["high_memory"], true);
    assert_eq!(body["triggers"]["memory_threshold_percent"], 85.0);
    assert_eq!(body["triggers"]["high_cpu"], true);
    assert_eq!(body["triggers"]["cpu_threshold_percent"], 90.0);
    assert_eq!(body["triggers"]["low_disk"], true);
    assert_eq!(body["triggers"]["disk_threshold_mb"], 2048);

    // Verify persistence with GET
    let (status, body) = app.get("/api/admin/alerts", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], true);
    assert_eq!(body["recipients"].as_array().unwrap().len(), 2);
    assert_eq!(body["recipients"][0], "admin@example.com");
    assert_eq!(body["recipients"][1], "ops@example.com");
    assert_eq!(body["triggers"]["disk_threshold_mb"], 2048);
}

#[tokio::test]
async fn test_save_alert_config_trims_recipients() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .put(
            "/api/admin/alerts",
            Some(&admin_token),
            json!({
                "enabled": true,
                "recipients": ["  admin@example.com  ", "  ", "ops@example.com"],
                "cooldown_secs": 300,
                "triggers": {
                    "server_crashed": true,
                    "restart_exhausted": true,
                    "server_down": false,
                    "down_threshold_mins": 10,
                    "high_memory": false,
                    "memory_threshold_percent": 90.0,
                    "high_cpu": false,
                    "cpu_threshold_percent": 95.0,
                    "low_disk": false,
                    "disk_threshold_mb": 1024
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Empty-after-trim recipients should be filtered out
    let recipients = body["recipients"].as_array().unwrap();
    assert_eq!(recipients.len(), 2);
    assert_eq!(recipients[0], "admin@example.com");
    assert_eq!(recipients[1], "ops@example.com");
}

#[tokio::test]
async fn test_save_alert_config_empty_base_url_becomes_null() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .put(
            "/api/admin/alerts",
            Some(&admin_token),
            json!({
                "enabled": false,
                "recipients": [],
                "base_url": "   ",
                "cooldown_secs": 300,
                "triggers": {
                    "server_crashed": true,
                    "restart_exhausted": true,
                    "server_down": false,
                    "down_threshold_mins": 10,
                    "high_memory": false,
                    "memory_threshold_percent": 90.0,
                    "high_cpu": false,
                    "cpu_threshold_percent": 95.0,
                    "low_disk": false,
                    "disk_threshold_mb": 1024
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["base_url"].is_null());
}

#[tokio::test]
async fn test_alert_config_endpoints_require_admin() {
    let app = TestApp::new().await;
    let (_, user_token, _) = app.setup_admin_and_user().await;

    // GET
    let (status, _) = app.get("/api/admin/alerts", Some(&user_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // PUT
    let (status, _) = app
        .put(
            "/api/admin/alerts",
            Some(&user_token),
            json!({
                "enabled": true,
                "recipients": ["test@example.com"],
                "cooldown_secs": 60,
                "triggers": {
                    "server_crashed": true,
                    "restart_exhausted": true,
                    "server_down": false,
                    "down_threshold_mins": 10,
                    "high_memory": false,
                    "memory_threshold_percent": 90.0,
                    "high_cpu": false,
                    "cpu_threshold_percent": 95.0,
                    "low_disk": false,
                    "disk_threshold_mb": 1024
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_alert_config_endpoints_require_auth() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app.get("/api/admin/alerts", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = app
        .put(
            "/api/admin/alerts",
            None,
            json!({
                "enabled": true,
                "recipients": [],
                "cooldown_secs": 300,
                "triggers": {
                    "server_crashed": true,
                    "restart_exhausted": true,
                    "server_down": false,
                    "down_threshold_mins": 10,
                    "high_memory": false,
                    "memory_threshold_percent": 90.0,
                    "high_cpu": false,
                    "cpu_threshold_percent": 95.0,
                    "low_disk": false,
                    "disk_threshold_mb": 1024
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ═══════════════════════════════════════════════════════════════════════
//  Per-Server Alert Settings
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_get_server_alerts_default_not_muted() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&admin_token, "Test Server").await;

    let (status, body) = app
        .get(
            &format!("/api/servers/{}/alerts", server_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["server_id"], server_id);
    assert_eq!(body["muted"], false);
}

#[tokio::test]
async fn test_mute_and_unmute_server_alerts() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&admin_token, "Test Server").await;

    // Mute
    let (status, body) = app
        .put(
            &format!("/api/servers/{}/alerts", server_id),
            Some(&admin_token),
            json!({ "muted": true }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["muted"], true);

    // Verify with GET
    let (status, body) = app
        .get(
            &format!("/api/servers/{}/alerts", server_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["muted"], true);

    // Unmute
    let (status, body) = app
        .put(
            &format!("/api/servers/{}/alerts", server_id),
            Some(&admin_token),
            json!({ "muted": false }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["muted"], false);
}

#[tokio::test]
async fn test_server_alerts_nonexistent_server() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    let fake_id = "00000000-0000-0000-0000-000000000000";

    let (status, _) = app
        .get(
            &format!("/api/servers/{}/alerts", fake_id),
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = app
        .put(
            &format!("/api/servers/{}/alerts", fake_id),
            Some(&admin_token),
            json!({ "muted": true }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_server_alerts_require_auth() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&admin_token, "Test Server").await;

    let (status, _) = app
        .get(&format!("/api/servers/{}/alerts", server_id), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = app
        .put(
            &format!("/api/servers/{}/alerts", server_id),
            None,
            json!({ "muted": true }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_server_alerts_viewer_can_read_but_not_write() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Test Server").await;

    // Grant viewer access
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
            json!({ "user_id": user_id, "level": "viewer" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Viewer CAN read alert settings
    let (status, body) = app
        .get(
            &format!("/api/servers/{}/alerts", server_id),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["muted"], false);

    // Viewer CANNOT write alert settings (requires manager level)
    let (status, _) = app
        .put(
            &format!("/api/servers/{}/alerts", server_id),
            Some(&user_token),
            json!({ "muted": true }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_server_alerts_manager_can_write() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Test Server").await;

    // Grant manager access
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/permissions", server_id),
            Some(&admin_token),
            json!({ "user_id": user_id, "level": "manager" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Manager CAN mute alerts
    let (status, body) = app
        .put(
            &format!("/api/servers/{}/alerts", server_id),
            Some(&user_token),
            json!({ "muted": true }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["muted"], true);
}

#[tokio::test]
async fn test_server_alerts_no_access_forbidden() {
    let app = TestApp::new().await;
    let (admin_token, user_token, _user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Test Server").await;

    // User has NO permissions on this server
    let (status, _) = app
        .get(
            &format!("/api/servers/{}/alerts", server_id),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = app
        .put(
            &format!("/api/servers/{}/alerts", server_id),
            Some(&user_token),
            json!({ "muted": true }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ═══════════════════════════════════════════════════════════════════════
//  Per-server alert settings are independent across servers
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_per_server_alert_independence() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    let (server_a, _) = app.create_test_server(&admin_token, "Server A").await;
    let (server_b, _) = app.create_test_server(&admin_token, "Server B").await;

    // Mute server A
    let (status, _) = app
        .put(
            &format!("/api/servers/{}/alerts", server_a),
            Some(&admin_token),
            json!({ "muted": true }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Server B should still be unmuted
    let (_, body_b) = app
        .get(
            &format!("/api/servers/{}/alerts", server_b),
            Some(&admin_token),
        )
        .await;
    assert_eq!(body_b["muted"], false);

    // Server A should be muted
    let (_, body_a) = app
        .get(
            &format!("/api/servers/{}/alerts", server_a),
            Some(&admin_token),
        )
        .await;
    assert_eq!(body_a["muted"], true);
}

// ═══════════════════════════════════════════════════════════════════════
//  Alert config update is idempotent
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_alert_config_update_is_idempotent() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    let config = json!({
        "enabled": true,
        "recipients": ["admin@example.com"],
        "base_url": "https://example.com",
        "cooldown_secs": 120,
        "triggers": {
            "server_crashed": false,
            "restart_exhausted": false,
            "server_down": true,
            "down_threshold_mins": 15,
            "high_memory": true,
            "memory_threshold_percent": 80.0,
            "high_cpu": false,
            "cpu_threshold_percent": 95.0,
            "low_disk": true,
            "disk_threshold_mb": 512
        }
    });

    // Apply twice
    let (s1, b1) = app
        .put("/api/admin/alerts", Some(&admin_token), config.clone())
        .await;
    let (s2, b2) = app
        .put("/api/admin/alerts", Some(&admin_token), config.clone())
        .await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(b1, b2);
}

// ═══════════════════════════════════════════════════════════════════════
//  SMTP password can be cleared by passing empty string
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_smtp_password_can_be_cleared() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;

    // Set config with password
    let (status, body) = app
        .put(
            "/api/admin/smtp",
            Some(&admin_token),
            json!({
                "host": "smtp.example.com",
                "port": 587,
                "tls": true,
                "username": "user",
                "password": "secret",
                "from_address": "test@example.com"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["password_set"], true);

    // Clear password by passing empty string
    let (status, body) = app
        .put(
            "/api/admin/smtp",
            Some(&admin_token),
            json!({
                "host": "smtp.example.com",
                "port": 587,
                "tls": true,
                "username": "user",
                "password": "",
                "from_address": "test@example.com"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["password_set"], false);
}
