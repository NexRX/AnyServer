pub mod dotnet_detect;
pub mod fetch_options;
pub mod java_detect;
pub mod json_path;
pub mod steamcmd;

pub use dotnet_detect::{detect_dotnet_runtimes, generate_dotnet_env_vars};
pub use fetch_options::{extract_options, sort_and_limit, substitute_template_vars};
pub use java_detect::detect_java_runtimes;
pub use json_path::json_navigate;
pub use steamcmd::{
    config_requires_steamcmd, detect_steamcmd, detect_steamcmd_cached, invalidate_steamcmd_cache,
    steamcmd_path, validate_app_id,
};

use crate::error::AppError;

/// Run a synchronous closure on tokio's blocking thread pool.
///
/// This is the shared version of the helper originally in `api/files.rs`.
/// Use it to offload heavy filesystem I/O (e.g. `/proc` scans, file reads)
/// so the async runtime is not stalled.
pub async fn blocking<F, T>(f: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Internal(format!("Blocking task panicked: {}", e)))?
}
