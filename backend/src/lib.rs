use std::sync::OnceLock;

use axum::http::header::{HeaderName, HeaderValue};
use axum::response::Response;

pub mod api;
pub mod auth;
pub mod auth_system;
pub mod error;
pub mod integrations;
pub mod monitoring;
pub mod pipeline;
pub mod sandbox;
pub mod security;
pub mod server_management;
pub mod sftp_server;
pub mod storage;
pub mod templates;
pub mod types;
pub mod utils;

#[cfg(feature = "bundle-frontend")]
pub mod frontend;

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::Router;
use dashmap::DashMap;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

/// Headers to allow explicitly when `allow_credentials(true)` is active.
/// `AllowHeaders::any()` expands to `*` which tower-http rejects when
/// combined with `Access-Control-Allow-Credentials: true`.
fn credentialed_allow_headers() -> AllowHeaders {
    AllowHeaders::list([
        axum::http::header::CONTENT_TYPE,
        axum::http::header::AUTHORIZATION,
        axum::http::header::ACCEPT,
        axum::http::header::ORIGIN,
        axum::http::header::CACHE_CONTROL,
        HeaderName::from_static("x-requested-with"),
    ])
}

/// Methods to allow explicitly when `allow_credentials(true)` is active.
/// `AllowMethods::any()` expands to `*` which tower-http rejects when
/// combined with `Access-Control-Allow-Credentials: true`.
fn credentialed_allow_methods() -> AllowMethods {
    AllowMethods::list([
        axum::http::Method::GET,
        axum::http::Method::POST,
        axum::http::Method::PUT,
        axum::http::Method::DELETE,
        axum::http::Method::PATCH,
        axum::http::Method::OPTIONS,
    ])
}
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use types::UpdateCheckResult;

pub struct AppState {
    pub db: storage::Database,
    pub process_manager: server_management::ProcessManager,
    pub pipeline_manager: pipeline::PipelineManager,
    pub data_dir: PathBuf,
    pub http_client: reqwest::Client,
    /// Kept across requests so CPU-usage deltas are meaningful (sysinfo
    /// needs two successive refreshes to compute percentages).
    pub system_monitor: parking_lot::Mutex<sysinfo::System>,
    pub stats_collector: std::sync::Arc<server_management::StatsCollector>,
    pub update_cache: DashMap<Uuid, UpdateCheckResult>,
    pub alert_dispatcher: monitoring::AlertDispatcher,
    pub ws_ticket_store: auth_system::WsTicketStore,
    pub login_attempt_tracker: auth_system::LoginAttemptTracker,
}

impl AppState {
    pub fn server_dir(&self, server_id: &uuid::Uuid) -> PathBuf {
        self.data_dir.join("servers").join(server_id.to_string())
    }
}

fn build_cors_layer() -> CorsLayer {
    if let Ok(raw) = std::env::var("ANYSERVER_CORS_ORIGIN") {
        let trimmed_raw = raw.trim();

        // Explicit wildcard: ANYSERVER_CORS_ORIGIN=*
        if trimmed_raw == "*" {
            tracing::warn!(
                "CORS: ANYSERVER_CORS_ORIGIN=* — allowing ANY origin. \
                 This is not recommended for production."
            );
            return CorsLayer::new()
                .allow_origin(AllowOrigin::any())
                .allow_methods(AllowMethods::any())
                .allow_headers(AllowHeaders::any());
        }

        let origins: Vec<_> = raw
            .split(',')
            .filter_map(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    trimmed.parse().ok()
                }
            })
            .collect();

        if origins.is_empty() {
            tracing::warn!(
                "ANYSERVER_CORS_ORIGIN is set but contains no valid origins — falling back to restrictive CORS"
            );
            CorsLayer::new()
                .allow_origin(AllowOrigin::exact("http://localhost:3000".parse().unwrap()))
                .allow_methods(credentialed_allow_methods())
                .allow_headers(credentialed_allow_headers())
                .allow_credentials(true)
        } else {
            tracing::info!("CORS: allowing origins {:?}", origins);
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(origins))
                .allow_methods(credentialed_allow_methods())
                .allow_headers(credentialed_allow_headers())
                .allow_credentials(true)
        }
    } else {
        #[cfg(feature = "bundle-frontend")]
        {
            tracing::info!(
                "CORS: bundle-frontend enabled — restricting to same-origin. \
                     Set ANYSERVER_CORS_ORIGIN to allow external origins."
            );
            CorsLayer::new()
                .allow_methods(AllowMethods::any())
                .allow_headers(AllowHeaders::any())
        }
        #[cfg(not(feature = "bundle-frontend"))]
        {
            tracing::info!(
                "CORS: dev mode — allowing http://localhost:3000. \
                 Set ANYSERVER_CORS_ORIGIN to override."
            );
            CorsLayer::new()
                .allow_origin(AllowOrigin::exact("http://localhost:3000".parse().unwrap()))
                .allow_methods(credentialed_allow_methods())
                .allow_headers(credentialed_allow_headers())
                .allow_credentials(true)
        }
    }
}

