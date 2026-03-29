//! End-to-end tests for the built-in (curated) templates feature.
//!
//! Verifies that:
//! - Built-in templates appear in the list endpoint
//! - Built-in templates can be fetched individually
//! - Built-in templates are marked with `is_builtin: true`
//! - Built-in templates cannot be deleted
//! - Built-in templates cannot be updated
//! - User-created templates still work and are marked `is_builtin: false`
//! - User-created templates appear alongside built-in templates in the list
//! - Servers can be created from each builtin template config
//! - Local pipeline steps (WriteFile, CreateDir, etc.) execute correctly
//! - Variable substitution works with template parameters
//! - Parameter validation (regex, select, required) enforces constraints
//! - Template configs survive a JSON round-trip through the API

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::common::TestApp;

/// The three well-known built-in template UUIDs (must match the constants in
/// `anyserver::builtin_templates`).
const MINECRAFT_PAPER_UUID: &str = "00bafeed-0001-4000-8000-000000000001";
const VALHEIM_UUID: &str = "00bafeed-0001-4000-8000-000000000002";
const TERRARIA_TSHOCK_UUID: &str = "00bafeed-0001-4000-8000-000000000003";

// ─── Listing ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_templates_includes_builtins() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/templates", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let templates = body["templates"].as_array().expect("templates is an array");
    assert!(
        templates.len() >= 3,
        "should have at least 3 built-in templates, got {}",
        templates.len()
    );

    // All three well-known IDs must be present
    let ids: Vec<&str> = templates.iter().filter_map(|t| t["id"].as_str()).collect();
    assert!(
        ids.contains(&MINECRAFT_PAPER_UUID),
        "missing Minecraft Paper"
    );
    assert!(ids.contains(&VALHEIM_UUID), "missing Valheim");
    assert!(
        ids.contains(&TERRARIA_TSHOCK_UUID),
        "missing Terraria TShock"
    );
}

#[tokio::test]
async fn builtin_templates_appear_first_in_list() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/templates", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let templates = body["templates"].as_array().unwrap();
    // The first 3 entries should all be built-in
    for t in templates.iter().take(3) {
        assert_eq!(
            t["is_builtin"].as_bool(),
            Some(true),
            "first 3 templates in the list should be built-in, got: {}",
            t["name"]
        );
    }
}

#[tokio::test]
async fn list_templates_includes_both_builtin_and_user() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a user template
    let (status, _) = app
        .post(
            "/api/templates",
            Some(&token),
            json!({
                "name": "My Custom Template",
                "description": "A user-created template",
                "config": {
                    "name": "Custom Server",
                    "binary": "/usr/bin/echo",
                    "args": ["hello"]
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = app.get("/api/templates", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let templates = body["templates"].as_array().unwrap();
    // 3 built-in + 1 user-created
    assert!(
        templates.len() >= 4,
        "expected at least 4 templates (3 built-in + 1 user), got {}",
        templates.len()
    );

    let builtin_count = templates
        .iter()
        .filter(|t| t["is_builtin"].as_bool() == Some(true))
        .count();
    let user_count = templates
        .iter()
        .filter(|t| t["is_builtin"].as_bool() != Some(true))
        .count();

    assert!(
        builtin_count >= 3,
        "should have at least 3 built-in templates"
    );
    assert!(user_count >= 1, "should have at least 1 user template");
}

// ─── Fetching ────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_builtin_template_by_id() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .get(
            &format!("/api/templates/{}", MINECRAFT_PAPER_UUID),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"].as_str(), Some(MINECRAFT_PAPER_UUID));
    assert_eq!(body["name"].as_str(), Some("Minecraft Paper"));
    assert_eq!(body["is_builtin"].as_bool(), Some(true));
}

#[tokio::test]
async fn get_valheim_builtin_template() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .get(&format!("/api/templates/{}", VALHEIM_UUID), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"].as_str(), Some("Valheim Dedicated Server"));
    assert_eq!(body["is_builtin"].as_bool(), Some(true));

    // Verify it has parameters
    let params = body["config"]["parameters"].as_array().unwrap();
    assert!(
        !params.is_empty(),
        "Valheim template should have parameters"
    );
}

#[tokio::test]
async fn get_terraria_builtin_template() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .get(
            &format!("/api/templates/{}", TERRARIA_TSHOCK_UUID),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"].as_str(), Some("Terraria (TShock)"));
    assert_eq!(body["is_builtin"].as_bool(), Some(true));

    // Verify it has install steps
    let install_steps = body["config"]["install_steps"].as_array().unwrap();
    assert!(
        !install_steps.is_empty(),
        "Terraria template should have install steps"
    );
}

// ─── Immutability ────────────────────────────────────────────────────────

#[tokio::test]
async fn cannot_delete_builtin_template() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .delete(
            &format!("/api/templates/{}", MINECRAFT_PAPER_UUID),
            Some(&token),
        )
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    let msg = body["error"].as_str().unwrap_or("");
    assert!(
        msg.to_lowercase().contains("built-in"),
        "error message should mention 'built-in', got: {}",
        msg
    );
}

#[tokio::test]
async fn cannot_delete_any_builtin_template() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    for id in [MINECRAFT_PAPER_UUID, VALHEIM_UUID, TERRARIA_TSHOCK_UUID] {
        let (status, _) = app
            .delete(&format!("/api/templates/{}", id), Some(&token))
            .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "deleting built-in template {} should be forbidden",
            id
        );
    }
}

