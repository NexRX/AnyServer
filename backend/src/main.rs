use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::signal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use anyserver::types::ServerStatus;
use anyserver::{
    auth_system, build_router, monitoring, pipeline, server_management, sftp_server, storage,
    AppState,
};
use sysinfo::System;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "anyserver=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let data_dir =
        PathBuf::from(std::env::var("ANYSERVER_DATA_DIR").unwrap_or_else(|_| "./data".to_string()));
    std::fs::create_dir_all(data_dir.join("servers"))?;
    // Canonicalize so that data_dir (and every path derived from it, such as
    // server_dir) is always absolute.  This is required because external tools
    // like SteamCMD interpret relative paths relative to their own working
    // directory, not ours.
    let data_dir = data_dir.canonicalize()?;

    anyserver::auth::init_jwt_secret(&data_dir);

    let db = storage::Database::open(data_dir.join("anyserver.db")).await?;
    let process_manager = server_management::ProcessManager::new();
    let pipeline_manager = pipeline::PipelineManager::new();

    let http_client = anyserver::security::ssrf::build_ssrf_safe_client()
        .timeout(std::time::Duration::from_secs(600))
        .connect_timeout(std::time::Duration::from_secs(15))
        .read_timeout(std::time::Duration::from_secs(30))
        .user_agent("AnyServer/1.0")
        .build()
        .expect("failed to build shared HTTP client");

    // Pre-seed sysinfo so the first health call has a baseline for CPU deltas.
    let mut sys = System::new();
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let stats_collector = Arc::new(server_management::StatsCollector::new());

    let alert_dispatcher = monitoring::AlertDispatcher::new();

    let ws_ticket_store = auth_system::WsTicketStore::new();
    auth_system::spawn_ws_ticket_reaper(
        ws_ticket_store.clone(),
        std::time::Duration::from_secs(10),
    );

    let login_attempt_tracker = auth_system::LoginAttemptTracker::new();
    auth_system::spawn_lockout_reaper(
        login_attempt_tracker.clone(),
        std::time::Duration::from_secs(60),
        std::time::Duration::from_secs(30 * 60),
        10_000,
    );

    let state = Arc::new(AppState {
        db,
        process_manager,
        pipeline_manager,
        data_dir: data_dir.clone(),
        http_client,
        system_monitor: parking_lot::Mutex::new(sys),
        stats_collector: Arc::clone(&stats_collector),
        update_cache: dashmap::DashMap::new(),
        alert_dispatcher,
        ws_ticket_store,
        login_attempt_tracker,
    });

    tracing::info!("\n{}", anyserver::sandbox::probe_capabilities());

    if let Err(e) = storage::migrate_sftp_passwords(&state.db).await {
        tracing::error!("SFTP password migration failed: {}", e);
    }

    match state.db.migrate_smtp_password().await {
        Ok(true) => tracing::info!("Migrated plaintext SMTP password to encrypted form"),
        Ok(false) => {}
        Err(e) => tracing::error!("SMTP password encryption migration failed: {}", e),
    }

    // Must run BEFORE auto-start so we don't double-launch servers
    // that survived an unclean shutdown.
    server_management::reconcile_processes(&state).await;

    {
        let servers = state.db.list_servers().await?;
        for server in servers {
            if server.config.auto_start {
                let already_running = {
                    let rt = state.process_manager.get_runtime(&server.id);
                    rt.status == anyserver::types::ServerStatus::Running
                };
                if already_running {
                    tracing::info!(
                        "Skipping auto-start for server {} — already running (reconciled)",
                        server.id,
                    );
                    continue;
                }
                let state_clone = Arc::clone(&state);
                let server_id = server.id;
                tokio::spawn(async move {
                    if let Err(e) = server_management::start_server(&state_clone, server_id).await {
                        tracing::error!("Failed to auto-start server {}: {}", server_id, e);
                    }
                });
            }
        }
    }

    let sftp_state = Arc::clone(&state);
    let sftp_port: u16 = std::env::var("ANYSERVER_SFTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(2222);
    tokio::spawn(async move {
        if let Err(e) = sftp_server::run_sftp_server(sftp_state, sftp_port).await {
            tracing::error!("SFTP server error: {}", e);
        }
    });

    let _stats_handle = server_management::spawn_collection_task(
        Arc::clone(&stats_collector),
        Arc::clone(&state),
        std::time::Duration::from_secs(3),
    );

    let _alert_handle = monitoring::spawn_alert_monitor_task(
        Arc::clone(&state),
        std::time::Duration::from_secs(30),
    );

    // Periodic cleanup of expired refresh tokens (every 6 hours).
    // Runs once immediately on startup to clear any backlog, then on interval.
    let cleanup_state = Arc::clone(&state);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(6 * 60 * 60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match cleanup_state.db.delete_expired_refresh_tokens().await {
                Ok(count) if count > 0 => {
                    tracing::info!("Cleaned up {} expired refresh token(s)", count);
                }
                Err(e) => {
                    tracing::warn!("Failed to clean up expired refresh tokens: {}", e);
                }
                _ => {}
            }
        }
    });

    // Periodic cleanup of expired invite codes (every 1 hour).
    let invite_cleanup_state = Arc::clone(&state);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match invite_cleanup_state.db.delete_expired_invite_codes().await {
                Ok(count) if count > 0 => {
                    tracing::info!("Cleaned up {} expired invite code(s)", count);
                }
                Err(e) => {
                    tracing::warn!("Failed to clean up expired invite codes: {}", e);
                }
                _ => {}
            }
        }
    });

    let shutdown_state = Arc::clone(&state);
    let app = build_router(state);

    let http_port: u16 = std::env::var("ANYSERVER_HTTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3001);
    let addr = SocketAddr::from(([0, 0, 0, 0], http_port));
    tracing::info!("AnyServer HTTP listening on {}", addr);
    tracing::info!("SFTP server listening on port {}", sftp_port);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    tracing::info!("Shutdown signal received — stopping all running servers...");

    let servers = shutdown_state.db.list_servers().await.unwrap_or_default();
    let mut stop_tasks = Vec::new();

    for server in &servers {
        let runtime = shutdown_state.process_manager.get_runtime(&server.id);
        if runtime.status == ServerStatus::Running || runtime.status == ServerStatus::Starting {
            let state_clone = Arc::clone(&shutdown_state);
            let server_id = server.id;
            let server_name = server.config.name.clone();
            stop_tasks.push(tokio::spawn(async move {
                tracing::info!("Stopping server '{}' ({})...", server_name, server_id);
                match server_management::stop_server(&state_clone, server_id).await {
                    Ok(()) => {
                        tracing::info!("Server '{}' ({}) stopped cleanly", server_name, server_id)
                    }
                    Err(e) => tracing::warn!(
                        "Failed to stop server '{}' ({}): {}",
                        server_name,
                        server_id,
                        e
                    ),
                }
            }));
        }
    }

    if !stop_tasks.is_empty() {
        tracing::info!("Waiting for {} server(s) to shut down...", stop_tasks.len());
        let shutdown_deadline = tokio::time::Duration::from_secs(30);
        let _ =
            tokio::time::timeout(shutdown_deadline, futures::future::join_all(stop_tasks)).await;
    }

    tracing::info!("AnyServer shut down gracefully.");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received SIGINT (Ctrl+C) — initiating graceful shutdown");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM — initiating graceful shutdown");
        }
    }
}
