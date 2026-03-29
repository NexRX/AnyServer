//! Integration tests for ServerWithStatus serialization (ticket 3-003).
//!
//! Covers:
//!   - Round-trip serialize/deserialize symmetry
//!   - No duplicate Server data in serialized JSON
//!   - Serialized JSON size stays under 15 KB for a fully configured server
//!   - `GET /api/servers` response entries have no duplicate Server data
//!   - `GET /api/servers/:id` response has no duplicate Server data
//!   - `ts-rs` generated ServerWithStatus type matches the wire format

use axum::http::StatusCode;
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

use crate::common::{resolve_binary, TestApp};

// ═══════════════════════════════════════════════════════════════════════
//  Unit-style tests on the ServerWithStatus type itself
// ═══════════════════════════════════════════════════════════════════════

/// Helper: build a realistic `ServerWithStatus` with a fully configured
/// `ServerConfig` (install, update, start, stop, uninstall steps, parameters,
/// isolation config, env vars, etc.) to stress-test serialization size.
fn build_fully_configured_server_with_status() -> anyserver::types::ServerWithStatus {
    use anyserver::types::*;

    let server_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();

    let make_steps = |count: usize, prefix: &str| -> Vec<PipelineStep> {
        (0..count)
            .map(|i| PipelineStep {
                name: format!("{}-step-{}", prefix, i),
                description: Some(format!("Description for {}-step-{}", prefix, i)),
                action: StepAction::RunCommand {
                    command: format!("/usr/bin/{}-cmd-{}", prefix, i),
                    args: vec![
                        "--flag".to_string(),
                        format!("value-{}", i),
                        "--verbose".to_string(),
                    ],
                    working_dir: Some("/opt/server".to_string()),
                    env: {
                        let mut m = HashMap::new();
                        m.insert("ENV_VAR".to_string(), "some_value".to_string());
                        m
                    },
                },
                condition: None,
                continue_on_error: false,
            })
            .collect()
    };

    let config = ServerConfig {
        name: "Fully Configured Test Server".to_string(),
        binary: "/usr/bin/java".to_string(),
        args: vec![
            "-Xmx4G".to_string(),
            "-Xms1G".to_string(),
            "-jar".to_string(),
            "server.jar".to_string(),
            "nogui".to_string(),
        ],
        env: {
            let mut m = HashMap::new();
            m.insert("JAVA_HOME".to_string(), "/usr/lib/jvm/java-17".to_string());
            m.insert("SERVER_PORT".to_string(), "25565".to_string());
            m.insert("EULA".to_string(), "true".to_string());
            m
        },
        working_dir: Some("/opt/minecraft".to_string()),
        auto_start: true,
        auto_restart: true,
        max_restart_attempts: 5,
        restart_delay_secs: 10,
        stop_command: Some("stop".to_string()),
        stop_signal: StopSignal::Sigterm,
        stop_timeout_secs: 30,
        sftp_username: Some("mc_sftp".to_string()),
        sftp_password: Some("hashed_password_placeholder".to_string()),
        parameters: vec![
            ConfigParameter {
                name: "version".to_string(),
                label: "Server Version".to_string(),
                description: Some("The Minecraft server version to install".to_string()),
                param_type: ConfigParameterType::String,
                default: Some("1.20.4".to_string()),
                required: true,
                options: vec![
                    "1.20.4".to_string(),
                    "1.20.3".to_string(),
                    "1.20.2".to_string(),
                ],
                regex: None,
                is_version: true,
                options_from: None,
                github_repo: None,
            },
            ConfigParameter {
                name: "memory".to_string(),
                label: "Memory Allocation".to_string(),
                description: Some("Amount of RAM to allocate".to_string()),
                param_type: ConfigParameterType::String,
                default: Some("4G".to_string()),
                required: false,
                options: vec![],
                regex: None,
                is_version: false,
                options_from: None,
                github_repo: None,
            },
        ],
        install_steps: make_steps(3, "install"),
        update_steps: make_steps(2, "update"),
        start_steps: make_steps(1, "start"),
        stop_steps: make_steps(1, "stop"),
        uninstall_steps: make_steps(2, "uninstall"),
        isolation: IsolationConfig {
            enabled: true,
            extra_read_paths: vec!["/usr/lib/jvm".to_string(), "/etc/ssl/certs".to_string()],
            extra_rw_paths: vec!["/tmp/mc-cache".to_string()],
            pids_max: Some(512),
        },
        update_check: Some(UpdateCheck {
            provider: UpdateCheckProvider::Api {
                url: "https://api.papermc.io/v2/projects/paper".to_string(),
                path: Some("versions".to_string()),
                pick: VersionPick::Last,
                value_key: None,
            },
            interval_secs: Some(3600),
            cache_secs: 300,
        }),
        log_to_disk: true,
        max_log_size_mb: 50,
        enable_java_helper: true,
        enable_dotnet_helper: false,
        steam_app_id: None,
    };

    let server = Server {
        id: server_id,
        owner_id,
        config,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        parameter_values: {
            let mut m = HashMap::new();
            m.insert("version".to_string(), "1.20.4".to_string());
            m.insert("memory".to_string(), "4G".to_string());
            m
        },
        installed: true,
        installed_at: Some(Utc::now()),
        updated_via_pipeline_at: Some(Utc::now()),
        installed_version: Some("1.20.4".to_string()),
        source_template_id: Some(Uuid::new_v4()),
    };

    let runtime = ServerRuntime {
        server_id,
        status: ServerStatus::Running,
        pid: Some(12345),
        started_at: Some(Utc::now()),
        restart_count: 2,
        next_restart_at: None,
    };

    let permission = EffectivePermission {
        level: PermissionLevel::Owner,
        is_global_admin: false,
    };

    ServerWithStatus {
        server,
        runtime,
        permission,
        phase_progress: None,
    }
}

