//! End-to-end tests for the `GET /api/templates/fetch-options` endpoint
//! (Ticket 013, Tier 1: API Fetch + Mapping).

use axum::http::StatusCode;
use serde_json::json;

use crate::common::TestApp;

// ─── Authentication ──────────────────────────────────────────────────

#[tokio::test]
async fn fetch_options_requires_auth() {
    let app = TestApp::new().await;

    let (status, body) = app
        .get("/api/templates/fetch-options?url=https://example.com", None)
        .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED, "body: {:?}", body);
}

// ─── Validation ──────────────────────────────────────────────────────

#[tokio::test]
async fn fetch_options_rejects_missing_url() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/templates/fetch-options", Some(&token)).await;

    // Missing required `url` query param → 400 or 422 (Axum query rejection)
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}: {:?}",
        status,
        body
    );
}

#[tokio::test]
async fn fetch_options_rejects_invalid_scheme() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app
        .get(
            "/api/templates/fetch-options?url=ftp://evil.com/data",
            Some(&token),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {:?}", body);
    let err = body["error"].as_str().unwrap_or("");
    assert!(
        err.contains("http or https"),
        "Expected scheme error, got: {}",
        err
    );
}

#[tokio::test]
async fn fetch_options_rejects_invalid_params_json() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // `params` is a JSON string — provide something invalid.
    let (status, body) = app
        .get(
            "/api/templates/fetch-options?url=https://example.com&params=not-json",
            Some(&token),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {:?}", body);
    let err = body["error"].as_str().unwrap_or("");
    assert!(
        err.contains("params"),
        "Expected params parse error, got: {}",
        err
    );
}

// ─── ConfigParameter.options_from serialization ──────────────────────

#[tokio::test]
async fn config_parameter_options_from_round_trips() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Create a template whose parameter has `options_from` set.
    let echo = crate::common::resolve_binary("echo");
    let (status, body) = app
        .post(
            "/api/templates",
            Some(&token),
            json!({
                "name": "Dynamic Opts Test",
                "description": "Template with options_from",
                "config": {
                    "name": "test-server",
                    "binary": echo,
                    "args": [],
                    "parameters": [
                        {
                            "name": "mc_version",
                            "label": "Minecraft Version",
                            "param_type": "string",
                            "required": true,
                            "options": [],
                            "is_version": true,
                            "options_from": {
                                "url": "https://api.papermc.io/v2/projects/paper",
                                "path": "versions",
                                "sort": "desc",
                                "limit": 25,
                                "cache_secs": 300
                            }
                        }
                    ]
                }
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "create template failed: {:?}", body);
    let template_id = body["id"].as_str().unwrap();

    // Fetch it back and check options_from is present.
    let (status, body) = app
        .get(&format!("/api/templates/{}", template_id), Some(&token))
        .await;

    assert_eq!(status, StatusCode::OK, "get template failed: {:?}", body);

    let param = &body["config"]["parameters"][0];
    assert_eq!(param["name"].as_str().unwrap(), "mc_version");

    let opts_from = &param["options_from"];
    assert!(!opts_from.is_null(), "options_from should be present");
    assert_eq!(
        opts_from["url"].as_str().unwrap(),
        "https://api.papermc.io/v2/projects/paper"
    );
    assert_eq!(opts_from["path"].as_str().unwrap(), "versions");
    assert_eq!(opts_from["sort"].as_str().unwrap(), "desc");
    assert_eq!(opts_from["limit"].as_u64().unwrap(), 25);
    assert_eq!(opts_from["cache_secs"].as_u64().unwrap(), 300);
}

#[tokio::test]
async fn config_parameter_options_from_null_by_default() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = crate::common::resolve_binary("echo");
    let (status, body) = app
        .post(
            "/api/templates",
            Some(&token),
            json!({
                "name": "No Dynamic Opts",
                "config": {
                    "name": "test-server",
                    "binary": echo,
                    "args": [],
                    "parameters": [
                        {
                            "name": "memory",
                            "label": "Memory",
                            "param_type": "select",
                            "options": ["2G", "4G"],
                            "required": true
                        }
                    ]
                }
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "create template failed: {:?}", body);
    let template_id = body["id"].as_str().unwrap();

    let (status, body) = app
        .get(&format!("/api/templates/{}", template_id), Some(&token))
        .await;

    assert_eq!(status, StatusCode::OK);
    let param = &body["config"]["parameters"][0];
    assert!(
        param["options_from"].is_null(),
        "options_from should be null when not specified"
    );
}

// ─── Built-in templates include options_from ─────────────────────────

#[tokio::test]
async fn builtin_minecraft_template_has_options_from() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/templates", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let templates = body["templates"].as_array().unwrap();
    let mc = templates
        .iter()
        .find(|t| t["name"].as_str().unwrap().contains("Minecraft Paper"))
        .expect("Minecraft Paper template should exist in builtins");

    assert!(mc["is_builtin"].as_bool().unwrap());

    let mc_version_param = mc["config"]["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"].as_str().unwrap() == "mc_version")
        .expect("mc_version parameter should exist");

    let opts_from = &mc_version_param["options_from"];
    assert!(!opts_from.is_null(), "mc_version should have options_from");
    assert!(
        opts_from["url"].as_str().unwrap().contains("papermc.io"),
        "URL should point to PaperMC API"
    );
    assert_eq!(opts_from["path"].as_str().unwrap(), "versions");
    assert_eq!(opts_from["sort"].as_str().unwrap(), "desc");
}