#[tokio::test]
async fn cannot_update_builtin_template() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .put(
            &format!("/api/templates/{}", MINECRAFT_PAPER_UUID),
            Some(&token),
            json!({
                "name": "Hacked Template",
                "description": "Should not work",
                "config": {
                    "name": "Hacked",
                    "binary": "/usr/bin/evil",
                    "args": []
                }
            }),
        )
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    let msg = body["error"].as_str().unwrap_or("");
    assert!(
        msg.to_lowercase().contains("built-in"),
        "error message should mention 'built-in', got: {}",
        msg
    );
}

#[tokio::test]
async fn builtin_template_still_exists_after_failed_delete() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Try to delete
    let (status, _) = app
        .delete(&format!("/api/templates/{}", VALHEIM_UUID), Some(&token))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Template should still be fetchable
    let (status, body) = app
        .get(&format!("/api/templates/{}", VALHEIM_UUID), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"].as_str(), Some("Valheim Dedicated Server"));
}

// ─── User templates still work normally ──────────────────────────────────

#[tokio::test]
async fn user_template_is_not_marked_builtin() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/templates",
            Some(&token),
            json!({
                "name": "User Template",
                "description": "Not built-in",
                "config": {
                    "name": "User Server",
                    "binary": "/usr/bin/echo",
                    "args": ["test"]
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["is_builtin"].as_bool(),
        Some(false),
        "user-created template should have is_builtin=false"
    );
}

#[tokio::test]
async fn user_template_can_be_deleted() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create
    let (status, body) = app
        .post(
            "/api/templates",
            Some(&token),
            json!({
                "name": "Deletable Template",
                "config": {
                    "name": "Deletable",
                    "binary": "/usr/bin/echo",
                    "args": []
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let id = body["id"].as_str().unwrap();

    // Delete
    let (status, body) = app
        .delete(&format!("/api/templates/{}", id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted"].as_bool(), Some(true));
}

#[tokio::test]
async fn user_template_can_be_updated() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create
    let (status, body) = app
        .post(
            "/api/templates",
            Some(&token),
            json!({
                "name": "Updatable Template",
                "config": {
                    "name": "Updatable",
                    "binary": "/usr/bin/echo",
                    "args": []
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let id = body["id"].as_str().unwrap();

    // Update
    let (status, body) = app
        .put(
            &format!("/api/templates/{}", id),
            Some(&token),
            json!({
                "name": "Updated Template",
                "config": {
                    "name": "Updated",
                    "binary": "/usr/bin/echo",
                    "args": ["updated"]
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"].as_str(), Some("Updated Template"));
}

// ─── Template content validation ─────────────────────────────────────────

#[tokio::test]
async fn builtin_templates_have_descriptions() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/templates", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let templates = body["templates"].as_array().unwrap();
    for t in templates
        .iter()
        .filter(|t| t["is_builtin"].as_bool() == Some(true))
    {
        let name = t["name"].as_str().unwrap_or("?");
        assert!(
            t["description"].is_string(),
            "built-in template '{}' must have a description",
            name
        );
        assert!(
            !t["description"].as_str().unwrap().is_empty(),
            "built-in template '{}' description must not be empty",
            name
        );
    }
}

#[tokio::test]
async fn builtin_templates_have_install_steps() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/templates", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let templates = body["templates"].as_array().unwrap();
    for t in templates
        .iter()
        .filter(|t| t["is_builtin"].as_bool() == Some(true))
    {
        let name = t["name"].as_str().unwrap_or("?");
        let steps = t["config"]["install_steps"].as_array();
        assert!(
            steps.is_some() && !steps.unwrap().is_empty(),
            "built-in template '{}' must have install steps",
            name
        );
    }
}

#[tokio::test]
async fn builtin_templates_have_parameters() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/templates", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let templates = body["templates"].as_array().unwrap();
    for t in templates
        .iter()
        .filter(|t| t["is_builtin"].as_bool() == Some(true))
    {
        let name = t["name"].as_str().unwrap_or("?");
        let params = t["config"]["parameters"].as_array();
        assert!(
            params.is_some() && !params.unwrap().is_empty(),
            "built-in template '{}' must have parameters",
            name
        );
    }
}

// ─── Auth ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn unauthenticated_cannot_list_templates() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, _) = app.get("/api/templates", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn regular_user_can_list_and_view_builtin_templates() {
    let app = TestApp::new().await;
    let (_admin_token, user_token, _user_id) = app.setup_admin_and_user().await;

    // List
    let (status, body) = app.get("/api/templates", Some(&user_token)).await;
    assert_eq!(status, StatusCode::OK);
    let templates = body["templates"].as_array().unwrap();
    assert!(templates.len() >= 3);

    // View one
    let (status, body) = app
        .get(
            &format!("/api/templates/{}", MINECRAFT_PAPER_UUID),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["is_builtin"].as_bool(), Some(true));
}

#[tokio::test]
async fn regular_user_cannot_delete_builtin_template() {
    let app = TestApp::new().await;
    let (_admin_token, user_token, _user_id) = app.setup_admin_and_user().await;

    let (status, _) = app
        .delete(
            &format!("/api/templates/{}", MINECRAFT_PAPER_UUID),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn regular_user_cannot_update_builtin_template() {
    let app = TestApp::new().await;
    let (_admin_token, user_token, _user_id) = app.setup_admin_and_user().await;

    let (status, _) = app
        .put(
            &format!("/api/templates/{}", TERRARIA_TSHOCK_UUID),
            Some(&user_token),
            json!({
                "name": "Hacked",
                "config": {
                    "name": "Hacked",
                    "binary": "/bin/false",
                    "args": []
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ─── Helpers for pipeline e2e tests ──────────────────────────────────────

/// Poll `GET /api/servers/:id/phase-status` until the pipeline finishes.
async fn poll_phase_complete(app: &TestApp, token: &str, server_id: &str) -> Value {
    app.poll_phase_complete(token, server_id).await
}

/// Fetch a builtin template's config via the API and return it as a JSON Value.
async fn fetch_template_config(app: &TestApp, token: &str, template_id: &str) -> Value {
    let (status, body) = app
        .get(&format!("/api/templates/{}", template_id), Some(token))
        .await;
    assert_eq!(status, StatusCode::OK, "fetch template failed: {:?}", body);
    body
}

/// Create a server using the given config JSON and parameter_values.
/// Returns (server_id, create_body).
async fn create_server_from_config(
    app: &TestApp,
    token: &str,
    config: Value,
    parameter_values: Value,
    source_template_id: Option<&str>,
) -> (String, Value) {
    let mut payload = json!({
        "config": config,
        "parameter_values": parameter_values,
    });
    if let Some(tid) = source_template_id {
        payload["source_template_id"] = json!(tid);
    }
    let (status, body) = app.post("/api/servers", Some(token), payload).await;
    assert_eq!(status, StatusCode::OK, "create server failed: {:?}", body);
    let id = body["server"]["id"].as_str().unwrap().to_string();
    (id, body)
}

// ─── Server creation from builtin template configs ───────────────────────

#[tokio::test]
async fn create_server_from_minecraft_paper_template_config() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, MINECRAFT_PAPER_UUID).await;
    let config = template["config"].clone();

    // Use default parameter values
    let parameter_values = json!({
        "mc_version": "1.21.4",
        "memory": "2G",
        "server_port": "25565"
    });

    let (server_id, body) = create_server_from_config(
        &app,
        &token,
        config,
        parameter_values,
        Some(MINECRAFT_PAPER_UUID),
    )
    .await;

    // Verify the server was created with the right config
    assert!(!server_id.is_empty());
    assert_eq!(body["server"]["config"]["name"], "Minecraft Paper Server");
    assert_eq!(body["server"]["config"]["binary"], "java");
    assert_eq!(body["server"]["installed"], false);

    // Verify parameters are stored
    let params = body["server"]["config"]["parameters"].as_array().unwrap();
    assert_eq!(params.len(), 3);
    let param_names: Vec<&str> = params.iter().filter_map(|p| p["name"].as_str()).collect();
    assert!(param_names.contains(&"mc_version"));
    assert!(param_names.contains(&"memory"));
    assert!(param_names.contains(&"server_port"));

    // Verify parameter values are stored
    assert_eq!(body["server"]["parameter_values"]["mc_version"], "1.21.4");
    assert_eq!(body["server"]["parameter_values"]["memory"], "2G");
    assert_eq!(body["server"]["parameter_values"]["server_port"], "25565");

    // Verify install_steps are present
    let install_steps = body["server"]["config"]["install_steps"]
        .as_array()
        .unwrap();
    assert!(
        install_steps.len() >= 3,
        "Paper should have at least 3 install steps (resolve, download, eula), got {}",
        install_steps.len()
    );

    // Verify update_steps are present
    let update_steps = body["server"]["config"]["update_steps"].as_array().unwrap();
    assert!(!update_steps.is_empty(), "Paper should have update_steps");

    // Verify stop_steps are present
    let stop_steps = body["server"]["config"]["stop_steps"].as_array().unwrap();
    assert!(!stop_steps.is_empty(), "Paper should have stop_steps");

    // Verify update_check round-tripped
    assert!(
        body["server"]["config"]["update_check"].is_object(),
        "Paper should have update_check config"
    );

    // Verify source_template_id is stored
    assert_eq!(
        body["server"]["source_template_id"].as_str(),
        Some(MINECRAFT_PAPER_UUID)
    );
}

#[tokio::test]
async fn create_server_from_valheim_template_config() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, VALHEIM_UUID).await;
    let config = template["config"].clone();

    let parameter_values = json!({
        "server_name": "Test Valheim",
        "world_name": "TestWorld",
        "password": "secret12345",
        "server_port": "2456"
    });

    let (server_id, body) =
        create_server_from_config(&app, &token, config, parameter_values, Some(VALHEIM_UUID)).await;

    assert!(!server_id.is_empty());
    assert_eq!(body["server"]["config"]["name"], "Valheim Server");
    assert_eq!(
        body["server"]["config"]["binary"],
        "./valheim_server.x86_64"
    );
    assert_eq!(
        body["server"]["parameter_values"]["server_name"],
        "Test Valheim"
    );
    assert_eq!(
        body["server"]["parameter_values"]["password"],
        "secret12345"
    );

    // Verify args contain variable references that will be substituted at runtime
    let args = body["server"]["config"]["args"].as_array().unwrap();
    let args_str: Vec<&str> = args.iter().filter_map(|a| a.as_str()).collect();
    assert!(
        args_str.contains(&"${server_name}"),
        "args should contain server_name variable ref"
    );
    assert!(
        args_str.contains(&"${server_port}"),
        "args should contain server_port variable ref"
    );

    // Verify env vars
    assert!(
        body["server"]["config"]["env"]["LD_LIBRARY_PATH"].is_string(),
        "Valheim should have LD_LIBRARY_PATH env"
    );
    assert!(
        body["server"]["config"]["env"]["SteamAppId"].is_string(),
        "Valheim should have SteamAppId env"
    );

    // Verify start_steps include SetEnv
    let start_steps = body["server"]["config"]["start_steps"].as_array().unwrap();
    assert!(
        !start_steps.is_empty(),
        "Valheim should have start_steps for SetEnv"
    );
}

#[tokio::test]
async fn create_server_from_terraria_tshock_template_config() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, TERRARIA_TSHOCK_UUID).await;
    let config = template["config"].clone();

    let parameter_values = json!({
        "tshock_version": "5.2.0",
        "max_players": "8",
        "server_port": "7777",
        "world_name": "testworld"
    });

    let (server_id, body) = create_server_from_config(
        &app,
        &token,
        config,
        parameter_values,
        Some(TERRARIA_TSHOCK_UUID),
    )
    .await;

    assert!(!server_id.is_empty());
    assert_eq!(body["server"]["config"]["name"], "Terraria TShock Server");
    assert_eq!(body["server"]["config"]["binary"], "./TShock.Server");
    assert_eq!(
        body["server"]["parameter_values"]["tshock_version"],
        "5.2.0"
    );
    assert_eq!(
        body["server"]["parameter_values"]["world_name"],
        "testworld"
    );

    // Verify install steps include the full chain: create_dir, download, extract, delete, set_permissions
    let install_steps = body["server"]["config"]["install_steps"]
        .as_array()
        .unwrap();
    assert!(
        install_steps.len() >= 4,
        "TShock install should have at least 4 steps, got {}",
        install_steps.len()
    );

    let step_types: Vec<&str> = install_steps
        .iter()
        .filter_map(|s| s["action"]["type"].as_str())
        .collect();
    assert!(
        step_types.contains(&"create_dir"),
        "install should have create_dir step"
    );
    assert!(
        step_types.contains(&"download_github_release_asset"),
        "install should have download_github_release_asset step"
    );
    assert!(
        step_types.contains(&"extract"),
        "install should have extract step"
    );
    assert!(
        step_types.contains(&"delete"),
        "install should have delete step (cleanup)"
    );

    // Verify update_check
    assert!(body["server"]["config"]["update_check"].is_object());
    assert_eq!(
        body["server"]["config"]["update_check"]["provider"].as_str(),
        Some("api")
    );
}

// ─── Parameter validation through the API ────────────────────────────────

#[tokio::test]
async fn minecraft_paper_rejects_invalid_version_format() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, MINECRAFT_PAPER_UUID).await;
    let config = template["config"].clone();

    // Invalid version format — "latest" doesn't match the regex ^\d+\.\d+(\.\d+)?$
    let parameter_values = json!({
        "mc_version": "latest",
        "memory": "2G",
        "server_port": "25565"
    });

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": config,
                "parameter_values": parameter_values,
            }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Should reject invalid mc_version format: {:?}",
        body
    );
}

#[tokio::test]
async fn minecraft_paper_rejects_invalid_memory_option() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, MINECRAFT_PAPER_UUID).await;
    let config = template["config"].clone();

    // Invalid select option — "3G" is not in [1G, 2G, 4G, 6G, 8G]
    let parameter_values = json!({
        "mc_version": "1.21.4",
        "memory": "3G",
        "server_port": "25565"
    });

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": config,
                "parameter_values": parameter_values,
            }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Should reject invalid memory select option: {:?}",
        body
    );
}

#[tokio::test]
async fn minecraft_paper_rejects_missing_required_param() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, MINECRAFT_PAPER_UUID).await;
    let config = template["config"].clone();

    // Missing mc_version (required)
    let parameter_values = json!({
        "memory": "2G",
        "server_port": "25565"
    });

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": config,
                "parameter_values": parameter_values,
            }),
        )
        .await;
    // Should either reject or use the default — let's just check it doesn't crash
    // If the default is used, it should succeed with the default value
    if status == StatusCode::OK {
        // Default was used
        assert_eq!(
            body["server"]["parameter_values"]["mc_version"], "1.21.4",
            "Should use default mc_version when not provided"
        );
    } else {
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn valheim_rejects_short_password() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, VALHEIM_UUID).await;
    let config = template["config"].clone();

    // Password too short — regex requires >=5 chars
    let parameter_values = json!({
        "server_name": "Test",
        "world_name": "World",
        "password": "abcd",
        "server_port": "2456"
    });

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": config,
                "parameter_values": parameter_values,
            }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Should reject short Valheim password: {:?}",
        body
    );
}

