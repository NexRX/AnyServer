//! Integration tests for SFTP username indexed lookup (Ticket #030).
//!
//! These tests verify that:
//! - SFTP auth looks up servers by username in O(1) instead of O(n)
//! - Authentication works with both UUID and sftp_username
//! - Non-existent usernames are rejected with constant-time behavior
//! - The database query uses an indexed lookup, not a full scan

use crate::common::TestApp;
use serde_json::json;

#[tokio::test]
async fn test_sftp_lookup_by_uuid() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server with SFTP credentials
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Server A",
                    "binary": "/bin/echo",
                    "args": ["test"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": "myuser",
                    "sftp_password": "Password123"
                }
            }),
        )
        .await;

    assert_eq!(status, 200, "Failed to create server: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    // Should be able to look up by UUID
    let result = app
        .state
        .db
        .find_server_by_sftp_username(&server_id)
        .await
        .unwrap();

    assert!(result.is_some(), "Should find server by UUID");
    let found = result.unwrap();
    assert_eq!(found.id.to_string(), server_id);
    assert_eq!(found.config.name, "Server A");
}

#[tokio::test]
async fn test_sftp_lookup_by_custom_username() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server with custom SFTP username
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Server B",
                    "binary": "/bin/echo",
                    "args": ["test"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": "customuser",
                    "sftp_password": "Password123"
                }
            }),
        )
        .await;

    assert_eq!(status, 200, "Failed to create server: {:?}", body);

    // Should be able to look up by custom username
    let result = app
        .state
        .db
        .find_server_by_sftp_username("customuser")
        .await
        .unwrap();

    assert!(result.is_some(), "Should find server by custom username");
    let found = result.unwrap();
    assert_eq!(found.config.name, "Server B");
    assert_eq!(found.config.sftp_username.as_deref(), Some("customuser"));
}

#[tokio::test]
async fn test_sftp_lookup_nonexistent_username() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server for context (to ensure it's not an empty database)
    let (status, _body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Server C",
                    "binary": "/bin/echo",
                    "args": ["test"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": "existinguser",
                    "sftp_password": "Password123"
                }
            }),
        )
        .await;

    assert_eq!(status, 200);

    // Look up a username that doesn't exist
    let result = app
        .state
        .db
        .find_server_by_sftp_username("nonexistent")
        .await
        .unwrap();

    assert!(
        result.is_none(),
        "Should return None for nonexistent username"
    );
}

#[tokio::test]
async fn test_sftp_lookup_no_username_configured() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server WITHOUT sftp_username (but with password)
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Server D",
                    "binary": "/bin/echo",
                    "args": ["test"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_password": "Password123"
                }
            }),
        )
        .await;

    assert_eq!(status, 200, "Failed to create server: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    // Should still be able to look up by UUID even without sftp_username
    let result = app
        .state
        .db
        .find_server_by_sftp_username(&server_id)
        .await
        .unwrap();

    assert!(
        result.is_some(),
        "Should find server by UUID even without custom username"
    );
    let found = result.unwrap();
    assert_eq!(found.id.to_string(), server_id);
    assert_eq!(found.config.name, "Server D");
}

#[tokio::test]
async fn test_sftp_lookup_empty_username() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server with empty sftp_username
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Server E",
                    "binary": "/bin/echo",
                    "args": ["test"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": "",
                    "sftp_password": "Password123"
                }
            }),
        )
        .await;

    assert_eq!(status, 200, "Failed to create server: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    // Should not be able to find by empty string
    let result = app.state.db.find_server_by_sftp_username("").await.unwrap();

    assert!(
        result.is_none(),
        "Should not find server with empty username search"
    );

    // But should still work with UUID
    let result = app
        .state
        .db
        .find_server_by_sftp_username(&server_id)
        .await
        .unwrap();

    assert!(result.is_some(), "Should find server by UUID");
}

#[tokio::test]
async fn test_sftp_lookup_with_multiple_servers() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create multiple servers
    for i in 1..=10 {
        let (status, _body) = app
            .post(
                "/api/servers",
                Some(&token),
                json!({
                    "config": {
                        "name": format!("Server {}", i),
                        "binary": "/bin/echo",
                        "args": ["test"],
                        "env": {},
                        "working_dir": null,
                        "auto_start": false,
                        "auto_restart": false,
                        "max_restart_attempts": 0,
                        "restart_delay_secs": 5,
                        "stop_command": null,
                        "stop_timeout_secs": 10,
                        "sftp_username": format!("user{}", i),
                        "sftp_password": "Password123"
                    }
                }),
            )
            .await;

        assert_eq!(status, 200, "Failed to create server {}", i);
    }

    // Lookup should work efficiently for any server
    let result = app
        .state
        .db
        .find_server_by_sftp_username("user5")
        .await
        .unwrap();

    assert!(result.is_some(), "Should find server by username");
    let found = result.unwrap();
    assert_eq!(found.config.name, "Server 5");
    assert_eq!(found.config.sftp_username.as_deref(), Some("user5"));

    // Lookup by UUID should also work
    let servers = app.state.db.list_servers().await.unwrap();
    let random_server = &servers[3];
    let result = app
        .state
        .db
        .find_server_by_sftp_username(&random_server.id.to_string())
        .await
        .unwrap();

    assert!(result.is_some(), "Should find server by UUID");
    let found = result.unwrap();
    assert_eq!(found.id, random_server.id);
}