// ─── OptionsFrom / OptionsSortOrder type generation ──────────────────

#[tokio::test]
async fn options_from_with_all_fields_round_trips() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let echo = crate::common::resolve_binary("echo");
    let (status, body) = app
        .post(
            "/api/templates",
            Some(&token),
            json!({
                "name": "Full Options From",
                "config": {
                    "name": "test",
                    "binary": echo,
                    "args": [],
                    "parameters": [
                        {
                            "name": "runtime",
                            "label": "Runtime",
                            "param_type": "select",
                            "options": [],
                            "options_from": {
                                "url": "https://api.example.com/v1/runtimes",
                                "path": "data.runtimes",
                                "value_key": "id",
                                "label_key": "display_name",
                                "sort": "asc",
                                "limit": 10,
                                "cache_secs": 600
                            }
                        }
                    ]
                }
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    let template_id = body["id"].as_str().unwrap();
    let (status, body) = app
        .get(&format!("/api/templates/{}", template_id), Some(&token))
        .await;

    assert_eq!(status, StatusCode::OK);
    let opts_from = &body["config"]["parameters"][0]["options_from"];
    assert_eq!(
        opts_from["url"].as_str().unwrap(),
        "https://api.example.com/v1/runtimes"
    );
    assert_eq!(opts_from["path"].as_str().unwrap(), "data.runtimes");
    assert_eq!(opts_from["value_key"].as_str().unwrap(), "id");
    assert_eq!(opts_from["label_key"].as_str().unwrap(), "display_name");
    assert_eq!(opts_from["sort"].as_str().unwrap(), "asc");
    assert_eq!(opts_from["limit"].as_u64().unwrap(), 10);
    assert_eq!(opts_from["cache_secs"].as_u64().unwrap(), 600);
}

// ─── Unit-level logic tests (via the public API of fetch_options) ────

#[test]
fn extract_options_string_array() {
    use anyserver::utils::fetch_options::{extract_options, json_navigate};
    use serde_json::json;

    let response = json!({
        "versions": ["1.21.4", "1.21.3", "1.20.6"]
    });

    let navigated = json_navigate(&response, Some("versions")).unwrap();
    let opts = extract_options(navigated, None, None).unwrap();

    assert_eq!(opts.len(), 3);
    assert_eq!(opts[0].value, "1.21.4");
    assert_eq!(opts[0].label, "1.21.4"); // same when no label_key
    assert_eq!(opts[2].value, "1.20.6");
}

#[test]
fn extract_options_object_array_with_keys() {
    use anyserver::utils::fetch_options::{extract_options, json_navigate};
    use serde_json::json;

    let response = json!({
        "data": {
            "runtimes": [
                {"id": "java17", "display_name": "Java 17 (LTS)"},
                {"id": "java21", "display_name": "Java 21 (LTS)"}
            ]
        }
    });

    let navigated = json_navigate(&response, Some("data.runtimes")).unwrap();
    let opts = extract_options(navigated, Some("id"), Some("display_name")).unwrap();

    assert_eq!(opts.len(), 2);
    assert_eq!(opts[0].value, "java17");
    assert_eq!(opts[0].label, "Java 17 (LTS)");
    assert_eq!(opts[1].value, "java21");
    assert_eq!(opts[1].label, "Java 21 (LTS)");
}

#[test]
fn sort_and_limit_works() {
    use anyserver::types::FetchedOption;
    use anyserver::types::OptionsSortOrder;
    use anyserver::utils::fetch_options::sort_and_limit;

    let opts = vec![
        FetchedOption {
            value: "1.20".into(),
            label: "1.20".into(),
        },
        FetchedOption {
            value: "1.21".into(),
            label: "1.21".into(),
        },
        FetchedOption {
            value: "1.19".into(),
            label: "1.19".into(),
        },
        FetchedOption {
            value: "1.18".into(),
            label: "1.18".into(),
        },
    ];

    let result = sort_and_limit(opts, Some(OptionsSortOrder::Desc), Some(2));
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].value, "1.21");
    assert_eq!(result[1].value, "1.20");
}