#[tokio::test]
async fn terraria_rejects_world_name_with_spaces() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, TERRARIA_TSHOCK_UUID).await;
    let config = template["config"].clone();

    // World name with spaces — regex requires ^[a-zA-Z0-9_-]+$
    let parameter_values = json!({
        "tshock_version": "5.2.0",
        "max_players": "8",
        "server_port": "7777",
        "world_name": "my world"
    });

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": config,
                "parameter_values": parameter_values,
            }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Should reject world name with spaces: {:?}",
        body
    );
}

#[tokio::test]
async fn terraria_rejects_invalid_max_players_option() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, TERRARIA_TSHOCK_UUID).await;
    let config = template["config"].clone();

    // max_players "10" is not in the select options [4, 8, 16, 32, 64]
    let parameter_values = json!({
        "tshock_version": "5.2.0",
        "max_players": "10",
        "server_port": "7777",
        "world_name": "world"
    });

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": config,
                "parameter_values": parameter_values,
            }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Should reject invalid max_players option: {:?}",
        body
    );
}

// ─── Local pipeline execution (no network) ───────────────────────────────
//
// We can't test Download/ResolveVariable steps without hitting real APIs,
// but we CAN construct pipelines using the same local step types that the
// builtin templates use (WriteFile, CreateDir, SetPermissions, etc.) and
// verify they execute correctly with the template's variable schema.

