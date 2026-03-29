use axum::http::StatusCode;
use serde_json::json;

use crate::common::{resolve_binary, TestApp};

#[tokio::test]
async fn test_create_server_with_parameters_and_values() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Parameterized Server",
                    "binary": echo,
                    "args": ["--port", "${port}"],
                    "env": {},
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": false,
                    "max_restart_attempts": 0,
                    "restart_delay_secs": 5,
                    "stop_command": null,
                    "stop_timeout_secs": 10,
                    "sftp_username": null,
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "port",
                            "label": "Server Port",
                            "description": "The port to listen on",
                            "param_type": "number",
                            "default": "25565",
                            "required": true,
                            "options": [],
                            "regex": null
                        },
                        {
                            "name": "motd",
                            "label": "Message of the Day",
                            "description": null,
                            "param_type": "string",
                            "default": "Welcome!",
                            "required": false,
                            "options": [],
                            "regex": null
                        }
                    ],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {
                    "port": "25577",
                    "motd": "Hello World"
                }
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    assert_eq!(body["server"]["config"]["name"], "Parameterized Server");

    // Verify parameters are stored in config
    let params = body["server"]["config"]["parameters"].as_array().unwrap();
    assert_eq!(params.len(), 2);
    assert_eq!(params[0]["name"], "port");
    assert_eq!(params[0]["label"], "Server Port");
    assert_eq!(params[0]["param_type"], "number");
    assert_eq!(params[0]["required"], true);
    assert_eq!(params[1]["name"], "motd");
    assert_eq!(params[1]["required"], false);

    // Verify parameter values are stored
    assert_eq!(body["server"]["parameter_values"]["port"], "25577");
    assert_eq!(body["server"]["parameter_values"]["motd"], "Hello World");

    // Server should not be installed yet
    assert_eq!(body["server"]["installed"], false);
    assert!(body["server"]["installed_at"].is_null());
}

#[tokio::test]
async fn test_create_server_missing_required_parameter() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Missing Param Server",
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
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "version",
                            "label": "Server Version",
                            "description": null,
                            "param_type": "string",
                            "default": null,
                            "required": true,
                            "options": [],
                            "regex": null
                        }
                    ],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = body["error"].as_str().unwrap();
    assert!(
        err.contains("required"),
        "Expected 'required' in error: {}",
        err
    );
    assert!(
        err.contains("version") || err.contains("Server Version"),
        "Expected parameter name in error: {}",
        err
    );
}

#[tokio::test]
async fn test_create_server_empty_required_parameter_value() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Empty Param Server",
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
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "version",
                            "label": "Server Version",
                            "description": null,
                            "param_type": "string",
                            "default": null,
                            "required": true,
                            "options": [],
                            "regex": null
                        }
                    ],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {
                    "version": "   "
                }
            }),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = body["error"].as_str().unwrap();
    assert!(
        err.contains("required"),
        "Expected 'required' in error for blank value: {}",
        err
    );
}

#[tokio::test]
async fn test_create_server_select_parameter_invalid_option() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Select Param Server",
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
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "difficulty",
                            "label": "Difficulty",
                            "description": null,
                            "param_type": "select",
                            "default": "normal",
                            "required": true,
                            "options": ["easy", "normal", "hard"],
                            "regex": null
                        }
                    ],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {
                    "difficulty": "impossible"
                }
            }),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = body["error"].as_str().unwrap();
    assert!(
        err.contains("not one of the allowed options"),
        "Expected options validation error: {}",
        err
    );
}

#[tokio::test]
async fn test_create_server_select_parameter_valid_option() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Select Param Valid",
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
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "difficulty",
                            "label": "Difficulty",
                            "description": null,
                            "param_type": "select",
                            "default": "normal",
                            "required": true,
                            "options": ["easy", "normal", "hard"],
                            "regex": null
                        }
                    ],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {
                    "difficulty": "hard"
                }
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    assert_eq!(body["server"]["parameter_values"]["difficulty"], "hard");
}