/// Version-aware sort: "1.9" must sort before "1.20" (numeric comparison,
/// not lexicographic).  This is the core bug that was reported — the old
/// string-based sort put "1.8.x" above "1.21.x".
#[test]
fn sort_and_limit_version_aware_minecraft_versions() {
    use anyserver::types::FetchedOption;
    use anyserver::types::OptionsSortOrder;
    use anyserver::utils::fetch_options::sort_and_limit;

    // Simulate the real PaperMC API response: versions arrive in API order
    // and the sort should produce a correct descending version list.
    let versions = vec![
        "1.8", "1.8.1", "1.8.2", "1.9", "1.9.1", "1.10", "1.11", "1.12", "1.13", "1.14", "1.15",
        "1.16", "1.17", "1.18", "1.19", "1.19.1", "1.20", "1.20.1", "1.20.4", "1.20.6", "1.21",
        "1.21.1", "1.21.2", "1.21.3", "1.21.4",
    ];
    let opts: Vec<FetchedOption> = versions
        .into_iter()
        .map(|v| FetchedOption {
            value: v.into(),
            label: v.into(),
        })
        .collect();

    let result = sort_and_limit(opts, Some(OptionsSortOrder::Desc), Some(10));
    let values: Vec<&str> = result.iter().map(|o| o.value.as_str()).collect();
    assert_eq!(
        values,
        vec![
            "1.21.4", "1.21.3", "1.21.2", "1.21.1", "1.21", "1.20.6", "1.20.4", "1.20.1", "1.20",
            "1.19.1",
        ],
        "Versions should be sorted numerically per segment, not lexicographically"
    );
}

