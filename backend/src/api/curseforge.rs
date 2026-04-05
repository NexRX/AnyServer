//! API endpoints for CurseForge integration.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::integrations::curseforge;
use crate::types::system::{
    CurseForgeFileOption, CurseForgeFilesQuery, CurseForgeFilesResponse, CurseForgeSettings,
    CurseForgeSettingsResponse, SaveCurseForgeSettingsRequest,
};
use crate::types::Role;
use crate::AppState;

/// GET /api/curseforge/files
///
/// Fetch available file versions for a CurseForge project.
/// Returns them as value/label pairs suitable for dropdown population.
/// The CurseForge API key must be configured by an admin.
pub async fn get_files(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Query(query): Query<CurseForgeFilesQuery>,
) -> Result<Json<CurseForgeFilesResponse>, AppError> {
    // Get the stored API key
    let settings = state.db.get_curseforge_settings().await?;
    let api_key = settings.and_then(|s| s.api_key).ok_or_else(|| {
        AppError::BadRequest(
            "CurseForge API key is not configured. \
                 Ask an admin to set it up in Admin Panel → CurseForge."
                .to_string(),
        )
    })?;

    // Fetch files from CurseForge (up to 50 most recent)
    let files =
        curseforge::fetch_project_files(&state.http_client, &api_key, query.project_id, 50).await?;

    // Map to value/label options
    let options: Vec<CurseForgeFileOption> = files
        .into_iter()
        .map(|f| CurseForgeFileOption {
            value: f.id.to_string(),
            label: f.display_name,
        })
        .collect();

    Ok(Json(CurseForgeFilesResponse { options }))
}

/// GET /api/admin/settings/curseforge
///
/// Get CurseForge settings (admin only).
/// Returns whether an API key is configured without revealing the actual key.
pub async fn get_curseforge_settings(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<CurseForgeSettingsResponse>, AppError> {
    if user.role != Role::Admin {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let settings = state.db.get_curseforge_settings().await?;
    let has_key = settings.and_then(|s| s.api_key).is_some();

    Ok(Json(CurseForgeSettingsResponse { has_key }))
}

/// PUT /api/admin/settings/curseforge
///
/// Save CurseForge settings (admin only).
pub async fn save_curseforge_settings(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<SaveCurseForgeSettingsRequest>,
) -> Result<StatusCode, AppError> {
    if user.role != Role::Admin {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // If key is empty or None, delete the settings
    if req.api_key.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        state.db.delete_curseforge_settings().await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    // Validate the key by making a test request (fetch game info for Minecraft, ID 432)
    let api_key = req.api_key.as_ref().unwrap();
    match curseforge::validate_project(&state.http_client, api_key, 432).await {
        Ok(_) => {}
        Err(AppError::BadRequest(msg)) if msg.contains("unauthorized") => {
            return Err(AppError::BadRequest(
                "The provided CurseForge API key is invalid or unauthorized.".to_string(),
            ));
        }
        Err(_) => {
            // Non-auth errors (network, etc.) — save anyway, the key might still be valid
            tracing::warn!("Could not validate CurseForge API key (non-auth error), saving anyway");
        }
    }

    // Save the settings
    let settings = CurseForgeSettings {
        api_key: req.api_key,
    };
    state.db.save_curseforge_settings(&settings).await?;

    Ok(StatusCode::NO_CONTENT)
}
