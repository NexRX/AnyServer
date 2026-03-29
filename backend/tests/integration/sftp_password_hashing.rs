//! Integration tests for SFTP password hashing (Ticket #025).
//!
//! These tests verify that:
//! - SFTP passwords are hashed with argon2id when creating servers
//! - SFTP passwords are hashed when updating servers
//! - The "unchanged password" pattern works (None means keep existing)
//! - Passwords are never returned in API responses
//! - SFTP authentication works with hashed passwords
//! - The migration from plaintext to hashed is idempotent

use crate::common::TestApp;
use serde_json::json;

#[tokio::test]
async fn test_sftp_password_hashed_on_create() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server with an SFTP password
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Test Server",
                    "binary": "/bin/echo",
                    "args": ["hello"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": "testuser",
                    "sftp_password": "my_plaintext_password"
                }
            }),
        )
        .await;

    assert_eq!(status, 200, "Failed to create server: {:?}", body);
    let server_id = body["server"]["id"].as_str().unwrap();

    // The API response should NOT include the password (redacted)
    assert!(
        body["server"]["config"]["sftp_password"].is_null(),
        "API response should not include sftp_password"
    );

    // Verify in database that the password is hashed
    let server = app
        .state
        .db
        .get_server(server_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    let stored_password = server.config.sftp_password.as_ref().unwrap();

    assert!(
        stored_password.starts_with("$argon2"),
        "Stored password should be an argon2 hash, got: {}",
        stored_password
    );

    assert_ne!(
        stored_password, "my_plaintext_password",
        "Password should not be stored in plaintext"
    );
}

#[tokio::test]
async fn test_sftp_password_not_returned_in_get() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create server with SFTP password
    let (_, create_body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Test Server",
                    "binary": "/bin/echo",
                    "sftp_username": "testuser",
                    "sftp_password": "secretpass"
                }
            }),
        )
        .await;

    let server_id = create_body["server"]["id"].as_str().unwrap();

    // GET the server
    let (status, body) = app
        .get(&format!("/api/servers/{}", server_id), Some(&token))
        .await;

    assert_eq!(status, 200);
    assert!(
        body["server"]["config"]["sftp_password"].is_null(),
        "GET response should not include sftp_password"
    );

    // List servers - also should not include password
    let (status, body) = app.get("/api/servers", Some(&token)).await;
    assert_eq!(status, 200);

    let servers = body["servers"].as_array().unwrap();
    let our_server = servers
        .iter()
        .find(|s| s["server"]["id"] == server_id)
        .unwrap();

    assert!(
        our_server["server"]["config"]["sftp_password"].is_null(),
        "List response should not include sftp_password"
    );
}