/// When values are a mix of version-like ("1.20", "2.0") and
/// non-version-like ("snapshot-abc", "beta"), the version-like entries
/// should appear first and the non-version entries should be placed after.
#[test]
fn sort_and_limit_mixed_version_and_non_version() {
    use anyserver::types::FetchedOption;
    use anyserver::types::OptionsSortOrder;
    use anyserver::utils::fetch_options::sort_and_limit;

    let opts = vec![
        FetchedOption {
            value: "snapshot-1".into(),
            label: "snapshot-1".into(),
        },
        FetchedOption {
            value: "1.20".into(),
            label: "1.20".into(),
        },
        FetchedOption {
            value: "beta-2".into(),
            label: "beta-2".into(),
        },
        FetchedOption {
            value: "1.9".into(),
            label: "1.9".into(),
        },
        FetchedOption {
            value: "2.0".into(),
            label: "2.0".into(),
        },
    ];

    // Ascending: version-like first (ascending), then non-version (ascending)
    let asc = sort_and_limit(opts.clone(), Some(OptionsSortOrder::Asc), None);
    let asc_vals: Vec<&str> = asc.iter().map(|o| o.value.as_str()).collect();
    assert_eq!(
        asc_vals,
        vec!["1.9", "1.20", "2.0", "beta-2", "snapshot-1"],
        "Asc: version-like entries first, non-version after"
    );

    // Descending: version-like first (descending), then non-version (descending)
    let desc = sort_and_limit(opts, Some(OptionsSortOrder::Desc), None);
    let desc_vals: Vec<&str> = desc.iter().map(|o| o.value.as_str()).collect();
    assert_eq!(
        desc_vals,
        vec!["2.0", "1.20", "1.9", "snapshot-1", "beta-2"],
        "Desc: version-like entries first (descending), non-version after (descending)"
    );
}

/// version_cmp is publicly exported — verify it directly for edge cases.
#[test]
fn version_cmp_direct() {
    use anyserver::utils::fetch_options::version_cmp;
    use std::cmp::Ordering;

    assert_eq!(version_cmp("1.9", "1.20"), Ordering::Less);
    assert_eq!(version_cmp("1.21.4", "1.21.3"), Ordering::Greater);
    assert_eq!(version_cmp("1.21", "1.21"), Ordering::Equal);
    assert_eq!(
        version_cmp("1.21", "1.21.0"),
        Ordering::Less,
        "shorter < longer when prefix equal"
    );
    assert_eq!(version_cmp("2.0", "1.99"), Ordering::Greater);
    assert_eq!(version_cmp("10", "9"), Ordering::Greater);
    // Non-numeric segment falls back to lexicographic
    assert_eq!(version_cmp("1.0a", "1.0b"), Ordering::Less);
}

#[test]
fn substitute_template_vars_replaces_double_braces() {
    use anyserver::utils::fetch_options::substitute_template_vars;
    use std::collections::HashMap;

    let mut vars = HashMap::new();
    vars.insert("version".into(), "1.21.4".into());
    vars.insert("project".into(), "paper".into());

    let result = substitute_template_vars(
        "https://api.papermc.io/v2/projects/{{project}}/versions/{{version}}/builds",
        &vars,
    );
    assert_eq!(
        result,
        "https://api.papermc.io/v2/projects/paper/versions/1.21.4/builds"
    );
}

#[test]
fn substitute_template_vars_no_op_without_vars() {
    use anyserver::utils::fetch_options::substitute_template_vars;
    use std::collections::HashMap;

    let vars = HashMap::new();
    let result = substitute_template_vars("https://example.com/data", &vars);
    assert_eq!(result, "https://example.com/data");
}

#[test]
fn json_navigate_returns_root_for_no_path() {
    use anyserver::utils::fetch_options::json_navigate;
    use serde_json::json;

    let val = json!(["a", "b", "c"]);
    assert_eq!(json_navigate(&val, None), Some(&val));
    assert_eq!(json_navigate(&val, Some("")), Some(&val));
}

#[test]
fn json_navigate_nested_path() {
    use anyserver::utils::fetch_options::json_navigate;
    use serde_json::json;

    let val = json!({"a": {"b": {"c": 42}}});
    assert_eq!(json_navigate(&val, Some("a.b.c")), Some(&json!(42)));
}

#[test]
fn json_navigate_missing_returns_none() {
    use anyserver::utils::fetch_options::json_navigate;
    use serde_json::json;

    let val = json!({"a": 1});
    assert_eq!(json_navigate(&val, Some("b")), None);
    assert_eq!(json_navigate(&val, Some("a.b")), None);
}

