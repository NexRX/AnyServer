//! API endpoint for unified integration/feature availability status.
//!
//! Provides a single endpoint that any authenticated user can call to
//! discover which integrations have been configured by an admin.  The
//! frontend uses this to proactively hide, disable, or annotate features
//! whose backing services aren't set up yet — instead of letting users
//! click through to a confusing error.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::types::system::IntegrationStatus;
use crate::utils::steamcmd;
use crate::AppState;

/// GET /api/integrations/status
///
/// Returns the availability status of all optional integrations.
/// Accessible to **any authenticated user** (not just admins).
///
/// This intentionally reveals *whether* integrations are configured
/// but never exposes secrets (API keys, tokens, passwords, etc.).
pub async fn get_integration_status(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<IntegrationStatus>, AppError> {
    // CurseForge: requires an API key to be saved
    let curseforge_configured = state
        .db
        .get_curseforge_settings()
        .await
        .ok()
        .flatten()
        .and_then(|s| s.api_key)
        .is_some();

    // GitHub: optional token (public repos work without it, but private
    // repos and higher rate limits require one)
    let github_configured = state
        .db
        .get_github_settings()
        .await
        .ok()
        .flatten()
        .and_then(|s| s.api_token)
        .is_some();

    // SteamCMD: binary must be on PATH
    let steamcmd_available =
        tokio::task::spawn_blocking(|| steamcmd::detect_steamcmd_cached().available)
            .await
            .unwrap_or(false);

    // SMTP: must have a saved configuration
    let smtp_configured = state.db.get_smtp_config().await.ok().flatten().is_some();

    Ok(Json(IntegrationStatus {
        curseforge_configured,
        github_configured,
        steamcmd_available,
        smtp_configured,
    }))
}