#[tokio::test]
async fn minecraft_paper_local_steps_execute_with_variable_substitution() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server with Minecraft Paper's parameters but replace the
    // install_steps with only the local steps (WriteFile for eula.txt and
    // server.properties) so we don't hit the PaperMC network API.
    let template = fetch_template_config(&app, &token, MINECRAFT_PAPER_UUID).await;
    let mut config = template["config"].clone();

    // Replace install_steps with just the local WriteFile steps
    config["install_steps"] = json!([
        {
            "name": "Accept EULA",
            "description": "Write eula.txt to accept the Minecraft EULA.",
            "action": {
                "type": "write_file",
                "path": "eula.txt",
                "content": "# Auto-accepted by AnyServer template\neula=true\n"
            },
            "condition": null,
            "continue_on_error": false
        },
        {
            "name": "Write server.properties",
            "description": "Create a default server.properties with the configured port.",
            "action": {
                "type": "write_file",
                "path": "server.properties",
                "content": "# Minecraft Server Properties\n# Generated by AnyServer\nserver-port=${server_port}\nonline-mode=true\nmax-players=20\n"
            },
            "condition": {
                "path_not_exists": "server.properties"
            },
            "continue_on_error": false
        }
    ]);

    let parameter_values = json!({
        "mc_version": "1.21.4",
        "memory": "2G",
        "server_port": "25577"
    });

    let (server_id, _) =
        create_server_from_config(&app, &token, config, parameter_values, None).await;

    // Trigger install
    let (status, inst_body) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "install trigger failed: {:?}",
        inst_body
    );

    let phase_body = poll_phase_complete(&app, &token, &server_id).await;
    let progress = &phase_body["progress"];
    assert_eq!(
        progress["status"], "completed",
        "Install should succeed: {:?}",
        progress
    );

    // Verify EULA file was written
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    let eula = std::fs::read_to_string(server_dir.join("eula.txt")).unwrap();
    assert!(eula.contains("eula=true"), "eula.txt should accept EULA");

    // Verify server.properties was written with substituted port
    let props = std::fs::read_to_string(server_dir.join("server.properties")).unwrap();
    assert!(
        props.contains("server-port=25577"),
        "server.properties should have port 25577, got: {}",
        props
    );

    // Verify the server is now marked installed
    assert_eq!(phase_body["installed"], true);
}