#[tokio::test]
async fn test_create_server_regex_parameter_validation() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    // Invalid: doesn't match regex
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Regex Param Server",
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
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "version",
                            "label": "Version",
                            "description": null,
                            "param_type": "string",
                            "default": null,
                            "required": true,
                            "options": [],
                            "regex": "^\\d+\\.\\d+\\.\\d+$"
                        }
                    ],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {
                    "version": "not-a-version"
                }
            }),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = body["error"].as_str().unwrap();
    assert!(
        err.contains("does not match"),
        "Expected regex validation error: {}",
        err
    );

    // Valid: matches regex
    let (status2, body2) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Regex Param Server OK",
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
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "version",
                            "label": "Version",
                            "description": null,
                            "param_type": "string",
                            "default": null,
                            "required": true,
                            "options": [],
                            "regex": "^\\d+\\.\\d+\\.\\d+$"
                        }
                    ],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {
                    "version": "1.20.4"
                }
            }),
        )
        .await;

    assert_eq!(status2, StatusCode::OK, "body: {:?}", body2);
    assert_eq!(body2["server"]["parameter_values"]["version"], "1.20.4");
}

#[tokio::test]
async fn test_create_server_with_install_steps() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Install Steps Server",
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
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [
                        {
                            "name": "Create config dir",
                            "description": "Set up the configuration directory",
                            "action": {
                                "type": "create_dir",
                                "path": "config"
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Write config file",
                            "description": "Write the default configuration",
                            "action": {
                                "type": "write_file",
                                "path": "config/server.properties",
                                "content": "server-port=25565\nmotd=Hello World"
                            },
                            "condition": {
                                "path_exists": null,
                                "path_not_exists": "config/server.properties"
                            },
                            "continue_on_error": false
                        },
                        {
                            "name": "Set permissions",
                            "description": null,
                            "action": {
                                "type": "set_permissions",
                                "path": "config/server.properties",
                                "mode": "644"
                            },
                            "condition": {
                                "path_exists": "config/server.properties",
                                "path_not_exists": null
                            },
                            "continue_on_error": true
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    let install_steps = body["server"]["config"]["install_steps"].as_array().unwrap();
    assert_eq!(install_steps.len(), 3);

    // Verify step 1
    assert_eq!(install_steps[0]["name"], "Create config dir");
    assert_eq!(install_steps[0]["action"]["type"], "create_dir");
    assert_eq!(install_steps[0]["action"]["path"], "config");
    assert_eq!(install_steps[0]["continue_on_error"], false);
    assert!(install_steps[0]["condition"].is_null());

    // Verify step 2 with condition
    assert_eq!(install_steps[1]["name"], "Write config file");
    assert_eq!(install_steps[1]["action"]["type"], "write_file");
    assert_eq!(
        install_steps[1]["action"]["path"],
        "config/server.properties"
    );
    assert!(install_steps[1]["condition"]["path_not_exists"].is_string());
    assert!(install_steps[1]["condition"]["path_exists"].is_null());

    // Verify step 3 with continue_on_error
    assert_eq!(install_steps[2]["name"], "Set permissions");
    assert_eq!(install_steps[2]["action"]["type"], "set_permissions");
    assert_eq!(install_steps[2]["action"]["mode"], "644");
    assert_eq!(install_steps[2]["continue_on_error"], true);
    assert!(install_steps[2]["condition"]["path_exists"].is_string());
}

#[tokio::test]
async fn test_create_server_with_update_steps() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Update Steps Server",
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
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "version",
                            "label": "Version",
                            "description": null,
                            "param_type": "string",
                            "default": "1.20.4",
                            "required": true,
                            "options": [],
                            "regex": null
                        }
                    ],
                    "install_steps": [],
                    "update_steps": [
                        {
                            "name": "Download new version",
                            "description": "Download the updated server jar",
                            "action": {
                                "type": "download",
                                "url": "https://example.com/server-${version}.jar",
                                "destination": ".",
                                "filename": "server.jar",
                                "executable": false
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Rename old jar",
                            "description": null,
                            "action": {
                                "type": "move",
                                "source": "server.jar",
                                "destination": "server.jar.bak"
                            },
                            "condition": {
                                "path_exists": "server.jar",
                                "path_not_exists": null
                            },
                            "continue_on_error": true
                        }
                    ]
                },
                "parameter_values": {
                    "version": "1.20.5"
                }
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    let update_steps = body["server"]["config"]["update_steps"].as_array().unwrap();
    assert_eq!(update_steps.len(), 2);

    assert_eq!(update_steps[0]["name"], "Download new version");
    assert_eq!(update_steps[0]["action"]["type"], "download");
    assert_eq!(
        update_steps[0]["action"]["url"],
        "https://example.com/server-${version}.jar"
    );
    assert_eq!(update_steps[0]["action"]["filename"], "server.jar");

    assert_eq!(update_steps[1]["name"], "Rename old jar");
    assert_eq!(update_steps[1]["action"]["type"], "move");
    assert_eq!(update_steps[1]["action"]["source"], "server.jar");
    assert_eq!(update_steps[1]["action"]["destination"], "server.jar.bak");
}