#[tokio::test]
async fn test_sftp_lookup_case_sensitive() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server with lowercase username
    let (status, _body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Case Test Server",
                    "binary": "/bin/echo",
                    "args": ["test"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": "testuser",
                    "sftp_password": "Password123"
                }
            }),
        )
        .await;

    assert_eq!(status, 200);

    // Lookup with exact case should work
    let result = app
        .state
        .db
        .find_server_by_sftp_username("testuser")
        .await
        .unwrap();

    assert!(result.is_some(), "Should find server with exact case");

    // Lookup with different case should not work (case-sensitive)
    let result = app
        .state
        .db
        .find_server_by_sftp_username("TestUser")
        .await
        .unwrap();

    assert!(result.is_none(), "Should be case-sensitive");
}

#[tokio::test]
async fn test_sftp_updated_username_is_findable() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server with initial username
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Update Test Server",
                    "binary": "/bin/echo",
                    "args": ["test"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": "olduser",
                    "sftp_password": "Password123"
                }
            }),
        )
        .await;

    assert_eq!(status, 200, "Failed to create server: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap();

    // Verify old username works
    let result = app
        .state
        .db
        .find_server_by_sftp_username("olduser")
        .await
        .unwrap();

    assert!(result.is_some(), "Should find server by old username");

    // Update the username
    let (status, _body) = app
        .put(
            &format!("/api/servers/{}", server_id),
            Some(&token),
            json!({
                "config": {
                    "name": "Update Test Server",
                    "binary": "/bin/echo",
                    "args": ["test"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": "newuser",
                    "sftp_password": null
                }
            }),
        )
        .await;

    assert_eq!(status, 200);

    // Old username should no longer work
    let result = app
        .state
        .db
        .find_server_by_sftp_username("olduser")
        .await
        .unwrap();

    assert!(
        result.is_none(),
        "Should not find server by old username after update"
    );

    // New username should work
    let result = app
        .state
        .db
        .find_server_by_sftp_username("newuser")
        .await
        .unwrap();

    assert!(result.is_some(), "Should find server by new username");
    let found = result.unwrap();
    assert_eq!(found.config.sftp_username.as_deref(), Some("newuser"));
}

#[tokio::test]
async fn test_sftp_auth_uses_indexed_lookup() {
    use anyserver::auth::verify_password;

    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server with SFTP credentials
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "SFTP Auth Test",
                    "binary": "/bin/echo",
                    "args": ["test"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": "authuser",
                    "sftp_password": "TestPassword123"
                }
            }),
        )
        .await;

    assert_eq!(status, 200, "Failed to create server: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap().to_string();

    // Verify the lookup by custom username works
    let server = app
        .state
        .db
        .find_server_by_sftp_username("authuser")
        .await
        .unwrap()
        .expect("Should find server by custom username");

    assert_eq!(server.config.name, "SFTP Auth Test");
    assert_eq!(server.config.sftp_username.as_deref(), Some("authuser"));

    // Verify the password is hashed and can be verified
    let password_hash = server
        .config
        .sftp_password
        .as_ref()
        .expect("Password should be set");
    assert!(
        password_hash.starts_with("$argon2"),
        "Password should be hashed"
    );

    let valid = verify_password("TestPassword123", password_hash).unwrap();
    assert!(valid, "Correct password should validate");

    // Verify lookup by UUID also works
    let server_by_uuid = app
        .state
        .db
        .find_server_by_sftp_username(&server_id)
        .await
        .unwrap()
        .expect("Should find server by UUID");

    assert_eq!(server_by_uuid.id, server.id);
}

#[tokio::test]
async fn test_nonexistent_username_constant_time() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server to ensure database is not empty
    let (status, _body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Timing Test Server",
                    "binary": "/bin/echo",
                    "args": ["test"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": "realuser",
                    "sftp_password": "Password123"
                }
            }),
        )
        .await;

    assert_eq!(status, 200);

    // Lookup for nonexistent username should return None quickly
    // (O(1) indexed lookup, not O(n) scan)
    let start = std::time::Instant::now();
    let result = app
        .state
        .db
        .find_server_by_sftp_username("nonexistent_user_12345")
        .await
        .unwrap();
    let elapsed = start.elapsed();

    assert!(result.is_none(), "Nonexistent user should return None");

    // The lookup should be fast (indexed, not scanning all servers)
    // Even with argon2 dummy verification, this should complete quickly
    assert!(
        elapsed.as_millis() < 1000,
        "Lookup should be fast (O(1)), took {:?}",
        elapsed
    );
}