/// Helper: build a minimal `ServerWithStatus` for simpler tests.
fn build_minimal_server_with_status() -> anyserver::types::ServerWithStatus {
    use anyserver::types::*;

    let server_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();

    let config = ServerConfig {
        name: "Minimal Test".to_string(),
        binary: "/usr/bin/echo".to_string(),
        args: vec!["hello".to_string()],
        env: HashMap::new(),
        working_dir: None,
        auto_start: false,
        auto_restart: false,
        max_restart_attempts: 0,
        restart_delay_secs: 5,
        stop_command: None,
        stop_signal: StopSignal::default(),
        stop_timeout_secs: 10,
        sftp_username: None,
        sftp_password: None,
        parameters: Vec::new(),
        install_steps: Vec::new(),
        update_steps: Vec::new(),
        start_steps: Vec::new(),
        stop_steps: Vec::new(),
        uninstall_steps: Vec::new(),
        isolation: IsolationConfig::default(),
        update_check: None,
        log_to_disk: true,
        max_log_size_mb: 50,
        enable_java_helper: false,
        enable_dotnet_helper: false,
        steam_app_id: None,
    };

    let server = Server {
        id: server_id,
        owner_id,
        config,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        parameter_values: HashMap::new(),
        installed: false,
        installed_at: None,
        updated_via_pipeline_at: None,
        installed_version: None,
        source_template_id: None,
    };

    let runtime = ServerRuntime {
        server_id,
        status: ServerStatus::Stopped,
        pid: None,
        started_at: None,
        restart_count: 0,
        next_restart_at: None,
    };

    let permission = EffectivePermission {
        level: PermissionLevel::Owner,
        is_global_admin: false,
    };

    ServerWithStatus {
        server,
        runtime,
        permission,
        phase_progress: None,
    }
}

// ─── Round-trip serialization ─────────────────────────────────────────

#[test]
fn test_server_with_status_round_trip_serialization() {
    let original = build_minimal_server_with_status();

    // Serialize to JSON
    let json_str =
        serde_json::to_string(&original).expect("ServerWithStatus should serialize to JSON");

    // Deserialize back
    let deserialized: anyserver::types::ServerWithStatus = serde_json::from_str(&json_str)
        .expect("ServerWithStatus should deserialize from its own JSON output");

    // Verify key fields survived the round-trip
    assert_eq!(deserialized.server.id, original.server.id);
    assert_eq!(deserialized.server.owner_id, original.server.owner_id);
    assert_eq!(deserialized.server.config.name, original.server.config.name);
    assert_eq!(
        deserialized.server.config.binary,
        original.server.config.binary
    );
    assert_eq!(deserialized.runtime.status, original.runtime.status);
    assert_eq!(deserialized.runtime.server_id, original.runtime.server_id);
    assert_eq!(deserialized.permission.level, original.permission.level);
    assert_eq!(
        deserialized.permission.is_global_admin,
        original.permission.is_global_admin
    );
    assert!(deserialized.phase_progress.is_none());
    assert_eq!(deserialized.server.installed, original.server.installed);
}