#[tokio::test]
async fn test_create_full_wizard_server_with_all_phases() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Full Wizard Server",
                    "binary": echo,
                    "args": ["-jar", "server.jar", "--port", "${port}"],
                    "env": { "JAVA_HOME": "/usr/lib/jvm/java-17" },
                    "working_dir": null,
                    "auto_start": false,
                    "auto_restart": true,
                    "max_restart_attempts": 3,
                    "restart_delay_secs": 10,
                    "stop_command": "stop",
                    "stop_timeout_secs": 30,
                    "sftp_username": "mc_user",
                    "sftp_password": "mc_pass",
                    "parameters": [
                        {
                            "name": "port",
                            "label": "Port",
                            "description": "Server port",
                            "param_type": "number",
                            "default": "25565",
                            "required": true,
                            "options": [],
                            "regex": null
                        },
                        {
                            "name": "version",
                            "label": "MC Version",
                            "description": null,
                            "param_type": "string",
                            "default": null,
                            "required": true,
                            "options": [],
                            "regex": "^\\d+\\.\\d+(\\.\\d+)?$"
                        },
                        {
                            "name": "difficulty",
                            "label": "Difficulty",
                            "description": null,
                            "param_type": "select",
                            "default": "normal",
                            "required": false,
                            "options": ["peaceful", "easy", "normal", "hard"],
                            "regex": null
                        }
                    ],
                    "install_steps": [
                        {
                            "name": "Create dirs",
                            "description": null,
                            "action": { "type": "create_dir", "path": "plugins" },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Write eula",
                            "description": null,
                            "action": {
                                "type": "write_file",
                                "path": "eula.txt",
                                "content": "eula=true"
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": [
                        {
                            "name": "Backup old jar",
                            "description": null,
                            "action": {
                                "type": "copy",
                                "source": "server.jar",
                                "destination": "server.jar.bak",
                                "recursive": false
                            },
                            "condition": { "path_exists": "server.jar", "path_not_exists": null },
                            "continue_on_error": true
                        }
                    ]
                },
                "parameter_values": {
                    "port": "25577",
                    "version": "1.20.4",
                    "difficulty": "hard"
                }
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    // Verify the full config round-trips correctly
    assert_eq!(body["server"]["config"]["name"], "Full Wizard Server");
    assert_eq!(body["server"]["config"]["auto_restart"], true);
    assert_eq!(body["server"]["config"]["max_restart_attempts"], 3);
    assert_eq!(body["server"]["config"]["stop_command"], "stop");
    assert_eq!(body["server"]["config"]["stop_timeout_secs"], 30);
    assert_eq!(body["server"]["config"]["sftp_username"], "mc_user");

    assert_eq!(body["server"]["config"]["parameters"].as_array().unwrap().len(), 3);
    assert_eq!(body["server"]["config"]["install_steps"].as_array().unwrap().len(), 2);
    assert_eq!(body["server"]["config"]["update_steps"].as_array().unwrap().len(), 1);

    assert_eq!(body["server"]["parameter_values"]["port"], "25577");
    assert_eq!(body["server"]["parameter_values"]["version"], "1.20.4");
    assert_eq!(body["server"]["parameter_values"]["difficulty"], "hard");

    assert_eq!(body["server"]["installed"], false);

    // Verify we can fetch the server back
    let id = body["server"]["id"].as_str().unwrap();
    let (get_status, get_body) = app.get(&format!("/api/servers/{}", id), Some(&token)).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(get_body["server"]["config"]["name"], "Full Wizard Server");
    assert_eq!(
        get_body["server"]["config"]["install_steps"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(get_body["server"]["parameter_values"]["version"], "1.20.4");
}

#[tokio::test]
async fn test_update_server_parameters_are_revalidated() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a server with a required parameter
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Updateable Server",
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
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "version",
                            "label": "Version",
                            "description": null,
                            "param_type": "string",
                            "default": null,
                            "required": true,
                            "options": [],
                            "regex": null
                        }
                    ],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {
                    "version": "1.0.0"
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let id = body["server"]["id"].as_str().unwrap();

    // Try updating with missing required parameter value
    let (update_status, update_body) = app
        .put(
            &format!("/api/servers/{}", id),
            Some(&token),
            json!({
                "config": {
                    "name": "Updateable Server v2",
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
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "version",
                            "label": "Version",
                            "description": null,
                            "param_type": "string",
                            "default": null,
                            "required": true,
                            "options": [],
                            "regex": null
                        }
                    ],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    assert_eq!(update_status, StatusCode::BAD_REQUEST);
    let err = update_body["error"].as_str().unwrap();
    assert!(
        err.contains("required"),
        "Expected 'required' in update error: {}",
        err
    );

    // Update with valid parameter value should succeed
    let (update_status2, update_body2) = app
        .put(
            &format!("/api/servers/{}", id),
            Some(&token),
            json!({
                "config": {
                    "name": "Updateable Server v2",
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
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "version",
                            "label": "Version",
                            "description": null,
                            "param_type": "string",
                            "default": null,
                            "required": true,
                            "options": [],
                            "regex": null
                        }
                    ],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {
                    "version": "2.0.0"
                }
            }),
        )
        .await;

    assert_eq!(update_status2, StatusCode::OK, "body: {:?}", update_body2);
    assert_eq!(update_body2["server"]["config"]["name"], "Updateable Server v2");
    assert_eq!(update_body2["server"]["parameter_values"]["version"], "2.0.0");
}

#[tokio::test]
async fn test_create_server_with_all_step_action_types() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    // Test that every step action type is accepted and round-trips
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "All Actions Server",
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
                    "sftp_password": null,
                    "parameters": [],
                    "start_steps": [
                        {
                            "name": "Set Env",
                            "description": null,
                            "action": {
                                "type": "set_env",
                                "variables": {
                                    "JAVA_HOME": "/usr/lib/jvm/java-17",
                                    "GAME_MODE": "survival"
                                }
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Set Working Dir",
                            "description": null,
                            "action": {
                                "type": "set_working_dir",
                                "path": "game_root"
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Set Stop Command",
                            "description": null,
                            "action": {
                                "type": "set_stop_command",
                                "command": "stop"
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "install_steps": [
                        {
                            "name": "Download",
                            "description": null,
                            "action": {
                                "type": "download",
                                "url": "https://example.com/file.jar",
                                "destination": ".",
                                "filename": "server.jar",
                                "executable": true
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Extract",
                            "description": null,
                            "action": {
                                "type": "extract",
                                "source": "archive.tar.gz",
                                "destination": "extracted",
                                "format": "tar_gz"
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Move",
                            "description": null,
                            "action": {
                                "type": "move",
                                "source": "old.txt",
                                "destination": "new.txt"
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Copy",
                            "description": null,
                            "action": {
                                "type": "copy",
                                "source": "src",
                                "destination": "dst",
                                "recursive": true
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Delete",
                            "description": null,
                            "action": {
                                "type": "delete",
                                "path": "tmp",
                                "recursive": true
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Create Dir",
                            "description": null,
                            "action": {
                                "type": "create_dir",
                                "path": "new_dir"
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Run Command",
                            "description": null,
                            "action": {
                                "type": "run_command",
                                "command": "echo",
                                "args": ["hello"],
                                "working_dir": null,
                                "env": { "TEST": "1" }
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Write File",
                            "description": null,
                            "action": {
                                "type": "write_file",
                                "path": "test.txt",
                                "content": "hello world"
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Edit File",
                            "description": null,
                            "action": {
                                "type": "edit_file",
                                "path": "test.txt",
                                "operation": {
                                    "type": "find_replace",
                                    "find": "hello",
                                    "replace": "goodbye",
                                    "all": true
                                }
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Set Permissions",
                            "description": null,
                            "action": {
                                "type": "set_permissions",
                                "path": "test.txt",
                                "mode": "755"
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Glob",
                            "description": null,
                            "action": {
                                "type": "glob",
                                "pattern": "server-*.jar",
                                "destination": "server.jar"
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Resolve Build",
                            "description": null,
                            "action": {
                                "type": "resolve_variable",
                                "url": "https://api.example.com/versions",
                                "path": "builds",
                                "pick": "last",
                                "value_key": "build",
                                "variable": "latest_build"
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    let steps = body["server"]["config"]["install_steps"].as_array().unwrap();
    assert_eq!(steps.len(), 12);

    // Verify action types round-trip
    let expected_types = [
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
        "resolve_variable",
    ];
    for (i, expected_type) in expected_types.iter().enumerate() {
        assert_eq!(
            steps[i]["action"]["type"].as_str().unwrap(),
            *expected_type,
            "Step {} action type mismatch",
            i
        );
    }

    // Spot-check a few specific fields
    assert_eq!(steps[0]["action"]["executable"], true);
    assert_eq!(steps[1]["action"]["format"], "tar_gz");
    assert_eq!(steps[3]["action"]["recursive"], true);
    assert_eq!(steps[6]["action"]["env"]["TEST"], "1");
    assert_eq!(steps[8]["action"]["operation"]["type"], "find_replace");
    assert_eq!(steps[8]["action"]["operation"]["find"], "hello");
    assert_eq!(steps[8]["action"]["operation"]["replace"], "goodbye");

    // Verify resolve_variable fields round-trip
    assert_eq!(steps[11]["action"]["type"], "resolve_variable");
    assert_eq!(
        steps[11]["action"]["url"],
        "https://api.example.com/versions"
    );
    assert_eq!(steps[11]["action"]["path"], "builds");
    assert_eq!(steps[11]["action"]["pick"], "last");
    assert_eq!(steps[11]["action"]["value_key"], "build");
    assert_eq!(steps[11]["action"]["variable"], "latest_build");

    // Verify start_steps round-trip with the three config step types
    let start_steps = body["server"]["config"]["start_steps"].as_array().unwrap();
    assert_eq!(start_steps.len(), 3);

    assert_eq!(start_steps[0]["action"]["type"], "set_env");
    assert_eq!(
        start_steps[0]["action"]["variables"]["JAVA_HOME"],
        "/usr/lib/jvm/java-17"
    );
    assert_eq!(
        start_steps[0]["action"]["variables"]["GAME_MODE"],
        "survival"
    );

    assert_eq!(start_steps[1]["action"]["type"], "set_working_dir");
    assert_eq!(start_steps[1]["action"]["path"], "game_root");

    assert_eq!(start_steps[2]["action"]["type"], "set_stop_command");
    assert_eq!(start_steps[2]["action"]["command"], "stop");
}

#[tokio::test]
async fn test_create_server_with_no_parameters_no_steps_works() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    // Basic server with empty parameters and steps (the "basic" mode)
    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Basic Server",
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
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    assert_eq!(body["server"]["config"]["parameters"].as_array().unwrap().len(), 0);
    assert_eq!(body["server"]["config"]["install_steps"].as_array().unwrap().len(), 0);
    assert_eq!(body["server"]["config"]["update_steps"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_create_server_null_working_dir_uses_server_data_dir() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Default WorkDir Server",
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
                },
                "parameter_values": {}
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    assert!(body["server"]["config"]["working_dir"].is_null());
    // The server's data directory should have been created
    let id = body["server"]["id"].as_str().unwrap();
    let (get_status, _) = app.get(&format!("/api/servers/{}", id), Some(&token)).await;
    assert_eq!(get_status, StatusCode::OK);
}

#[tokio::test]
async fn test_create_server_optional_param_can_be_omitted() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Optional Param Server",
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
                    "sftp_password": null,
                    "parameters": [
                        {
                            "name": "motd",
                            "label": "MOTD",
                            "description": null,
                            "param_type": "string",
                            "default": "Default MOTD",
                            "required": false,
                            "options": [],
                            "regex": null
                        },
                        {
                            "name": "debug",
                            "label": "Debug Mode",
                            "description": null,
                            "param_type": "boolean",
                            "default": "false",
                            "required": false,
                            "options": [],
                            "regex": null
                        }
                    ],
                    "install_steps": [],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    // Optional params with no values should be accepted
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    assert_eq!(body["server"]["config"]["parameters"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_create_server_edit_file_operations_round_trip() {
    let app = TestApp::new().await;
    let echo = resolve_binary("echo");
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .post(
            "/api/servers",
            Some(&token),
            json!({
                "config": {
                    "name": "Edit File Ops Server",
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
                    "sftp_password": null,
                    "parameters": [],
                    "install_steps": [
                        {
                            "name": "Overwrite",
                            "description": null,
                            "action": {
                                "type": "edit_file",
                                "path": "test.txt",
                                "operation": { "type": "overwrite", "content": "new content" }
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Append",
                            "description": null,
                            "action": {
                                "type": "edit_file",
                                "path": "test.txt",
                                "operation": { "type": "append", "content": "\nappended" }
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Prepend",
                            "description": null,
                            "action": {
                                "type": "edit_file",
                                "path": "test.txt",
                                "operation": { "type": "prepend", "content": "header\n" }
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Regex Replace",
                            "description": null,
                            "action": {
                                "type": "edit_file",
                                "path": "test.txt",
                                "operation": {
                                    "type": "regex_replace",
                                    "pattern": "v\\d+",
                                    "replace": "v2",
                                    "all": false
                                }
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Insert After",
                            "description": null,
                            "action": {
                                "type": "edit_file",
                                "path": "config.yml",
                                "operation": {
                                    "type": "insert_after",
                                    "pattern": "[server]",
                                    "content": "port=25565"
                                }
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Insert Before",
                            "description": null,
                            "action": {
                                "type": "edit_file",
                                "path": "config.yml",
                                "operation": {
                                    "type": "insert_before",
                                    "pattern": "[end]",
                                    "content": "# footer"
                                }
                            },
                            "condition": null,
                            "continue_on_error": false
                        },
                        {
                            "name": "Replace Line",
                            "description": null,
                            "action": {
                                "type": "edit_file",
                                "path": "config.yml",
                                "operation": {
                                    "type": "replace_line",
                                    "pattern": "old-setting",
                                    "content": "new-setting=value",
                                    "all": true
                                }
                            },
                            "condition": null,
                            "continue_on_error": false
                        }
                    ],
                    "update_steps": []
                },
                "parameter_values": {}
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    let steps = body["server"]["config"]["install_steps"].as_array().unwrap();
    assert_eq!(steps.len(), 7);

    // Verify each edit_file operation type round-trips
    assert_eq!(steps[0]["action"]["operation"]["type"], "overwrite");
    assert_eq!(steps[1]["action"]["operation"]["type"], "append");
    assert_eq!(steps[2]["action"]["operation"]["type"], "prepend");
    assert_eq!(steps[3]["action"]["operation"]["type"], "regex_replace");
    assert_eq!(steps[3]["action"]["operation"]["pattern"], "v\\d+");
    assert_eq!(steps[3]["action"]["operation"]["all"], false);
    assert_eq!(steps[4]["action"]["operation"]["type"], "insert_after");
    assert_eq!(steps[5]["action"]["operation"]["type"], "insert_before");
    assert_eq!(steps[6]["action"]["operation"]["type"], "replace_line");
    assert_eq!(steps[6]["action"]["operation"]["all"], true);
}
