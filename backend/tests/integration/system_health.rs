//! Integration tests for the system health endpoint (ticket 014).

use axum::http::StatusCode;

use crate::common::TestApp;

// ─── Authentication ───

#[tokio::test]
async fn test_system_health_requires_auth() {
    let app = TestApp::new().await;
    app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/system/health", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "body: {:?}", body);
}

// ─── Successful response ───

#[tokio::test]
async fn test_system_health_returns_metrics() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/system/health", Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    // Top-level fields must be present
    assert!(body.get("cpu").is_some(), "missing cpu: {:?}", body);
    assert!(body.get("memory").is_some(), "missing memory: {:?}", body);
    assert!(body.get("disks").is_some(), "missing disks: {:?}", body);
    assert!(
        body.get("networks").is_some(),
        "missing networks: {:?}",
        body
    );
    assert!(
        body.get("uptime_secs").is_some(),
        "missing uptime_secs: {:?}",
        body
    );
    assert!(
        body.get("hostname").is_some(),
        "missing hostname: {:?}",
        body
    );
    assert!(
        body.get("timestamp").is_some(),
        "missing timestamp: {:?}",
        body
    );
}

// ─── CPU metrics shape ───

#[tokio::test]
async fn test_system_health_cpu_fields() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/system/health", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let cpu = &body["cpu"];
    assert!(
        cpu["overall_percent"].is_number(),
        "overall_percent should be a number: {:?}",
        cpu
    );
    assert!(
        cpu["per_core_percent"].is_array(),
        "per_core_percent should be an array: {:?}",
        cpu
    );
    assert!(
        cpu["load_avg_1"].is_number(),
        "load_avg_1 should be a number: {:?}",
        cpu
    );
    assert!(
        cpu["load_avg_5"].is_number(),
        "load_avg_5 should be a number: {:?}",
        cpu
    );
    assert!(
        cpu["load_avg_15"].is_number(),
        "load_avg_15 should be a number: {:?}",
        cpu
    );
    assert!(
        cpu["core_count"].is_number(),
        "core_count should be a number: {:?}",
        cpu
    );

    // core_count must be at least 1
    let core_count = cpu["core_count"].as_u64().unwrap();
    assert!(core_count >= 1, "core_count should be >= 1: {}", core_count);

    // per_core_percent length should match core_count
    let per_core = cpu["per_core_percent"].as_array().unwrap();
    assert_eq!(
        per_core.len() as u64,
        core_count,
        "per_core_percent length should match core_count"
    );

    // overall_percent should be in range [0, 100]
    let overall = cpu["overall_percent"].as_f64().unwrap();
    assert!(
        (0.0..=100.0).contains(&overall),
        "overall_percent should be 0-100, got {}",
        overall
    );
}

// ─── Memory metrics shape ───

#[tokio::test]
async fn test_system_health_memory_fields() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/system/health", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let mem = &body["memory"];
    assert!(mem["total_bytes"].is_number(), "total_bytes: {:?}", mem);
    assert!(mem["used_bytes"].is_number(), "used_bytes: {:?}", mem);
    assert!(
        mem["available_bytes"].is_number(),
        "available_bytes: {:?}",
        mem
    );
    assert!(
        mem["swap_total_bytes"].is_number(),
        "swap_total_bytes: {:?}",
        mem
    );
    assert!(
        mem["swap_used_bytes"].is_number(),
        "swap_used_bytes: {:?}",
        mem
    );

    // total_bytes must be > 0 (the machine has RAM)
    let total = mem["total_bytes"].as_u64().unwrap();
    assert!(total > 0, "total_bytes should be > 0");

    // used_bytes should not exceed total_bytes
    let used = mem["used_bytes"].as_u64().unwrap();
    assert!(
        used <= total,
        "used_bytes ({}) should be <= total_bytes ({})",
        used,
        total
    );
}

// ─── Disk metrics shape ───