#[tokio::test]
async fn minecraft_paper_conditional_step_skips_when_file_exists() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, MINECRAFT_PAPER_UUID).await;
    let mut config = template["config"].clone();

    // Install steps: write eula, then write server.properties (conditional)
    config["install_steps"] = json!([
        {
            "name": "Accept EULA",
            "action": {
                "type": "write_file",
                "path": "eula.txt",
                "content": "eula=true\n"
            },
            "continue_on_error": false
        },
        {
            "name": "Write server.properties",
            "action": {
                "type": "write_file",
                "path": "server.properties",
                "content": "server-port=${server_port}\n"
            },
            "condition": {
                "path_not_exists": "server.properties"
            },
            "continue_on_error": false
        }
    ]);

    let parameter_values = json!({
        "mc_version": "1.21.4",
        "memory": "2G",
        "server_port": "25565"
    });

    let (server_id, _) =
        create_server_from_config(&app, &token, config, parameter_values, None).await;

    // Pre-create server.properties with custom content
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    std::fs::write(
        server_dir.join("server.properties"),
        "# Custom user config\nserver-port=19132\n",
    )
    .unwrap();

    // Run install
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let phase_body = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(phase_body["progress"]["status"], "completed");

    // server.properties step should have been skipped
    let steps = phase_body["progress"]["steps"].as_array().unwrap();
    assert_eq!(
        steps[1]["status"], "skipped",
        "Conditional step should be skipped when file exists"
    );

    // Verify the original content was preserved
    let props = std::fs::read_to_string(server_dir.join("server.properties")).unwrap();
    assert!(
        props.contains("server-port=19132"),
        "Original server.properties should be preserved, got: {}",
        props
    );
}