#[test]
fn extract_options_non_array_returns_error() {
    use anyserver::utils::fetch_options::extract_options;
    use serde_json::json;

    let val = json!("hello");
    let err = extract_options(&val, None, None).unwrap_err();
    assert!(err.contains("Expected a JSON array"));
}

#[test]
fn extract_options_empty_array() {
    use anyserver::utils::fetch_options::extract_options;
    use serde_json::json;

    let val = json!([]);
    let opts = extract_options(&val, None, None).unwrap();
    assert!(opts.is_empty());
}

#[test]
fn extract_options_with_missing_value_key_errors() {
    use anyserver::utils::fetch_options::extract_options;
    use serde_json::json;

    let val = json!([{"name": "x"}]);
    let err = extract_options(&val, Some("id"), None).unwrap_err();
    assert!(err.contains("does not have key 'id'"));
}

#[test]
fn extract_options_numeric_values() {
    use anyserver::utils::fetch_options::extract_options;
    use serde_json::json;

    let val = json!([
        {"build": 100, "channel": "stable"},
        {"build": 101, "channel": "beta"}
    ]);
    let opts = extract_options(&val, Some("build"), Some("channel")).unwrap();
    assert_eq!(opts[0].value, "100");
    assert_eq!(opts[0].label, "stable");
    assert_eq!(opts[1].value, "101");
    assert_eq!(opts[1].label, "beta");
}

#[test]
fn extract_options_label_key_fallback() {
    use anyserver::utils::fetch_options::extract_options;
    use serde_json::json;

    // When label_key is specified but missing from a particular object,
    // it should fall back to the value.
    let val = json!([
        {"id": "a"},
        {"id": "b", "label": "B Label"}
    ]);
    let opts = extract_options(&val, Some("id"), Some("label")).unwrap();
    assert_eq!(opts[0].value, "a");
    assert_eq!(opts[0].label, "a"); // fallback
    assert_eq!(opts[1].value, "b");
    assert_eq!(opts[1].label, "B Label");
}

#[tokio::test]
async fn fetch_and_extract_rejects_non_http_scheme() {
    use anyserver::utils::fetch_options::fetch_and_extract;
    use std::collections::HashMap;

    let client = reqwest::Client::new();
    let vars = HashMap::new();
    let err = fetch_and_extract(
        &client,
        "ftp://evil.com/data",
        None,
        None,
        None,
        None,
        None,
        &vars,
    )
    .await
    .unwrap_err();
    assert!(err.contains("http or https"));
}

#[tokio::test]
async fn fetch_and_extract_rejects_schemeless_url() {
    use anyserver::utils::fetch_options::fetch_and_extract;
    use std::collections::HashMap;

    let client = reqwest::Client::new();
    let vars = HashMap::new();
    let err = fetch_and_extract(&client, "not-a-url", None, None, None, None, None, &vars)
        .await
        .unwrap_err();
    assert!(err.contains("http or https"));
}

// ─── Response format ─────────────────────────────────────────────────

#[tokio::test]
async fn fetch_options_response_shape() {
    // We can't easily mock the external HTTP call in an e2e test,
    // but we can at least verify that the endpoint returns a proper
    // error structure when the URL is unreachable.
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Use a URL that will fail to connect (RFC 5737 test network).
    let (status, body) = app
        .get(
            "/api/templates/fetch-options?url=https://192.0.2.1/data",
            Some(&token),
        )
        .await;

    // Should be 400 (bad request) with an error message.
    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {:?}", body);
    assert!(
        body["error"].is_string(),
        "Response should have an 'error' field"
    );
}

// ─── Route is accessible ─────────────────────────────────────────────

#[tokio::test]
async fn fetch_options_route_exists() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    // Even though this URL won't resolve, the route should be found (not 404).
    let (status, _body) = app
        .get(
            "/api/templates/fetch-options?url=https://localhost:1/nope",
            Some(&token),
        )
        .await;

    // Should be 400 (connection error), NOT 404 (route not found).
    assert_ne!(
        status,
        StatusCode::NOT_FOUND,
        "fetch-options route should exist"
    );
}