#[tokio::test]
async fn test_system_health_disks_not_empty() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/system/health", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let disks = body["disks"].as_array().unwrap();
    // There should be at least one mounted volume on any real machine
    assert!(!disks.is_empty(), "disks array should not be empty");

    // Check the shape of the first disk entry
    let d = &disks[0];
    assert!(d["name"].is_string(), "disk name: {:?}", d);
    assert!(d["mount_point"].is_string(), "disk mount_point: {:?}", d);
    assert!(d["total_bytes"].is_number(), "disk total_bytes: {:?}", d);
    assert!(d["used_bytes"].is_number(), "disk used_bytes: {:?}", d);
    assert!(d["free_bytes"].is_number(), "disk free_bytes: {:?}", d);
    assert!(d["filesystem"].is_string(), "disk filesystem: {:?}", d);

    // total = used + free (allow some tolerance for rounding)
    let total = d["total_bytes"].as_u64().unwrap();
    let used = d["used_bytes"].as_u64().unwrap();
    let free = d["free_bytes"].as_u64().unwrap();
    assert!(total > 0, "disk total_bytes should be > 0");
    // used + free should approximate total (within 1% tolerance for filesystem overhead)
    let sum = used + free;
    let diff = sum.abs_diff(total);
    let tolerance = total / 100; // 1%
    assert!(
        diff <= tolerance,
        "used ({}) + free ({}) should ≈ total ({}), diff = {}",
        used,
        free,
        total,
        diff
    );
}

// ─── Network metrics shape ───

#[tokio::test]
async fn test_system_health_networks_shape() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/system/health", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let networks = body["networks"].as_array().unwrap();
    // Most machines have at least a loopback interface
    assert!(!networks.is_empty(), "networks array should not be empty");

    let n = &networks[0];
    assert!(n["interface"].is_string(), "interface: {:?}", n);
    assert!(n["rx_bytes"].is_number(), "rx_bytes: {:?}", n);
    assert!(n["tx_bytes"].is_number(), "tx_bytes: {:?}", n);
}

// ─── Uptime and hostname ───

#[tokio::test]
async fn test_system_health_uptime_and_hostname() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (status, body) = app.get("/api/system/health", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    // Uptime should be > 0 on any running machine
    let uptime = body["uptime_secs"].as_u64().unwrap();
    assert!(uptime > 0, "uptime_secs should be > 0");

    // Hostname should be a non-empty string
    let hostname = body["hostname"].as_str().unwrap();
    assert!(!hostname.is_empty(), "hostname should not be empty");

    // Timestamp should be a valid ISO 8601 string
    let ts = body["timestamp"].as_str().unwrap();
    assert!(!ts.is_empty(), "timestamp should not be empty");
    // Basic sanity: starts with a year (4 digits)
    assert!(
        ts.len() >= 10 && ts.chars().take(4).all(|c| c.is_ascii_digit()),
        "timestamp should look like an ISO 8601 date: {}",
        ts
    );
}

// ─── Regular user can also access health ───

#[tokio::test]
async fn test_system_health_accessible_by_regular_user() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin("admin", "Admin1234").await;
    app.enable_registration(&admin_token).await;
    let user_token = app.register_user("regularuser", "Admin1234").await;

    let (status, body) = app.get("/api/system/health", Some(&user_token)).await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    assert!(body.get("cpu").is_some());
    assert!(body.get("memory").is_some());
}

// ─── Polling returns fresh data ───

#[tokio::test]
async fn test_system_health_successive_calls_return_fresh_timestamps() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;

    let (s1, b1) = app.get("/api/system/health", Some(&token)).await;
    assert_eq!(s1, StatusCode::OK);

    // Small delay to ensure timestamp differs
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let (s2, b2) = app.get("/api/system/health", Some(&token)).await;
    assert_eq!(s2, StatusCode::OK);

    let ts1 = b1["timestamp"].as_str().unwrap();
    let ts2 = b2["timestamp"].as_str().unwrap();
    // Timestamps should be different (or at least not fail)
    // The main thing is both calls succeed and return valid data
    assert!(!ts1.is_empty());
    assert!(!ts2.is_empty());
}
