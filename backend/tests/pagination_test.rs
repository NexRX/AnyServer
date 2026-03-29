//! Integration tests for server list pagination.

use anyserver::storage::db::Database;
use anyserver::types::{PermissionLevel, Server, ServerConfig, ServerPermission};
use chrono::Utc;
use uuid::Uuid;

async fn setup_test_db() -> Database {
    // Use a temporary file instead of :memory: to allow migrations to run
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("test_pagination_{}.db", Uuid::new_v4()));

    let db = Database::open(&db_path)
        .await
        .expect("Failed to open test database");

    // Clean up the database file when done is handled by the OS (temp dir cleanup)
    // or we could explicitly delete it, but for tests it's fine to leave it
    db
}

async fn create_test_servers(db: &Database, count: usize, prefix: &str) -> Vec<Uuid> {
    let mut ids = Vec::new();
    let owner_id = Uuid::new_v4();

    // Create the owner user to satisfy foreign key constraints
    let owner = anyserver::types::User {
        id: owner_id,
        username: format!("owner_{}", Uuid::new_v4()),
        password_hash: "$argon2id$v=19$m=19456,t=2,p=1$test$test".to_string(),
        role: anyserver::types::Role::User,
        created_at: Utc::now(),
        token_generation: 0,
        global_capabilities: vec![],
    };
    db.insert_user(&owner)
        .await
        .expect("Failed to insert owner user");

    for i in 0..count {
        let config = ServerConfig {
            name: format!("{}{:03}", prefix, i),
            binary: "/bin/bash".to_string(),
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
            sftp_username: None,
            sftp_password: None,
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
        };

        let server = Server {
            id: Uuid::new_v4(),
            owner_id,
            config,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            parameter_values: Default::default(),
            installed: false,
            installed_at: None,
            updated_via_pipeline_at: None,
            installed_version: None,
            source_template_id: None,
        };

        db.insert_server(&server)
            .await
            .expect("Failed to insert server");
        ids.push(server.id);
    }

    ids
}

