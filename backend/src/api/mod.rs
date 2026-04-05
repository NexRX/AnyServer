pub mod alerts;
pub mod auth;
pub mod curseforge;
pub mod files;
pub mod github;
pub mod import;
pub mod invite_codes;
pub mod permissions;
pub mod pipeline;
pub mod sandbox;
pub mod servers;
pub mod steamcmd;
pub mod system;
pub mod templates;
pub mod update_check;
pub mod users;
pub mod ws;

use std::sync::Arc;
use std::time::Duration;

use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::security::RateLimitLayer;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    // ── Rate-limit tiers ────────────────────────────────────────────────
    let auth_rate_limit = RateLimitLayer::new(10, Duration::from_secs(60));
    crate::security::spawn_eviction_task(auth_rate_limit.state(), Duration::from_secs(300));

    let lifecycle_rate_limit = RateLimitLayer::new(30, Duration::from_secs(60));
    crate::security::spawn_eviction_task(lifecycle_rate_limit.state(), Duration::from_secs(300));

    let pipeline_rate_limit = RateLimitLayer::new(5, Duration::from_secs(60));
    crate::security::spawn_eviction_task(pipeline_rate_limit.state(), Duration::from_secs(300));

    let file_mutation_rate_limit = RateLimitLayer::new(60, Duration::from_secs(60));
    crate::security::spawn_eviction_task(
        file_mutation_rate_limit.state(),
        Duration::from_secs(300),
    );

    let outbound_rate_limit = RateLimitLayer::new(10, Duration::from_secs(60));
    crate::security::spawn_eviction_task(outbound_rate_limit.state(), Duration::from_secs(300));

    // ── Auth routes (public, rate-limited) ──────────────────────────────
    // Credential endpoints — tight limit (brute-force protection)
    let session_rate_limit = RateLimitLayer::new(30, Duration::from_secs(60));
    crate::security::spawn_eviction_task(session_rate_limit.state(), Duration::from_secs(300));

    let status_rate_limit = RateLimitLayer::new(60, Duration::from_secs(60));
    crate::security::spawn_eviction_task(status_rate_limit.state(), Duration::from_secs(300));

    // Invite redemption — very tight dedicated tier (3 req / 5 min)
    let invite_rate_limit = RateLimitLayer::new(3, Duration::from_secs(300));
    crate::security::spawn_eviction_task(invite_rate_limit.state(), Duration::from_secs(600));

    let credential_auth_routes = Router::new()
        .route("/auth/setup", post(auth::setup))
        .route("/auth/login", post(auth::login))
        .route("/auth/register", post(auth::register))
        .layer(auth_rate_limit);

    // Invite redemption on its own tier — separate from login/register
    let invite_auth_routes = Router::new()
        .route(
            "/auth/redeem-invite",
            post(invite_codes::redeem_invite_code),
        )
        .layer(invite_rate_limit);

    // Session lifecycle — moderate limit (automatic frontend calls)
    let session_auth_routes = Router::new()
        .route("/auth/refresh", post(auth::refresh))
        .route("/auth/logout", post(auth::logout))
        .layer(session_rate_limit);

    // Status — generous limit (called on every page load, no side effects)
    let status_auth_routes = Router::new()
        .route("/auth/status", get(auth::status))
        .layer(status_rate_limit);

    let authed_auth_routes = Router::new()
        .route("/auth/me", get(auth::me))
        .route("/auth/change-password", post(auth::change_password))
        .route("/auth/logout-everywhere", post(auth::logout_everywhere))
        .route("/auth/settings", put(auth::update_settings))
        .route("/auth/sessions", get(auth::list_sessions))
        .route("/auth/sessions/revoke", post(auth::revoke_session))
        .route("/auth/ws-ticket", post(auth::ws_ticket))
        .route("/auth/api-tokens", post(auth::create_api_token))
        .route("/auth/api-tokens", get(auth::list_api_tokens))
        .route("/auth/api-tokens/:id", delete(auth::revoke_api_token));

    let admin_routes = Router::new()
        .route("/admin/users", get(users::list_users))
        .route("/admin/users/:id", get(users::get_user))
        .route("/admin/users/:id/role", put(users::update_role))
        .route(
            "/admin/users/:id/capabilities",
            put(users::update_capabilities),
        )
        .route("/admin/users/:id", delete(users::delete_user))
        // Invite codes (admin)
        .route(
            "/admin/invite-codes",
            get(invite_codes::list_invite_codes).post(invite_codes::create_invite_code),
        )
        .route(
            "/admin/invite-codes/:id",
            get(invite_codes::get_invite_code).delete(invite_codes::delete_invite_code),
        )
        .route(
            "/admin/invite-codes/:id/permissions",
            put(invite_codes::update_invite_permissions),
        )
        // Sandbox feature flag (admin/owner)
        .route(
            "/admin/sandbox/capabilities",
            get(sandbox::get_sandbox_capabilities),
        )
        .route(
            "/admin/sandbox/feature",
            put(sandbox::toggle_sandbox_feature),
        )
        // User permission management (admin)
        .route(
            "/admin/user-permissions",
            get(invite_codes::list_user_permissions),
        );

    // ── Outbound-request routes (10 req/60s) ────────────────────────────
    let import_routes = Router::new()
        .route("/import/url", post(import::import_url))
        .route("/import/folder", post(import::import_folder))
        .layer(outbound_rate_limit.clone());

    let outbound_server_routes = Router::new()
        .route("/servers/:id/check-update", get(update_check::check_update))
        .layer(outbound_rate_limit.clone());

    let outbound_template_routes = Router::new()
        .route("/templates/fetch-options", get(templates::fetch_options))
        .layer(outbound_rate_limit.clone());

    let outbound_smtp_routes = Router::new()
        .route("/admin/smtp/test", post(alerts::send_test_email))
        .layer(outbound_rate_limit.clone());

    // ── Server lifecycle routes (30 req/60s) ────────────────────────────
    let lifecycle_routes = Router::new()
        .route("/servers/:id/start", post(servers::start))
        .route("/servers/:id/stop", post(servers::stop))
        .route("/servers/:id/cancel-stop", post(servers::cancel_stop))
        .route("/servers/:id/restart", post(servers::restart))
        .route("/servers/:id/cancel-restart", post(servers::cancel_restart))
        .route("/servers/:id/sigint", post(servers::send_sigint))
        .route("/servers/:id/reset", post(servers::reset_server))
        .route(
            "/servers/:id/kill-directory-processes",
            post(servers::kill_directory_processes),
        )
        .layer(lifecycle_rate_limit);

    // ── Pipeline execution routes (5 req/60s) ───────────────────────────
    let pipeline_routes = Router::new()
        .route("/servers/:id/install", post(pipeline::install))
        .route("/servers/:id/update", post(pipeline::update))
        .route("/servers/:id/uninstall", post(pipeline::uninstall))
        .route("/servers/:id/kill", post(pipeline::kill))
        .layer(pipeline_rate_limit);

    // ── File mutation routes (60 req/60s) ───────────────────────────────
    let file_mutation_routes = Router::new()
        .route("/servers/:id/files/write", post(files::write_file))
        .route("/servers/:id/files/mkdir", post(files::create_dir))
        .route("/servers/:id/files/delete", post(files::delete_path))
        .route("/servers/:id/files/chmod", post(files::chmod))
        .layer(file_mutation_rate_limit);

    // ── User routes (any authenticated user) ────────────────────────────
    let user_routes = Router::new().route("/users/search", get(users::search_users));

    // ── Remaining server routes (no extra rate limit) ───────────────────
    let server_routes = Router::new()
        .route("/servers", get(servers::list_servers))
        .route("/servers", post(servers::create_server))
        .route("/servers/update-status", get(update_check::update_status))
        .route("/servers/:id", get(servers::get_server))
        .route("/servers/:id", put(servers::update_server))
        .route("/servers/:id", delete(servers::delete_server))
        .route("/servers/:id/command", post(servers::send_command))
        .route("/servers/:id/mark-installed", post(servers::mark_installed))
        .route(
            "/servers/:id/directory-processes",
            get(servers::list_directory_processes),
        )
        .route(
            "/servers/:id/permissions",
            get(permissions::list_permissions),
        )
        .route(
            "/servers/:id/permissions",
            post(permissions::set_permission),
        )
        .route(
            "/servers/:id/permissions/remove",
            post(permissions::remove_permission),
        )
        .route("/servers/:id/phase-status", get(pipeline::phase_status))
        .route("/servers/:id/cancel-phase", post(pipeline::cancel_phase))
        .route("/servers/:id/files", get(files::list_files))
        .route("/servers/:id/files/read", get(files::read_file))
        .route(
            "/servers/:id/files/permissions",
            get(files::get_permissions),
        )
        .route("/servers/:id/stats", get(servers::get_server_stats))
        .route("/servers/:id/alerts", get(alerts::get_server_alerts))
        .route("/servers/:id/alerts", put(alerts::update_server_alerts))
        .route("/servers/:id/ws", get(ws::ws_handler))
        // Per-server sandbox profiles
        .route(
            "/servers/:id/sandbox",
            get(sandbox::get_sandbox_profile)
                .put(sandbox::update_sandbox_profile)
                .delete(sandbox::reset_sandbox_profile),
        );

    let template_routes = Router::new()
        .route("/templates", get(templates::list_templates))
        .route("/templates", post(templates::create_template))
        .route("/templates/:id", get(templates::get_template))
        .route("/templates/:id", put(templates::update_template))
        .route("/templates/:id", delete(templates::delete_template));

    let system_routes = Router::new()
        .route("/system/health", get(system::get_health))
        .route("/system/java-runtimes", get(system::get_java_runtimes))
        .route("/system/java-env", get(system::get_java_env))
        .route("/system/dotnet-runtimes", get(system::get_dotnet_runtimes))
        .route("/system/dotnet-env", get(system::get_dotnet_env))
        .route("/system/steamcmd-status", get(steamcmd::steamcmd_status))
        .route("/admin/backup", get(system::backup_database));

    let steamcmd_routes = Router::new()
        .route("/steamcmd/validate-app", get(steamcmd::validate_app))
        .layer(outbound_rate_limit.clone());

    let smtp_alert_routes = Router::new()
        .route("/admin/smtp", get(alerts::get_smtp_config))
        .route("/admin/smtp", put(alerts::save_smtp_config))
        .route("/admin/smtp", delete(alerts::delete_smtp_config))
        .route("/admin/alerts", get(alerts::get_alert_config))
        .route("/admin/alerts", put(alerts::save_alert_config));

    let github_routes = Router::new()
        .route("/github/releases", get(github::get_releases))
        .route("/admin/settings/github", get(github::get_github_settings))
        .route("/admin/settings/github", put(github::save_github_settings))
        .layer(outbound_rate_limit.clone());

    let curseforge_routes = Router::new()
        .route("/curseforge/files", get(curseforge::get_files))
        .route(
            "/admin/settings/curseforge",
            get(curseforge::get_curseforge_settings),
        )
        .route(
            "/admin/settings/curseforge",
            put(curseforge::save_curseforge_settings),
        )
        .layer(outbound_rate_limit.clone());

    let ws_routes = Router::new().route("/ws/events", get(ws::global_events_handler));

    credential_auth_routes
        .merge(invite_auth_routes)
        .merge(session_auth_routes)
        .merge(status_auth_routes)
        .merge(authed_auth_routes)
        .merge(admin_routes)
        .merge(import_routes)
        .merge(outbound_server_routes)
        .merge(outbound_template_routes)
        .merge(outbound_smtp_routes)
        .merge(lifecycle_routes)
        .merge(pipeline_routes)
        .merge(file_mutation_routes)
        .merge(user_routes)
        .merge(server_routes)
        .merge(template_routes)
        .merge(system_routes)
        .merge(steamcmd_routes)
        .merge(smtp_alert_routes)
        .merge(github_routes)
        .merge(curseforge_routes)
        .merge(ws_routes)
}
