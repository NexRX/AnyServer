//! SteamCMD-related API endpoints.
//!
//! - `GET /api/system/steamcmd-status` — check if steamcmd is available
//! - `GET /api/steamcmd/validate-app`  — validate a Steam app ID against
//!   the store API and return its display name

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::utils::steamcmd::{self, SteamCmdStatusResponse, ValidateAppQuery, ValidateAppResponse};
use crate::AppState;

/// GET /api/system/steamcmd-status
///
/// Returns whether `steamcmd` is available on the host and, if so, the
/// absolute path to the binary.  This is a lightweight check (PATH lookup
/// only — no subprocess is spawned).
pub async fn steamcmd_status(_auth: AuthUser) -> Result<Json<SteamCmdStatusResponse>, AppError> {
    let status = tokio::task::spawn_blocking(steamcmd::detect_steamcmd_cached)
        .await
        .map_err(|e| AppError::Internal(format!("SteamCMD detection task failed: {}", e)))?;

    Ok(Json(SteamCmdStatusResponse { status }))
}

/// `GET /api/steamcmd/validate-app?app_id=<id>`
///
/// Validates a Steam application ID by querying the Steam store API.
/// On success returns the application's display name.  On failure returns
/// a structured error so the frontend can show a clear message.
pub async fn validate_app(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Query(q): Query<ValidateAppQuery>,
) -> Result<Json<ValidateAppResponse>, AppError> {
    if q.app_id == 0 {
        return Ok(Json(ValidateAppResponse {
            valid: false,
            app: None,
            error: Some("App ID must be a positive integer".into()),
        }));
    }

    match steamcmd::validate_app_id(&state.http_client, q.app_id).await {
        Ok(info) => Ok(Json(ValidateAppResponse {
            valid: true,
            app: Some(info),
            error: None,
        })),
        Err(msg) => Ok(Json(ValidateAppResponse {
            valid: false,
            app: None,
            error: Some(msg),
        })),
    }
}