#[test]
fn test_server_with_status_round_trip_fully_configured() {
    let original = build_fully_configured_server_with_status();

    let json_str = serde_json::to_string(&original)
        .expect("Fully configured ServerWithStatus should serialize");

    let deserialized: anyserver::types::ServerWithStatus = serde_json::from_str(&json_str)
        .expect("Fully configured ServerWithStatus should deserialize from its own output");

    // Verify complex nested fields survived
    assert_eq!(deserialized.server.id, original.server.id);
    assert_eq!(deserialized.server.config.name, original.server.config.name);
    assert_eq!(
        deserialized.server.config.install_steps.len(),
        original.server.config.install_steps.len()
    );
    assert_eq!(
        deserialized.server.config.update_steps.len(),
        original.server.config.update_steps.len()
    );
    assert_eq!(
        deserialized.server.config.parameters.len(),
        original.server.config.parameters.len()
    );
    assert_eq!(
        deserialized.server.config.isolation.enabled,
        original.server.config.isolation.enabled
    );
    assert_eq!(
        deserialized.server.config.isolation.pids_max,
        original.server.config.isolation.pids_max
    );
    assert_eq!(
        deserialized.server.parameter_values,
        original.server.parameter_values
    );
    assert_eq!(
        deserialized.server.installed_version,
        original.server.installed_version
    );
    assert_eq!(
        deserialized.server.source_template_id,
        original.server.source_template_id
    );
    assert!(deserialized.server.config.update_check.is_some());
    assert_eq!(deserialized.runtime.pid, original.runtime.pid);
    assert_eq!(
        deserialized.runtime.restart_count,
        original.runtime.restart_count
    );
}

// ─── No duplicate data ───────────────────────────────────────────────

#[test]
fn test_server_with_status_no_duplicate_server_fields() {
    let sws = build_fully_configured_server_with_status();
    let json_val = serde_json::to_value(&sws).expect("Should serialize to Value");

    let obj = json_val.as_object().expect("Should be a JSON object");

    // With Option B (nested, no flatten), the top-level keys should be
    // exactly: "server", "runtime", "permission", "phase_progress"
    let expected_keys: Vec<&str> = vec!["server", "runtime", "permission", "phase_progress"];

    for key in expected_keys.iter() {
        assert!(
            obj.contains_key(*key),
            "Expected top-level key '{}' in ServerWithStatus JSON",
            key
        );
    }

    assert_eq!(
        obj.len(),
        expected_keys.len(),
        "ServerWithStatus JSON should have exactly {} top-level keys, got {}: {:?}",
        expected_keys.len(),
        obj.len(),
        obj.keys().collect::<Vec<_>>()
    );

    // Specifically verify that Server fields are NOT flattened at top level
    let server_field_names = [
        "id",
        "owner_id",
        "config",
        "created_at",
        "updated_at",
        "parameter_values",
        "installed",
        "installed_at",
        "updated_via_pipeline_at",
        "installed_version",
        "source_template_id",
    ];
    for field in server_field_names.iter() {
        assert!(
            !obj.contains_key(*field),
            "Server field '{}' should NOT appear at the top level of ServerWithStatus JSON \
             (it should be nested under 'server')",
            field
        );
    }

    // Verify the nested 'server' key contains the Server fields
    let server_obj = obj["server"]
        .as_object()
        .expect("'server' should be an object");
    for field in server_field_names.iter() {
        assert!(
            server_obj.contains_key(*field),
            "Server field '{}' should be present inside the nested 'server' object",
            field
        );
    }
}

#[test]
fn test_server_with_status_no_duplicate_data_minimal() {
    let sws = build_minimal_server_with_status();
    let json_str = serde_json::to_string(&sws).unwrap();

    // Count occurrences of the server id string — it should appear exactly
    // twice: once in server.id and once in runtime.server_id
    let id_str = sws.server.id.to_string();
    let count = json_str.matches(&id_str).count();
    assert_eq!(
        count, 2,
        "Server ID '{}' should appear exactly 2 times in JSON (server.id + runtime.server_id), found {}",
        id_str, count
    );

    // The server name should appear exactly once
    let name = &sws.server.config.name;
    let name_count = json_str.matches(name.as_str()).count();
    assert_eq!(
        name_count, 1,
        "Server name '{}' should appear exactly once in JSON, found {}",
        name, name_count
    );
}