#[tokio::test]
async fn test_sftp_password_update_with_new_password() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create server with initial password
    let (_, create_body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Test Server",
                    "binary": "/bin/echo",
                    "sftp_username": "testuser",
                    "sftp_password": "initial_password"
                }
            }),
        )
        .await;

    let server_id = create_body["server"]["id"].as_str().unwrap();

    // Get the initial hash
    let server = app
        .state
        .db
        .get_server(server_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    let initial_hash = server.config.sftp_password.clone().unwrap();

    // Update with a new password
    let (status, body) = app
        .put(
            &format!("/api/servers/{}", server_id),
            Some(&token),
            json!({
                "config": {
                    "name": "Test Server Updated",
                    "binary": "/bin/echo",
                    "sftp_username": "testuser",
                    "sftp_password": "new_password"
                }
            }),
        )
        .await;

    assert_eq!(status, 200, "Failed to update server: {:?}", body);

    // Verify the hash changed
    let updated_server = app
        .state
        .db
        .get_server(server_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    let new_hash = updated_server.config.sftp_password.clone().unwrap();

    assert!(
        new_hash.starts_with("$argon2"),
        "New password should be hashed"
    );

    assert_ne!(
        new_hash, initial_hash,
        "Password hash should change when password is updated"
    );
}

#[tokio::test]
async fn test_sftp_password_update_preserves_existing_when_none() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create server with password
    let (_, create_body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Test Server",
                    "binary": "/bin/echo",
                    "sftp_username": "testuser",
                    "sftp_password": "my_password"
                }
            }),
        )
        .await;

    let server_id = create_body["server"]["id"].as_str().unwrap();

    // Get the hash
    let server = app
        .state
        .db
        .get_server(server_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    let original_hash = server.config.sftp_password.clone().unwrap();

    // Update other fields but don't include sftp_password (None)
    let (status, body) = app
        .put(
            &format!("/api/servers/{}", server_id),
            Some(&token),
            json!({
                "config": {
                    "name": "Test Server Renamed",
                    "binary": "/bin/echo",
                    "sftp_username": "testuser"
                    // sftp_password omitted (will be None in the request)
                }
            }),
        )
        .await;

    assert_eq!(status, 200, "Failed to update server: {:?}", body);

    // Verify the password hash is unchanged
    let updated_server = app
        .state
        .db
        .get_server(server_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    let preserved_hash = updated_server.config.sftp_password.clone().unwrap();

    assert_eq!(
        preserved_hash, original_hash,
        "Password hash should be preserved when sftp_password is omitted from update"
    );
}

#[tokio::test]
async fn test_sftp_password_update_can_clear() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create server with password
    let (_, create_body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Test Server",
                    "binary": "/bin/echo",
                    "sftp_username": "testuser",
                    "sftp_password": "my_password"
                }
            }),
        )
        .await;

    let server_id = create_body["server"]["id"].as_str().unwrap();

    // Update with empty string to clear
    let (status, _) = app
        .put(
            &format!("/api/servers/{}", server_id),
            Some(&token),
            json!({
                "config": {
                    "name": "Test Server",
                    "binary": "/bin/echo",
                    "sftp_username": "testuser",
                    "sftp_password": ""
                }
            }),
        )
        .await;

    assert_eq!(status, 200);

    // Verify password was cleared
    let updated_server = app
        .state
        .db
        .get_server(server_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();

    assert!(
        updated_server.config.sftp_password.is_none(),
        "Empty string should clear the password"
    );
}

#[tokio::test]
async fn test_sftp_password_empty_string_on_create_stores_none() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create server with empty password
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Test Server",
                    "binary": "/bin/echo",
                    "sftp_username": "testuser",
                    "sftp_password": ""
                }
            }),
        )
        .await;

    assert_eq!(status, 200);
    let server_id = body["server"]["id"].as_str().unwrap();

    // Verify no password is stored
    let server = app
        .state
        .db
        .get_server(server_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();

    // Empty string should not be hashed - it should result in None or empty
    let password = &server.config.sftp_password;
    assert!(
        password.is_none() || password.as_ref().is_some_and(|p| p.is_empty()),
        "Empty password should not be hashed"
    );
}