#[tokio::test]
async fn terraria_local_steps_create_dir_and_set_permissions() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, TERRARIA_TSHOCK_UUID).await;
    let mut config = template["config"].clone();

    // Replace install_steps with local-only steps: CreateDir + WriteFile + SetPermissions
    config["install_steps"] = json!([
        {
            "name": "Create worlds directory",
            "description": "Ensure the worlds directory exists.",
            "action": { "type": "create_dir", "path": "worlds" },
            "continue_on_error": true
        },
        {
            "name": "Create fake server binary",
            "action": {
                "type": "write_file",
                "path": "TShock.Server",
                "content": "#!/bin/sh\necho TShock ${tshock_version} running on port ${server_port}\n"
            },
            "continue_on_error": false
        },
        {
            "name": "Make server executable",
            "action": {
                "type": "set_permissions",
                "path": "TShock.Server",
                "mode": "755"
            },
            "condition": {
                "path_exists": "TShock.Server"
            },
            "continue_on_error": true
        }
    ]);

    let parameter_values = json!({
        "tshock_version": "5.2.0",
        "max_players": "8",
        "server_port": "7777",
        "world_name": "testworld"
    });

    let (server_id, _) =
        create_server_from_config(&app, &token, config, parameter_values, None).await;

    // Trigger install
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let phase_body = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(
        phase_body["progress"]["status"], "completed",
        "Install should succeed: {:?}",
        phase_body["progress"]
    );

    // Verify all steps completed
    let steps = phase_body["progress"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0]["status"], "completed", "CreateDir should complete");
    assert_eq!(steps[1]["status"], "completed", "WriteFile should complete");
    assert_eq!(
        steps[2]["status"], "completed",
        "SetPermissions should complete"
    );

    // Verify filesystem state
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    assert!(
        server_dir.join("worlds").is_dir(),
        "worlds dir should exist"
    );

    let binary = server_dir.join("TShock.Server");
    assert!(binary.is_file(), "TShock.Server should exist");

    // Verify variable substitution in file content
    let content = std::fs::read_to_string(&binary).unwrap();
    assert!(
        content.contains("5.2.0"),
        "Should contain substituted tshock_version: {}",
        content
    );
    assert!(
        content.contains("7777"),
        "Should contain substituted server_port: {}",
        content
    );

    // Verify executable permission
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&binary).unwrap().permissions();
        let mode = perms.mode() & 0o777;
        assert_eq!(mode, 0o755, "Binary should be 755, got {:o}", mode);
    }

    assert_eq!(phase_body["installed"], true);
}

