use std::process::Command as StdCommand;
use std::sync::Arc;

use anyserver::server_management::stats::StatsCollector;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

use anyserver::{build_router, server_management::ProcessManager, storage::Database, AppState};

/// Resolve the absolute path for a command name by searching PATH.
/// Falls back to `/usr/bin/env <name>` wrapper approach if `which` fails.
pub fn resolve_binary(name: &str) -> String {
    // Try `which` first
    if let Ok(output) = StdCommand::new("which").arg(name).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return path;
            }
        }
    }
    // Fallback: common paths
    for prefix in &["/usr/bin", "/bin", "/run/current-system/sw/bin"] {
        let candidate = format!("{}/{}", prefix, name);
        if std::path::Path::new(&candidate).exists() {
            return candidate;
        }
    }
    // Last resort — just return the name and hope it's on PATH
    name.to_string()
}

/// A self-contained test environment with its own database, temp dir, and router.
pub struct TestApp {
    pub router: Router,
    pub state: Arc<AppState>,
    pub _temp_dir: TempDir, // dropped (and cleaned up) at end of test
}

impl TestApp {
    /// Create a fresh test environment.
    pub async fn new() -> Self {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let data_dir = temp_dir.path().to_path_buf();
        std::fs::create_dir_all(data_dir.join("servers")).unwrap();

        // Initialise the JWT secret so token creation/validation works in tests.
        // Uses a OnceLock internally — the first test to run wins, which is fine
        // because all tests share the same process and the secret value doesn't
        // matter as long as it's consistent within a test run.
        anyserver::auth::init_jwt_secret(&data_dir);

        let db = Database::open(data_dir.join("anyserver.db"))
            .await
            .expect("failed to open database");
        let process_manager = ProcessManager::new();
        let pipeline_manager = anyserver::pipeline::PipelineManager::new();

        let mut sys = sysinfo::System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory();

        let stats_collector = Arc::new(StatsCollector::new());

        let http_client = anyserver::security::ssrf::build_ssrf_safe_client()
            .timeout(std::time::Duration::from_secs(15))
            .connect_timeout(std::time::Duration::from_secs(15))
            .read_timeout(std::time::Duration::from_secs(30))
            .user_agent("AnyServer-Test/1.0")
            .build()
            .expect("failed to build test HTTP client");

        let state = Arc::new(AppState {
            db,
            process_manager,
            pipeline_manager,
            data_dir,
            http_client,
            system_monitor: parking_lot::Mutex::new(sys),
            stats_collector,
            update_cache: dashmap::DashMap::new(),
            alert_dispatcher: anyserver::monitoring::alerts::AlertDispatcher::new(),
            ws_ticket_store: anyserver::auth_system::ws_ticket::WsTicketStore::new(),
            login_attempt_tracker: anyserver::auth_system::lockout::LoginAttemptTracker::new(),
        });

        let router = build_router(Arc::clone(&state));

        Self {
            router,
            state,
            _temp_dir: temp_dir,
        }
    }

    /// Send a request through the router and return the raw response.
    /// Useful when you need to inspect headers.
    pub async fn raw_request(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> axum::response::Response {
        let body_str = body.map(|v| v.to_string()).unwrap_or_default();

        let method = method.parse::<Method>().expect("invalid method");
        let mut builder = Request::builder().method(method).uri(uri);

        builder = builder.header(header::CONTENT_TYPE, "application/json");

        if let Some(t) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {}", t));
        }

        let req = builder
            .body(Body::from(body_str))
            .expect("failed to build request");

        self.router
            .clone()
            .oneshot(req)
            .await
            .expect("request failed")
    }

    /// Send a request through the router and return (status, body as Value).
    pub async fn request(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let body_str = body.map(|v| v.to_string()).unwrap_or_default();

        let mut builder = Request::builder().method(method).uri(uri);

        builder = builder.header(header::CONTENT_TYPE, "application/json");

        if let Some(t) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {}", t));
        }

        let req = builder
            .body(Body::from(body_str))
            .expect("failed to build request");

        let resp = self
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("request failed");

        let status = resp.status();
        let bytes = resp
            .into_body()
            .collect()
            .await
            .expect("failed to collect body")
            .to_bytes();