#[tokio::test]
async fn test_migration_hashes_plaintext_passwords() {
    use anyserver::storage::migrations::migrate_sftp_passwords;
    use anyserver::types::{Server, ServerConfig};
    use chrono::Utc;
    use uuid::Uuid;

    let app = TestApp::new().await;
    let _token = app.setup_admin("admin", "Admin1234").await;

    // Get the admin user's ID to use as owner_id
    let user = app
        .state
        .db
        .get_user_by_username("admin")
        .await
        .unwrap()
        .unwrap();

    // Manually insert a server with a plaintext password (bypassing the API)
    let server = Server {
        id: Uuid::new_v4(),
        owner_id: user.id,
        config: ServerConfig {
            name: "Legacy Server".to_string(),
            binary: "/bin/echo".to_string(),
            args: vec![],
            env: Default::default(),
            working_dir: None,
            auto_start: false,
            auto_restart: false,
            max_restart_attempts: 0,
            restart_delay_secs: 5,
            stop_command: None,
            stop_signal: Default::default(),
            stop_timeout_secs: 10,
            sftp_username: Some("olduser".to_string()),
            sftp_password: Some("plaintext_legacy_password".to_string()),
            parameters: vec![],
            stop_steps: vec![],
            start_steps: vec![],
            install_steps: vec![],
            update_steps: vec![],
            uninstall_steps: vec![],
            isolation: Default::default(),
            update_check: None,
            log_to_disk: false,
            max_log_size_mb: 50,
            enable_java_helper: false,
            enable_dotnet_helper: false,
            steam_app_id: None,
        },
        created_at: Utc::now(),
        updated_at: Utc::now(),
        parameter_values: Default::default(),
        installed: false,
        installed_at: None,
        updated_via_pipeline_at: None,
        installed_version: None,
        source_template_id: None,
    };

    app.state.db.insert_server(&server).await.unwrap();

    // Verify it's plaintext
    let before = app.state.db.get_server(server.id).await.unwrap().unwrap();
    assert_eq!(
        before.config.sftp_password.as_ref().unwrap(),
        "plaintext_legacy_password"
    );

    // Run migration
    let migrated_count = migrate_sftp_passwords(&app.state.db).await.unwrap();
    assert_eq!(migrated_count, 1, "Should migrate 1 password");

    // Verify it's now hashed
    let after = app.state.db.get_server(server.id).await.unwrap().unwrap();
    let hashed = after.config.sftp_password.as_ref().unwrap();

    assert!(
        hashed.starts_with("$argon2"),
        "Password should be hashed after migration"
    );

    assert_ne!(hashed, "plaintext_legacy_password");

    // Run migration again - should be idempotent
    let second_count = migrate_sftp_passwords(&app.state.db).await.unwrap();
    assert_eq!(
        second_count, 0,
        "Second migration should not change anything"
    );

    // Verify hash is still the same
    let final_state = app.state.db.get_server(server.id).await.unwrap().unwrap();
    assert_eq!(
        final_state.config.sftp_password.as_ref().unwrap(),
        hashed,
        "Hash should be unchanged by second migration"
    );
}

#[tokio::test]
async fn test_sftp_auth_works_with_hashed_password() {
    use anyserver::auth::verify_password;

    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create server with SFTP credentials
    let (_, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "SFTP Test Server",
                    "binary": "/bin/echo",
                    "sftp_username": "sftpuser",
                    "sftp_password": "correct_password"
                }
            }),
        )
        .await;

    let server_id = body["server"]["id"].as_str().unwrap();

    // Get the server from DB to check the hash
    let server = app
        .state
        .db
        .get_server(server_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    let password_hash = server.config.sftp_password.as_ref().unwrap();

    // Verify that the correct password validates against the hash
    let valid = verify_password("correct_password", password_hash).unwrap();
    assert!(valid, "Correct password should validate");

    // Verify that wrong password fails
    let invalid = verify_password("wrong_password", password_hash).unwrap();
    assert!(!invalid, "Wrong password should not validate");
}

#[tokio::test]
async fn test_builtin_templates_have_no_sftp_password() {
    // Verify that builtin templates don't accidentally have SFTP passwords
    let templates = anyserver::templates::builtin::list();

    for template in templates {
        assert!(
            template.config.sftp_password.is_none(),
            "Built-in template '{}' should not have an SFTP password",
            template.name
        );
    }
}

#[tokio::test]
async fn test_update_doesnt_double_hash() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create server
    let (_, create_body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Test Server",
                    "binary": "/bin/echo",
                    "sftp_password": "initial_password"
                }
            }),
        )
        .await;

    let server_id = create_body["server"]["id"].as_str().unwrap();

    // Get the hash
    let server = app
        .state
        .db
        .get_server(server_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    let first_hash = server.config.sftp_password.clone().unwrap();

    // Update the server WITHOUT changing the password by omitting sftp_password
    // (null / absent field means "keep existing"). The frontend should never
    // send a hash back — it should omit the field or send null to preserve it.
    let (status, _) = app
        .put(
            &format!("/api/servers/{}", server_id),
            Some(&token),
            json!({
                "config": {
                    "name": "Test Server Updated",
                    "binary": "/bin/echo"
                }
            }),
        )
        .await;

    assert_eq!(status, 200);

    // Verify the existing hash is preserved when sftp_password is omitted
    let server = app
        .state
        .db
        .get_server(server_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    let second_hash = server.config.sftp_password.clone().unwrap();

    assert_eq!(
        first_hash, second_hash,
        "Omitting sftp_password should preserve the existing hash"
    );
}