const GLOBAL_BODY_LIMIT: usize = 16 * 1024 * 1024; // 16 MB

pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = build_cors_layer();

    let router = Router::new()
        .nest("/api", api::router())
        .layer(DefaultBodyLimit::max(GLOBAL_BODY_LIMIT))
        .layer(cors)
        .layer(axum::middleware::map_response(add_security_headers))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    #[cfg(feature = "bundle-frontend")]
    let router = router.fallback(axum::routing::get(frontend::static_handler));

    router
}

/// The default Content-Security-Policy applied when `bundle-frontend` is
/// enabled and no `ANYSERVER_CSP` override is set.
#[cfg(feature = "bundle-frontend")]
const DEFAULT_CSP: &str = "default-src 'self'; \
    script-src 'self'; \
    style-src 'self' 'unsafe-inline'; \
    connect-src 'self' wss:; \
    img-src 'self' data:; \
    font-src 'self'; \
    object-src 'none'; \
    base-uri 'self'; \
    form-action 'self'; \
    frame-ancestors 'none'";

/// Returns the CSP header value to use, or `None` to omit the header.
///
/// Resolution order:
/// 1. `ANYSERVER_CSP` env var — if set to a non-empty value, use it;
///    if set to an empty string, disable CSP entirely.
/// 2. When `bundle-frontend` is enabled, use [`DEFAULT_CSP`].
/// 3. In dev mode (no `bundle-frontend`), omit the header so that
///    Vite HMR / dev-server injected scripts are not blocked.
fn csp_header_value() -> Option<&'static str> {
    static CSP: OnceLock<Option<String>> = OnceLock::new();

    CSP.get_or_init(|| match std::env::var("ANYSERVER_CSP") {
        Ok(val) if val.is_empty() => {
            tracing::info!("CSP: disabled via empty ANYSERVER_CSP env var");
            None
        }
        Ok(val) => {
            tracing::info!("CSP: using custom policy from ANYSERVER_CSP");
            Some(val)
        }
        Err(_) => {
            #[cfg(feature = "bundle-frontend")]
            {
                tracing::info!(
                    "CSP: bundle-frontend enabled — applying default Content-Security-Policy"
                );
                Some(DEFAULT_CSP.to_owned())
            }
            #[cfg(not(feature = "bundle-frontend"))]
            {
                tracing::debug!(
                    "CSP: dev mode — omitting Content-Security-Policy. \
                         Set ANYSERVER_CSP to apply a policy in dev mode."
                );
                None
            }
        }
    })
    .as_deref()
}

async fn add_security_headers(mut response: Response) -> Response {
    const SECURITY_HEADERS: &[(&str, &str)] = &[
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "strict-origin-when-cross-origin"),
        ("x-xss-protection", "0"),
        (
            "permissions-policy",
            "camera=(), microphone=(), geolocation=(), payment=(), usb=()",
        ),
    ];

    let headers = response.headers_mut();
    for &(name, value) in SECURITY_HEADERS {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }

    if let Some(csp) = csp_header_value() {
        if let Ok(val) = HeaderValue::from_str(csp) {
            headers.insert(HeaderName::from_static("content-security-policy"), val);
        }
    }

    response
}
