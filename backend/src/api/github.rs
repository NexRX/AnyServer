//! API endpoints for GitHub integration.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::integrations::github;
use crate::types::system::{
    GithubReleasesQuery, GithubReleasesResponse, GithubSettings, GithubSettingsResponse,
    SaveGithubSettingsRequest,
};
use crate::types::Role;
use crate::AppState;

/// GET /api/github/releases
///
/// Fetch release tags for a GitHub repository.
/// Uses cached results when available to avoid hitting rate limits.
pub async fn get_releases(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Query(query): Query<GithubReleasesQuery>,
) -> Result<Json<GithubReleasesResponse>, AppError> {
    // Get GitHub settings to check for token
    let github_settings = state.db.get_github_settings().await?;
    let token = github_settings.and_then(|s| s.api_token);

    // Fetch releases from GitHub
    let releases =
        github::fetch_release_tags(&state.http_client, &query.repo, token.as_deref()).await?;

    Ok(Json(GithubReleasesResponse {
        releases,
        cached: false,
    }))
}

/// GET /api/admin/settings/github
///
/// Get GitHub settings (admin only).
/// Returns whether a token is configured without revealing the actual token.
pub async fn get_github_settings(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<GithubSettingsResponse>, AppError> {
    if user.role != Role::Admin {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let settings = state.db.get_github_settings().await?;
    let has_token = settings.and_then(|s| s.api_token).is_some();

    Ok(Json(GithubSettingsResponse { has_token }))
}

/// PUT /api/admin/settings/github
///
/// Save GitHub settings (admin only).
pub async fn save_github_settings(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<SaveGithubSettingsRequest>,
) -> Result<StatusCode, AppError> {
    if user.role != Role::Admin {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // If token is empty or None, delete the settings
    if req.api_token.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        state.db.delete_github_settings().await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    // Save the settings
    let settings = GithubSettings {
        api_token: req.api_token,
    };
    state.db.save_github_settings(&settings).await?;

    Ok(StatusCode::NO_CONTENT)
}
