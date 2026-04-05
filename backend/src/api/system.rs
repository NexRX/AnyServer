use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use sysinfo::{Disks, Networks, System};

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::types::{
    CpuMetrics, DiskMetrics, DotnetRuntimesResponse, JavaRuntimesResponse, MemoryMetrics,
    NetworkMetrics, SystemHealth,
};
use crate::utils::{dotnet_detect, java_detect};

/// GET /api/system/health
pub async fn get_health(
    _user: AuthUser,
    State(state): State<Arc<crate::AppState>>,
) -> Result<Json<SystemHealth>, AppError> {
    let health = {
        let mut sys = state.system_monitor.lock();

        sys.refresh_cpu_usage();
        sys.refresh_memory();

        let cpus = sys.cpus();
        let per_core_percent: Vec<f32> = cpus.iter().map(|c| c.cpu_usage()).collect();
        let overall_percent = if per_core_percent.is_empty() {
            0.0
        } else {
            per_core_percent.iter().sum::<f32>() / per_core_percent.len() as f32
        };

        let load_avg = System::load_average();

        let cpu = CpuMetrics {
            overall_percent,
            per_core_percent,
            load_avg_1: load_avg.one,
            load_avg_5: load_avg.five,
            load_avg_15: load_avg.fifteen,
            core_count: cpus.len() as u32,
        };

        let memory = MemoryMetrics {
            total_bytes: sys.total_memory(),
            used_bytes: sys.used_memory(),
            available_bytes: sys.available_memory(),
            swap_total_bytes: sys.total_swap(),
            swap_used_bytes: sys.used_swap(),
        };

        let disk_info = Disks::new_with_refreshed_list();
        let disks: Vec<DiskMetrics> = disk_info
            .iter()
            .map(|d| {
                let total = d.total_space();
                let free = d.available_space();
                DiskMetrics {
                    name: d.name().to_string_lossy().to_string(),
                    mount_point: d.mount_point().to_string_lossy().to_string(),
                    total_bytes: total,
                    used_bytes: total.saturating_sub(free),
                    free_bytes: free,
                    filesystem: d.file_system().to_string_lossy().to_string(),
                }
            })
            .collect();

        let net_info = Networks::new_with_refreshed_list();
        let networks: Vec<NetworkMetrics> = net_info
            .iter()
            .map(|(name, data)| NetworkMetrics {
                interface: name.clone(),
                rx_bytes: data.total_received(),
                tx_bytes: data.total_transmitted(),
            })
            .collect();

        let uptime_secs = System::uptime();
        let hostname = System::host_name().unwrap_or_else(|| "unknown".to_string());

        SystemHealth {
            cpu,
            memory,
            disks,
            networks,
            uptime_secs,
            hostname,
            timestamp: Utc::now(),
        }
    };

    Ok(Json(health))
}

/// GET /api/system/java-runtimes
pub async fn get_java_runtimes(_user: AuthUser) -> Result<Json<JavaRuntimesResponse>, AppError> {
    let runtimes = tokio::task::spawn_blocking(java_detect::detect_java_runtimes)
        .await
        .map_err(|e| AppError::Internal(format!("Java detection task failed: {}", e)))?;

    Ok(Json(JavaRuntimesResponse { runtimes }))
}

/// Query parameters for `GET /api/system/java-env`.
#[derive(Debug, Deserialize)]
pub struct JavaEnvQuery {
    /// The JAVA_HOME directory of the runtime to use.
    pub java_home: String,
}

/// GET /api/system/java-env
///
/// Generate recommended environment variables for a specific Java runtime.
/// This helps servers that use shell scripts or wrappers that invoke `java`
/// under the hood — setting `JAVA_HOME` ensures the correct JDK is used.
///
/// At spawn time the backend automatically prepends `$JAVA_HOME/bin` to
/// `PATH`, so callers do not need to set PATH manually.
///
/// Query parameters:
/// - `java_home`: The JAVA_HOME directory (from the runtime list)
///
/// Returns a HashMap of environment variable key-value pairs.
pub async fn get_java_env(
    _user: AuthUser,
    Query(params): Query<JavaEnvQuery>,
) -> Result<Json<HashMap<String, String>>, AppError> {
    let env_vars = java_detect::generate_java_env_vars(&params.java_home);
    Ok(Json(env_vars))
}

/// GET /api/system/dotnet-runtimes
pub async fn get_dotnet_runtimes(
    _user: AuthUser,
) -> Result<Json<DotnetRuntimesResponse>, AppError> {
    let runtimes = tokio::task::spawn_blocking(dotnet_detect::detect_dotnet_runtimes)
        .await
        .map_err(|e| AppError::Internal(format!(".NET detection task failed: {}", e)))?;

    Ok(Json(DotnetRuntimesResponse { runtimes }))
}

/// Query parameters for `GET /api/system/dotnet-env`.
#[derive(Debug, Deserialize)]
pub struct DotnetEnvQuery {
    /// The installation root path of the .NET runtime to use.
    pub installation_root: String,
    /// Optional server directory path for bundle extraction cache.
    #[serde(default)]
    pub server_dir: Option<String>,
}

/// GET /api/system/dotnet-env
///
/// Generate recommended environment variables for a specific .NET runtime.
/// This helps servers that need specific .NET versions (like TShock) run properly.
///
/// Query parameters:
/// - `installation_root`: The .NET installation root path (from the runtime list)
/// - `server_dir`: Optional server directory path for bundle extraction
///
/// Returns a HashMap of environment variable key-value pairs.
pub async fn get_dotnet_env(
    _user: AuthUser,
    Query(params): Query<DotnetEnvQuery>,
) -> Result<Json<HashMap<String, String>>, AppError> {
    let env_vars = dotnet_detect::generate_dotnet_env_vars(
        &params.installation_root,
        params.server_dir.as_deref(),
    );

    Ok(Json(env_vars))
}

/// GET /api/admin/backup — download a consistent snapshot of the database.
///
/// Uses SQLite `VACUUM INTO` to produce a standalone `.db` file that does
/// not depend on WAL or SHM files.  Admin-only.
pub async fn backup_database(
    auth: AuthUser,
    State(state): State<Arc<crate::AppState>>,
) -> Result<impl IntoResponse, AppError> {
    if !auth.is_admin() {
        return Err(AppError::Forbidden(
            "Only admins can download database backups".into(),
        ));
    }

    let tmp = tempfile::NamedTempFile::new()
        .map_err(|e| AppError::Internal(format!("Failed to create temp file: {}", e)))?;

    state.db.vacuum_into(tmp.path()).await?;

    let bytes = tokio::fs::read(tmp.path())
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read backup file: {}", e)))?;

    let size_mb = bytes.len() as f64 / (1024.0 * 1024.0);
    let filename = format!("anyserver-backup-{}.db", Utc::now().format("%Y%m%d-%H%M%S"));

    tracing::info!(
        "Admin '{}' downloaded database backup ({:.2} MB)",
        auth.username,
        size_mb,
    );

    Ok((
        [
            (header::CONTENT_TYPE, "application/x-sqlite3".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            ),
        ],
        bytes,
    ))
}