        let value: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).to_string()))
        };

        (status, value)
    }

    // ─── Convenience methods ──────────────────────────────────────────────

    pub async fn get(&self, uri: &str, token: Option<&str>) -> (StatusCode, Value) {
        self.request(Method::GET, uri, token, None).await
    }

    pub async fn post(&self, uri: &str, token: Option<&str>, body: Value) -> (StatusCode, Value) {
        self.request(Method::POST, uri, token, Some(body)).await
    }

    pub async fn put(&self, uri: &str, token: Option<&str>, body: Value) -> (StatusCode, Value) {
        self.request(Method::PUT, uri, token, Some(body)).await
    }

    pub async fn delete(&self, uri: &str, token: Option<&str>) -> (StatusCode, Value) {
        self.request(Method::DELETE, uri, token, None).await
    }

    // ─── Higher-level helpers ─────────────────────────────────────────────

    /// Run the first-time setup to create an admin user. Returns the JWT.
    /// The standard test password that meets the password policy
    /// (≥8 chars, uppercase, lowercase, digit).
    pub const TEST_PASSWORD: &'static str = "Admin1234";

    pub async fn setup_admin(&self, username: &str, password: &str) -> String {
        let (status, body) = self
            .post(
                "/api/auth/setup",
                None,
                json!({ "username": username, "password": password }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "setup_admin failed: {:?}", body);
        body["token"].as_str().unwrap().to_string()
    }

    /// Log in and return the JWT.
    #[allow(dead_code)]
    pub async fn login(&self, username: &str, password: &str) -> String {
        let (status, body) = self
            .post(
                "/api/auth/login",
                None,
                json!({ "username": username, "password": password }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "login failed: {:?}", body);
        body["token"].as_str().unwrap().to_string()
    }

    /// Enable registration (requires admin token).
    pub async fn enable_registration(&self, admin_token: &str) {
        let (status, _) = self
            .put(
                "/api/auth/settings",
                Some(admin_token),
                json!({
                    "registration_enabled": true,
                    "allow_run_commands": false,
                    "run_command_sandbox": "auto",
                    "run_command_default_timeout_secs": 300,
                    "run_command_use_namespaces": true
                }),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }

    /// Register a new user (registration must be enabled). Returns the JWT.
    pub async fn register_user(&self, username: &str, password: &str) -> String {
        let (status, body) = self
            .post(
                "/api/auth/register",
                None,
                json!({ "username": username, "password": password }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "register failed: {:?}", body);
        body["token"].as_str().unwrap().to_string()
    }

    /// Create a minimal server config and return (server_id, body).
    pub async fn create_test_server(&self, token: &str, name: &str) -> (String, Value) {
        let echo = resolve_binary("echo");
        let (status, body) = self
            .post(
                "/api/servers",
                Some(token),
                json!({
                    "config": {
                        "name": name,
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
        assert_eq!(status, StatusCode::OK, "create_server failed: {:?}", body);
        let id = body["server"]["id"].as_str().unwrap().to_string();
        (id, body)
    }

    /// Get the admin token (assumes admin has already been set up).
    #[allow(dead_code)]
    pub async fn admin_token(&self) -> String {
        self.login("admin", Self::TEST_PASSWORD).await
    }

    /// Set up admin + enable registration + create a regular user. Returns (admin_token, user_token, user_id).
    pub async fn setup_admin_and_user(&self) -> (String, String, String) {
        let admin_token = self.setup_admin("admin", Self::TEST_PASSWORD).await;
        self.enable_registration(&admin_token).await;
        let user_token = self.register_user("regularuser", Self::TEST_PASSWORD).await;

        // Get user ID from /auth/me
        let (_, me) = self.get("/api/auth/me", Some(&user_token)).await;
        let user_id = me["user"]["id"].as_str().unwrap().to_string();

        (admin_token, user_token, user_id)
    }

    /// Poll `/api/servers/:id/phase-status` until the pipeline is no longer running,
    /// or until the timeout is reached. Returns the final phase-status body.
    ///
    /// This uses a 200ms base polling interval with jitter to reduce resource
    /// contention when many tests run in parallel.
    pub async fn poll_phase_complete(&self, token: &str, server_id: &str) -> Value {
        use std::time::Duration;
        let url = format!("/api/servers/{}/phase-status", server_id);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);

        loop {
            let (status, body) = self.get(&url, Some(token)).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "phase-status request failed: {:?}",
                body
            );

            if let Some(progress) = body.get("progress") {
                let phase_status = progress["status"].as_str().unwrap_or("");
                if phase_status != "running" {
                    // Pipeline completed - poll to ensure DB writes have persisted
                    // The background task updates the DB asynchronously, so we poll
                    // a few times with a short delay to ensure consistency
                    return self.await_db_consistency(&url, token).await;
                }
            }

            if tokio::time::Instant::now() >= deadline {
                panic!("Pipeline timed out after 30s. Last status: {:?}", body);
            }

            // Sleep 200ms with +/- 50ms jitter to prevent thundering herd
            // when many tests poll simultaneously
            let jitter = (rand::random::<u64>() % 100) as i64 - 50;
            let sleep_ms = 200 + jitter;
            tokio::time::sleep(Duration::from_millis(sleep_ms as u64)).await;
        }
    }

    /// Wait for database consistency after pipeline completion.
    ///
    /// When a pipeline completes, the background task updates the database
    /// asynchronously. We poll a few times to ensure those writes have settled.
    async fn await_db_consistency(&self, url: &str, token: &str) -> Value {
        use std::time::Duration;

        // Poll up to 3 times with 50ms delays to catch DB writes
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            let (status, body) = self.get(url, Some(token)).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "DB consistency check failed: {:?}",
                body
            );

            // On the last attempt, return whatever we got
            if attempt == 2 {
                return body;
            }
        }

        unreachable!()
    }
}