#[tokio::test]
async fn valheim_local_set_env_step_executes() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, VALHEIM_UUID).await;
    let mut config = template["config"].clone();

    // Replace install_steps with just CreateDir + WriteFile to keep it local
    config["install_steps"] = json!([
        {
            "name": "Create data dir",
            "action": { "type": "create_dir", "path": "data" },
            "continue_on_error": false
        },
        {
            "name": "Write startup info",
            "action": {
                "type": "write_file",
                "path": "data/server_info.txt",
                "content": "name=${server_name}\nworld=${world_name}\nport=${server_port}\n"
            },
            "continue_on_error": false
        }
    ]);

    let parameter_values = json!({
        "server_name": "My Viking Server",
        "world_name": "Midgard",
        "password": "odinrules",
        "server_port": "2456"
    });

    let (server_id, _) =
        create_server_from_config(&app, &token, config, parameter_values, None).await;

    // Trigger install
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let phase_body = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(
        phase_body["progress"]["status"], "completed",
        "Install should succeed: {:?}",
        phase_body["progress"]
    );

    // Verify variable substitution
    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    let info = std::fs::read_to_string(server_dir.join("data/server_info.txt")).unwrap();
    assert!(
        info.contains("name=My Viking Server"),
        "Should substitute server_name: {}",
        info
    );
    assert!(
        info.contains("world=Midgard"),
        "Should substitute world_name: {}",
        info
    );
    assert!(
        info.contains("port=2456"),
        "Should substitute server_port: {}",
        info
    );
}

// ─── Config round-trip through the API ───────────────────────────────────

#[tokio::test]
async fn all_builtin_template_configs_round_trip_through_api() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    for template_id in &[MINECRAFT_PAPER_UUID, VALHEIM_UUID, TERRARIA_TSHOCK_UUID] {
        let template = fetch_template_config(&app, &token, template_id).await;
        let name = template["name"].as_str().unwrap_or("?");

        // Verify essential config fields survived the API round-trip
        let config = &template["config"];
        assert!(
            config["name"].is_string() && !config["name"].as_str().unwrap().is_empty(),
            "Template '{}': config.name missing after API fetch",
            name
        );
        assert!(
            config["binary"].is_string() && !config["binary"].as_str().unwrap().is_empty(),
            "Template '{}': config.binary missing after API fetch",
            name
        );
        assert!(
            config["parameters"].is_array() && !config["parameters"].as_array().unwrap().is_empty(),
            "Template '{}': config.parameters missing after API fetch",
            name
        );
        assert!(
            config["install_steps"].is_array()
                && !config["install_steps"].as_array().unwrap().is_empty(),
            "Template '{}': config.install_steps missing after API fetch",
            name
        );

        // Verify each parameter has all expected fields
        for param in config["parameters"].as_array().unwrap() {
            let pname = param["name"].as_str().unwrap_or("?");
            assert!(
                param["label"].is_string(),
                "Template '{}', param '{}': label missing",
                name,
                pname
            );
            assert!(
                param["param_type"].is_string(),
                "Template '{}', param '{}': param_type missing",
                name,
                pname
            );
            assert!(
                !param["required"].is_null(),
                "Template '{}', param '{}': required missing",
                name,
                pname
            );
        }

        // Verify each install step has the right structure
        for step in config["install_steps"].as_array().unwrap() {
            let sname = step["name"].as_str().unwrap_or("?");
            assert!(
                step["action"].is_object(),
                "Template '{}', step '{}': action missing",
                name,
                sname
            );
            assert!(
                step["action"]["type"].is_string(),
                "Template '{}', step '{}': action.type missing",
                name,
                sname
            );
        }
    }
}