// ─── JSON size sanity check ──────────────────────────────────────────

#[test]
fn test_server_with_status_json_size_under_15kb() {
    let sws = build_fully_configured_server_with_status();
    let json_str = serde_json::to_string(&sws).expect("Should serialize");

    let size_bytes = json_str.len();
    let size_kb = size_bytes as f64 / 1024.0;

    assert!(
        size_bytes < 15 * 1024,
        "Fully configured ServerWithStatus JSON should be under 15 KB, \
         but was {:.2} KB ({} bytes). This suggests duplicate data may be present.",
        size_kb,
        size_bytes
    );

    // Also verify it's a reasonable size (not suspiciously small, which
    // would indicate missing data)
    assert!(
        size_bytes > 500,
        "Fully configured ServerWithStatus JSON seems too small ({} bytes), \
         data may be missing",
        size_bytes
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Integration tests via the HTTP API
// ═══════════════════════════════════════════════════════════════════════

/// Assert that a `ServerWithStatus` JSON value from an API response has the
/// correct shape: exactly `server`, `runtime`, `permission`, `phase_progress`
/// at the top level, with no flattened Server fields.
fn assert_server_with_status_shape(val: &Value, context: &str) {
    let obj = val
        .as_object()
        .unwrap_or_else(|| panic!("{}: expected JSON object, got {:?}", context, val));

    let expected_keys = ["server", "runtime", "permission", "phase_progress"];
    for key in expected_keys.iter() {
        assert!(
            obj.contains_key(*key),
            "{}: missing expected key '{}'",
            context,
            key
        );
    }

    // Server fields should NOT be at the top level
    let forbidden_top_level = ["id", "owner_id", "config", "created_at", "updated_at"];
    for key in forbidden_top_level.iter() {
        assert!(
            !obj.contains_key(*key),
            "{}: found flattened Server field '{}' at top level — \
             this indicates duplicate serialization",
            context,
            key
        );
    }

    // Verify server sub-object has the expected structure
    assert!(
        obj["server"].is_object(),
        "{}: 'server' field should be an object",
        context
    );
    assert!(
        obj["server"]["id"].is_string(),
        "{}: server.id should be a string",
        context
    );
    assert!(
        obj["server"]["config"].is_object(),
        "{}: server.config should be an object",
        context
    );
    assert!(
        obj["server"]["config"]["name"].is_string(),
        "{}: server.config.name should be a string",
        context
    );

    // Verify runtime sub-object
    assert!(
        obj["runtime"].is_object(),
        "{}: 'runtime' field should be an object",
        context
    );
    assert!(
        obj["runtime"]["status"].is_string(),
        "{}: runtime.status should be a string",
        context
    );

    // Verify permission sub-object
    assert!(
        obj["permission"].is_object(),
        "{}: 'permission' field should be an object",
        context
    );
}

#[tokio::test]
async fn test_get_server_response_no_duplicate_data() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;
    let (id, _) = app.create_test_server(&token, "no-dup-test").await;

    let (status, body) = app.get(&format!("/api/servers/{}", id), Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    assert_server_with_status_shape(&body, "GET /api/servers/:id");

    // Verify no duplicate: the server name should appear exactly once
    let json_str = serde_json::to_string(&body).unwrap();
    let name_count = json_str.matches("no-dup-test").count();
    assert_eq!(
        name_count, 1,
        "Server name should appear once in GET /api/servers/:id response, found {}",
        name_count
    );
}

#[tokio::test]
async fn test_list_servers_response_no_duplicate_data() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    app.create_test_server(&token, "list-dup-test-1").await;
    app.create_test_server(&token, "list-dup-test-2").await;
    app.create_test_server(&token, "list-dup-test-3").await;

    let (status, body) = app.get("/api/servers", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let servers = body["servers"]
        .as_array()
        .expect("GET /api/servers should return a 'servers' array");

    assert_eq!(servers.len(), 3);

    for (i, server_entry) in servers.iter().enumerate() {
        assert_server_with_status_shape(server_entry, &format!("GET /api/servers — entry {}", i));
    }

    // Verify each name appears exactly once in the entire response
    let json_str = serde_json::to_string(&body).unwrap();
    for name in ["list-dup-test-1", "list-dup-test-2", "list-dup-test-3"] {
        let count = json_str.matches(name).count();
        assert_eq!(
            count, 1,
            "Server name '{}' should appear exactly once in list response, found {}",
            name, count
        );
    }
}

#[tokio::test]
async fn test_create_server_response_no_duplicate_data() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    let echo = resolve_binary("echo");
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "create-dup-test",
                    "binary": echo,
                    "args": ["hello"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_server_with_status_shape(&body, "POST /api/servers");

    let json_str = serde_json::to_string(&body).unwrap();
    let name_count = json_str.matches("create-dup-test").count();
    assert_eq!(
        name_count, 1,
        "Server name should appear once in create response, found {}",
        name_count
    );
}

#[tokio::test]
async fn test_update_server_response_no_duplicate_data() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;
    let (id, _) = app.create_test_server(&token, "before-update").await;

    let echo = resolve_binary("echo");
    let (status, body) = app
        .put(
            &format!("/api/servers/{}", id),
            Some(&token),
            json!({
                "config": {
                    "name": "after-update",
                    "binary": echo,
                    "args": [],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": null,
                    "sftp_password": null
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_server_with_status_shape(&body, "PUT /api/servers/:id");

    let json_str = serde_json::to_string(&body).unwrap();
    let name_count = json_str.matches("after-update").count();
    assert_eq!(
        name_count, 1,
        "Updated server name should appear once in update response, found {}",
        name_count
    );
}

// ─── API response size sanity check ──────────────────────────────────

#[tokio::test]
async fn test_list_servers_response_size_is_reasonable() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", TestApp::TEST_PASSWORD).await;

    // Create 5 servers
    for i in 0..5 {
        app.create_test_server(&token, &format!("size-test-{}", i))
            .await;
    }

    let (status, body) = app.get("/api/servers", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let json_str = serde_json::to_string(&body).unwrap();
    let size_bytes = json_str.len();

    // 5 minimal servers should be well under 50 KB
    // If duplication existed, each entry would be roughly double, pushing
    // towards 50+ KB even for minimal configs
    assert!(
        size_bytes < 50 * 1024,
        "GET /api/servers with 5 minimal servers should be under 50 KB, was {} bytes",
        size_bytes
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  ts-rs generated type matches wire format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_ts_rs_server_with_status_type_matches_wire_format() {
    let ts_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../frontend/src/types/generated/ServerWithStatus.ts");

    assert!(
        ts_path.exists(),
        "ServerWithStatus.ts should exist at {:?}. Run `cargo test` to generate ts-rs bindings.",
        ts_path
    );

    let content = std::fs::read_to_string(&ts_path).unwrap();

    // The type should reference `server: Server` (nested, not flattened)
    assert!(
        content.contains("server: Server"),
        "ServerWithStatus.ts should contain 'server: Server' (nested field). Got:\n{}",
        content
    );

    assert!(
        content.contains("runtime: ServerRuntime"),
        "ServerWithStatus.ts should contain 'runtime: ServerRuntime'. Got:\n{}",
        content
    );

    assert!(
        content.contains("permission: EffectivePermission"),
        "ServerWithStatus.ts should contain 'permission: EffectivePermission'. Got:\n{}",
        content
    );

    assert!(
        content.contains("phase_progress: PhaseProgress | null"),
        "ServerWithStatus.ts should contain 'phase_progress: PhaseProgress | null'. Got:\n{}",
        content
    );

    // The type should NOT contain flattened Server fields at the top level
    // (e.g., 'id: string' or 'config: ServerConfig' or 'owner_id: string')
    // Since the type uses nested `server: Server`, these should not appear
    // as direct fields on ServerWithStatus.
    // Note: We check for patterns that would indicate flattened fields.
    // `id:` could be part of `server_id:` so we look for standalone `id:`.
    assert!(
        !content.contains("config: ServerConfig"),
        "ServerWithStatus.ts should NOT contain a direct 'config: ServerConfig' field \
         (it should be nested under 'server'). Got:\n{}",
        content
    );

    assert!(
        !content.contains("owner_id:"),
        "ServerWithStatus.ts should NOT contain a direct 'owner_id' field \
         (it should be nested under 'server'). Got:\n{}",
        content
    );
}

#[test]
fn test_ts_rs_server_with_status_imports_correct_types() {
    let ts_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../frontend/src/types/generated/ServerWithStatus.ts");

    if !ts_path.exists() {
        // Skip if bindings haven't been generated yet
        return;
    }

    let content = std::fs::read_to_string(&ts_path).unwrap();

    // Should import the necessary types
    assert!(
        content.contains("import type { Server }") || content.contains("import type { Server,"),
        "ServerWithStatus.ts should import 'Server'. Got:\n{}",
        content
    );
    assert!(
        content.contains("import type { ServerRuntime }")
            || content.contains("import type { ServerRuntime,"),
        "ServerWithStatus.ts should import 'ServerRuntime'. Got:\n{}",
        content
    );
    assert!(
        content.contains("import type { EffectivePermission }")
            || content.contains("import type { EffectivePermission,"),
        "ServerWithStatus.ts should import 'EffectivePermission'. Got:\n{}",
        content
    );
    assert!(
        content.contains("import type { PhaseProgress }")
            || content.contains("import type { PhaseProgress,"),
        "ServerWithStatus.ts should import 'PhaseProgress'. Got:\n{}",
        content
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Deserialize asymmetry regression test
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_server_with_status_deserialize_rejects_flattened_format() {
    // If someone accidentally re-introduces the custom Serialize that emits
    // both flattened and nested data, the standard Deserialize would fail
    // on the flattened format. This test verifies that only the correct
    // nested format is accepted.
    let bad_json = json!({
        "id": "00000000-0000-0000-0000-000000000001",
        "owner_id": "00000000-0000-0000-0000-000000000002",
        "config": {
            "name": "bad",
            "binary": "/bin/echo",
            "args": [],
            "env": {},
            "auto_start": false,
            "auto_restart": false,
            "max_restart_attempts": 0,
            "restart_delay_secs": 5,
            "stop_timeout_secs": 10,
            "parameters": [],
            "install_steps": [],
            "update_steps": [],
            "start_steps": [],
            "stop_steps": [],
            "uninstall_steps": [],
            "isolation": { "enabled": true, "extra_read_paths": [], "extra_rw_paths": [] },
            "log_to_disk": true,
            "max_log_size_mb": 50
        },
        "created_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-01T00:00:00Z",
        "runtime": {
            "server_id": "00000000-0000-0000-0000-000000000001",
            "status": "stopped",
            "pid": null,
            "started_at": null,
            "restart_count": 0
        },
        "permission": {
            "level": "owner",
            "is_global_admin": false
        },
        "phase_progress": null
    });

    // This should fail because "server" key is missing (fields are flattened)
    let result = serde_json::from_value::<anyserver::types::ServerWithStatus>(bad_json);
    assert!(
        result.is_err(),
        "Deserializing a flattened (non-nested) ServerWithStatus JSON should fail, \
         but it succeeded. This means the Serialize/Deserialize are asymmetric."
    );
}

#[test]
fn test_server_with_status_deserialize_accepts_nested_format() {
    let good_json = json!({
        "server": {
            "id": "00000000-0000-0000-0000-000000000001",
            "owner_id": "00000000-0000-0000-0000-000000000002",
            "config": {
                "name": "good",
                "binary": "/bin/echo",
                "args": [],
                "env": {},
                "auto_start": false,
                "auto_restart": false,
                "max_restart_attempts": 0,
                "restart_delay_secs": 5,
                "stop_timeout_secs": 10,
                "parameters": [],
                "install_steps": [],
                "update_steps": [],
                "start_steps": [],
                "stop_steps": [],
                "uninstall_steps": [],
                "isolation": { "enabled": true, "extra_read_paths": [], "extra_rw_paths": [] },
                "log_to_disk": true,
                "max_log_size_mb": 50
            },
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z"
        },
        "runtime": {
            "server_id": "00000000-0000-0000-0000-000000000001",
            "status": "stopped",
            "pid": null,
            "started_at": null,
            "restart_count": 0
        },
        "permission": {
            "level": "owner",
            "is_global_admin": false
        },
        "phase_progress": null
    });

    let result = serde_json::from_value::<anyserver::types::ServerWithStatus>(good_json);
    assert!(
        result.is_ok(),
        "Deserializing a correctly nested ServerWithStatus JSON should succeed: {:?}",
        result.err()
    );

    let sws = result.unwrap();
    assert_eq!(sws.server.config.name, "good");
    assert_eq!(sws.runtime.status, anyserver::types::ServerStatus::Stopped);
    assert_eq!(
        sws.permission.level,
        anyserver::types::PermissionLevel::Owner
    );
    assert!(sws.phase_progress.is_none());
}