#[tokio::test]
async fn test_pagination_basic() {
    let db = setup_test_db().await;
    create_test_servers(&db, 50, "server-").await;

    // First page
    let (servers, total) = db
        .list_servers_paginated(1, 25, None, None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(servers.len(), 25);
    assert_eq!(total, 50);

    // Second page
    let (servers, total) = db
        .list_servers_paginated(2, 25, None, None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(servers.len(), 25);
    assert_eq!(total, 50);

    // Third page (should be empty)
    let (servers, total) = db
        .list_servers_paginated(3, 25, None, None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(servers.len(), 0);
    assert_eq!(total, 50);
}

#[tokio::test]
async fn test_pagination_search() {
    let db = setup_test_db().await;

    // Create servers with different names
    create_test_servers(&db, 10, "minecraft-").await;
    create_test_servers(&db, 10, "terraria-").await;
    create_test_servers(&db, 10, "valheim-").await;

    // Search for "minecraft"
    let (servers, total) = db
        .list_servers_paginated(1, 25, Some("minecraft"), None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(servers.len(), 10);
    assert_eq!(total, 10);
    assert!(servers.iter().all(|s| s.config.name.contains("minecraft")));

    // Search for "terra"
    let (servers, total) = db
        .list_servers_paginated(1, 25, Some("terra"), None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(servers.len(), 10);
    assert_eq!(total, 10);
    assert!(servers.iter().all(|s| s.config.name.contains("terraria")));

    // Case-insensitive search
    let (servers, total) = db
        .list_servers_paginated(1, 25, Some("MINECRAFT"), None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(servers.len(), 10);
    assert_eq!(total, 10);
}

#[tokio::test]
async fn test_pagination_per_page_clamping() {
    let db = setup_test_db().await;
    create_test_servers(&db, 150, "server-").await;

    // Try to request 200 per page (should clamp to 100)
    let (servers, total) = db
        .list_servers_paginated(1, 200, None, None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(servers.len(), 100); // Clamped to max
    assert_eq!(total, 150);

    // Try to request 0 per page (should clamp to 1)
    let (servers, total) = db
        .list_servers_paginated(1, 0, None, None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(servers.len(), 1); // Clamped to min
    assert_eq!(total, 150);
}

#[tokio::test]
async fn test_pagination_sorting() {
    let db = setup_test_db().await;
    create_test_servers(&db, 10, "server-").await;

    // Sort by name ascending
    let (servers_asc, _) = db
        .list_servers_paginated(1, 25, None, None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    // Sort by name descending
    let (servers_desc, _) = db
        .list_servers_paginated(1, 25, None, None, "name", "desc", None)
        .await
        .expect("Failed to list servers");

    // Verify opposite order
    assert_eq!(servers_asc.len(), servers_desc.len());
    assert_eq!(
        servers_asc.first().unwrap().config.name,
        servers_desc.last().unwrap().config.name
    );
    assert_eq!(
        servers_asc.last().unwrap().config.name,
        servers_desc.first().unwrap().config.name
    );

    // Sort by created_at
    let (servers, _) = db
        .list_servers_paginated(1, 25, None, None, "created_at", "desc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(servers.len(), 10);
}

#[tokio::test]
async fn test_pagination_empty_result() {
    let db = setup_test_db().await;

    let (servers, total) = db
        .list_servers_paginated(1, 25, None, None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(servers.len(), 0);
    assert_eq!(total, 0);
}

#[tokio::test]
async fn test_pagination_search_no_matches() {
    let db = setup_test_db().await;
    create_test_servers(&db, 10, "minecraft-").await;

    let (servers, total) = db
        .list_servers_paginated(1, 25, Some("nonexistent"), None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(servers.len(), 0);
    assert_eq!(total, 0);
}

#[tokio::test]
async fn test_pagination_with_permissions() {
    let db = setup_test_db().await;

    // Create user
    let user_id = Uuid::new_v4();
    let user = anyserver::types::User {
        id: user_id,
        username: "test_user".to_string(),
        password_hash: "$argon2id$v=19$m=19456,t=2,p=1$test$test".to_string(),
        role: anyserver::types::Role::User,
        created_at: Utc::now(),
        token_generation: 0,
        global_capabilities: vec![],
    };
    db.insert_user(&user)
        .await
        .expect("Failed to insert test user");

    // Create servers owned by different users
    let owner_id_1 = Uuid::new_v4();
    let owner_id_2 = Uuid::new_v4();

    // Create owner users
    let owner1 = anyserver::types::User {
        id: owner_id_1,
        username: "owner1".to_string(),
        password_hash: "$argon2id$v=19$m=19456,t=2,p=1$test$test".to_string(),
        role: anyserver::types::Role::User,
        created_at: Utc::now(),
        token_generation: 0,
        global_capabilities: vec![],
    };
    db.insert_user(&owner1)
        .await
        .expect("Failed to insert owner1");

    let owner2 = anyserver::types::User {
        id: owner_id_2,
        username: "owner2".to_string(),
        password_hash: "$argon2id$v=19$m=19456,t=2,p=1$test$test".to_string(),
        role: anyserver::types::Role::User,
        created_at: Utc::now(),
        token_generation: 0,
        global_capabilities: vec![],
    };
    db.insert_user(&owner2)
        .await
        .expect("Failed to insert owner2");

    // Create 5 servers for owner 1
    for i in 0..5 {
        let config = ServerConfig {
            name: format!("owner1-server-{:03}", i),
            binary: "/bin/bash".to_string(),
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
            sftp_username: None,
            sftp_password: None,
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
        };

        let server = Server {
            id: Uuid::new_v4(),
            owner_id: owner_id_1,
            config,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            parameter_values: Default::default(),
            installed: false,
            installed_at: None,
            updated_via_pipeline_at: None,
            installed_version: None,
            source_template_id: None,
        };

        db.insert_server(&server)
            .await
            .expect("Failed to insert server");

        // Give user_id permission to first 2 servers
        if i < 2 {
            let perm = ServerPermission {
                user_id,
                server_id: server.id,
                level: PermissionLevel::Manager,
            };
            db.set_permission(&perm)
                .await
                .expect("Failed to set permission");
        }
    }

    // Create 5 servers for owner 2
    for i in 0..5 {
        let config = ServerConfig {
            name: format!("owner2-server-{:03}", i),
            binary: "/bin/bash".to_string(),
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
            sftp_username: None,
            sftp_password: None,
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
        };

        let server = Server {
            id: Uuid::new_v4(),
            owner_id: owner_id_2,
            config,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            parameter_values: Default::default(),
            installed: false,
            installed_at: None,
            updated_via_pipeline_at: None,
            installed_version: None,
            source_template_id: None,
        };

        db.insert_server(&server)
            .await
            .expect("Failed to insert server");
    }

    // Admin sees all servers (None = admin, no filtering)
    let (servers, total) = db
        .list_servers_paginated(1, 25, None, None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(total, 10);
    assert_eq!(servers.len(), 10);

    // Regular user sees only their permitted servers (2 from permissions)
    let (servers, total) = db
        .list_servers_paginated(1, 25, None, None, "name", "asc", Some(&user_id))
        .await
        .expect("Failed to list servers");

    assert_eq!(total, 2);
    assert_eq!(servers.len(), 2);
}

#[tokio::test]
async fn test_pagination_search_with_special_characters() {
    let db = setup_test_db().await;

    // Create owner user
    let owner_id = Uuid::new_v4();
    let owner = anyserver::types::User {
        id: owner_id,
        username: "special_owner".to_string(),
        password_hash: "$argon2id$v=19$m=19456,t=2,p=1$test$test".to_string(),
        role: anyserver::types::Role::User,
        created_at: Utc::now(),
        token_generation: 0,
        global_capabilities: vec![],
    };
    db.insert_user(&owner)
        .await
        .expect("Failed to insert owner user");

    // Create server with special characters in name
    let config = ServerConfig {
        name: "Test's \"Special\" Server %".to_string(),
        binary: "/bin/bash".to_string(),
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
        sftp_username: None,
        sftp_password: None,
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
    };

    let server = Server {
        id: Uuid::new_v4(),
        owner_id,
        config,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        parameter_values: Default::default(),
        installed: false,
        installed_at: None,
        updated_via_pipeline_at: None,
        installed_version: None,
        source_template_id: None,
    };

    db.insert_server(&server)
        .await
        .expect("Failed to insert server");

    // Search for partial match
    let (servers, total) = db
        .list_servers_paginated(1, 25, Some("Special"), None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    assert_eq!(servers.len(), 1);
    assert_eq!(total, 1);
}

#[tokio::test]
async fn test_pagination_offset_calculation() {
    let db = setup_test_db().await;
    create_test_servers(&db, 100, "server-").await;

    // Page 1: items 0-24
    let (page1, _) = db
        .list_servers_paginated(1, 25, None, None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    // Page 2: items 25-49
    let (page2, _) = db
        .list_servers_paginated(2, 25, None, None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    // Page 3: items 50-74
    let (page3, _) = db
        .list_servers_paginated(3, 25, None, None, "name", "asc", None)
        .await
        .expect("Failed to list servers");

    // Verify no overlap
    assert_ne!(page1.first().unwrap().id, page2.first().unwrap().id);
    assert_ne!(page2.first().unwrap().id, page3.first().unwrap().id);
    assert_ne!(page1.first().unwrap().id, page3.first().unwrap().id);
}