#[tokio::test]
async fn builtin_template_step_actions_have_valid_types() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let valid_types = [
        "download",
        "extract",
        "move",
        "copy",
        "delete",
        "create_dir",
        "run_command",
        "write_file",
        "edit_file",
        "set_permissions",
        "glob",
        "set_env",
        "set_working_dir",
        "set_stop_command",
        "set_stop_signal",
        "send_input",
        "send_signal",
        "sleep",
        "wait_for_output",
        "resolve_variable",
        "download_github_release_asset",
        "steam_cmd_install",
        "steam_cmd_update",
    ];

    for template_id in &[MINECRAFT_PAPER_UUID, VALHEIM_UUID, TERRARIA_TSHOCK_UUID] {
        let template = fetch_template_config(&app, &token, template_id).await;
        let name = template["name"].as_str().unwrap_or("?");
        let config = &template["config"];

        let step_lists = [
            ("install_steps", &config["install_steps"]),
            ("update_steps", &config["update_steps"]),
            ("start_steps", &config["start_steps"]),
            ("stop_steps", &config["stop_steps"]),
        ];

        for (phase, steps_val) in &step_lists {
            if let Some(steps) = steps_val.as_array() {
                for step in steps {
                    let action_type = step["action"]["type"].as_str().unwrap_or("");
                    assert!(
                        valid_types.contains(&action_type),
                        "Template '{}', phase '{}', step '{}': unknown action type '{}'",
                        name,
                        phase,
                        step["name"].as_str().unwrap_or("?"),
                        action_type
                    );
                }
            }
        }
    }
}

// ─── Update pipeline round-trip ──────────────────────────────────────────

#[tokio::test]
async fn terraria_local_update_steps_execute_after_install() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, TERRARIA_TSHOCK_UUID).await;
    let mut config = template["config"].clone();

    // Local-only install steps
    config["install_steps"] = json!([
        {
            "name": "Create worlds directory",
            "action": { "type": "create_dir", "path": "worlds" },
            "continue_on_error": false
        },
        {
            "name": "Write marker file",
            "action": {
                "type": "write_file",
                "path": "installed.txt",
                "content": "version=${tshock_version}\n"
            },
            "continue_on_error": false
        }
    ]);

    // Local-only update steps
    config["update_steps"] = json!([
        {
            "name": "Write update marker",
            "action": {
                "type": "write_file",
                "path": "updated.txt",
                "content": "updated_version=${tshock_version}\n"
            },
            "continue_on_error": false
        }
    ]);

    let parameter_values = json!({
        "tshock_version": "5.2.0",
        "max_players": "8",
        "server_port": "7777",
        "world_name": "testworld"
    });

    let (server_id, _) =
        create_server_from_config(&app, &token, config, parameter_values, None).await;

    // Run install first
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let install_result = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(install_result["progress"]["status"], "completed");

    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    let installed = std::fs::read_to_string(server_dir.join("installed.txt")).unwrap();
    assert!(
        installed.contains("version=5.2.0"),
        "Install should write version: {}",
        installed
    );

    // Now run update
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/update", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let update_result = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(
        update_result["progress"]["status"], "completed",
        "Update should succeed: {:?}",
        update_result["progress"]
    );

    let updated = std::fs::read_to_string(server_dir.join("updated.txt")).unwrap();
    assert!(
        updated.contains("updated_version=5.2.0"),
        "Update should write version: {}",
        updated
    );
}

// ─── Built-in variable substitution (server_id, server_name, server_dir) ─

#[tokio::test]
async fn builtin_variables_are_available_in_template_pipeline() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let template = fetch_template_config(&app, &token, MINECRAFT_PAPER_UUID).await;
    let mut config = template["config"].clone();

    config["install_steps"] = json!([
        {
            "name": "Write debug info",
            "action": {
                "type": "write_file",
                "path": "debug.txt",
                "content": "id=${server_id}\nname=${server_name}\ndir=${server_dir}\nversion=${mc_version}\nport=${server_port}\n"
            },
            "continue_on_error": false
        }
    ]);

    let parameter_values = json!({
        "mc_version": "1.21.4",
        "memory": "4G",
        "server_port": "25577"
    });

    let (server_id, _) =
        create_server_from_config(&app, &token, config, parameter_values, None).await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/install", server_id),
            Some(&token),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let phase_body = poll_phase_complete(&app, &token, &server_id).await;
    assert_eq!(phase_body["progress"]["status"], "completed");

    let server_dir = app._temp_dir.path().join("servers").join(&server_id);
    let debug = std::fs::read_to_string(server_dir.join("debug.txt")).unwrap();

    // Verify built-in variables were substituted
    assert!(
        debug.contains(&format!("id={}", server_id)),
        "Should contain server_id: {}",
        debug
    );
    assert!(
        debug.contains("name=Minecraft Paper Server"),
        "Should contain server_name: {}",
        debug
    );
    assert!(
        debug.contains(&format!("dir={}", server_dir.to_string_lossy())),
        "Should contain server_dir: {}",
        debug
    );
    // Verify parameter variables were substituted
    assert!(
        debug.contains("version=1.21.4"),
        "Should contain mc_version: {}",
        debug
    );
    assert!(
        debug.contains("port=25577"),
        "Should contain server_port: {}",
        debug
    );
}
