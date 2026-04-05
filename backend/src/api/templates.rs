use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::integrations::github;
use crate::templates::builtin;
use crate::types::*;
use crate::utils::fetch_options;
use crate::utils::steamcmd;
use crate::AppState;

/// Validate that all github_release_tag parameters have valid github_repo set
/// and that the repositories are accessible.
async fn validate_github_params(
    state: &Arc<AppState>,
    config: &ServerConfig,
) -> Result<(), AppError> {
    // Get GitHub settings to check for token
    let github_settings = state.db.get_github_settings().await?;
    let token = github_settings.and_then(|s| s.api_token);

    for param in &config.parameters {
        if matches!(param.param_type, ConfigParameterType::GithubReleaseTag) {
            // Validate that github_repo is set
            let repo = param.github_repo.as_ref().ok_or_else(|| {
                AppError::BadRequest(format!(
                    "Parameter '{}' is of type github_release_tag but missing required github_repo field",
                    param.name
                ))
            })?;

            // Validate repo format
            github::validate_repo_format(repo)?;

            // Validate that the repo is accessible
            github::validate_repo_accessible(&state.http_client, repo, token.as_deref())
                .await
                .map_err(|e| {
                    AppError::BadRequest(format!(
                        "GitHub repository '{}' for parameter '{}' is not accessible: {}",
                        repo, param.name, e
                    ))
                })?;
        }
    }

    Ok(())
}

/// GET /api/templates
pub async fn list_templates(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
) -> Result<Json<TemplateListResponse>, AppError> {
    let mut templates: Vec<ServerTemplate> = builtin::list().to_vec();

    let user_templates = state.db.list_templates().await?;
    templates.extend(user_templates);

    // Compute all `requires_*` integration flags for every template
    let templates: Vec<ServerTemplate> = templates
        .into_iter()
        .map(|t| t.with_integration_flags())
        .collect();

    // Check SteamCMD availability (cached PATH lookup, no subprocess)
    let steamcmd_available =
        tokio::task::spawn_blocking(|| steamcmd::detect_steamcmd_cached().available)
            .await
            .unwrap_or(false);

    // Check CurseForge / GitHub integration availability
    let curseforge_available = state
        .db
        .get_curseforge_settings()
        .await
        .ok()
        .flatten()
        .and_then(|s| s.api_key)
        .is_some();

    let github_available = state
        .db
        .get_github_settings()
        .await
        .ok()
        .flatten()
        .and_then(|s| s.api_token)
        .is_some();

    Ok(Json(TemplateListResponse {
        templates,
        steamcmd_available,
        curseforge_available,
        github_available,
    }))
}

/// GET /api/templates/:id
pub async fn get_template(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ServerTemplate>, AppError> {
    if let Some(builtin) = builtin::get(id) {
        return Ok(Json(builtin.clone().with_integration_flags()));
    }

    let template = state.db.require_template(id).await?;

    Ok(Json(template.with_integration_flags()))
}

/// POST /api/templates
pub async fn create_template(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateTemplateRequest>,
) -> Result<Json<ServerTemplate>, AppError> {
    // Only users with the ManageTemplates capability (or admins) may create templates.
    auth.require_capability(&state, crate::types::GlobalCapability::ManageTemplates)
        .await?;

    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Template name is required".into()));
    }

    // Validate github_release_tag parameters
    validate_github_params(&state, &req.config).await?;

    let now = Utc::now();
    let requires_steamcmd = steamcmd::config_requires_steamcmd(&req.config);
    let requires_curseforge = crate::types::template::config_requires_curseforge(&req.config);
    let requires_github = crate::types::template::config_requires_github(&req.config);
    let template = ServerTemplate {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        config: req.config,
        created_by: auth.user_id,
        created_at: now,
        updated_at: now,
        is_builtin: false,
        requires_steamcmd,
        requires_curseforge,
        requires_github,
    };

    state.db.insert_template(&template).await?;

    tracing::info!(
        "User '{}' created template '{}' (id={})",
        auth.username,
        template.name,
        template.id,
    );

    Ok(Json(template))
}

/// PUT /api/templates/:id
pub async fn update_template(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTemplateRequest>,
) -> Result<Json<ServerTemplate>, AppError> {
    // Only users with the ManageTemplates capability (or admins) may update templates.
    auth.require_capability(&state, crate::types::GlobalCapability::ManageTemplates)
        .await?;

    if builtin::is_builtin(id) {
        return Err(AppError::Forbidden(
            "Built-in templates cannot be modified. Create a copy instead.".into(),
        ));
    }

    let mut template = state.db.require_template(id).await?;

    if template.created_by != auth.user_id && !auth.is_admin() {
        return Err(AppError::Forbidden(
            "Only the template creator or an admin can update this template".into(),
        ));
    }

    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Template name is required".into()));
    }

    // Validate github_release_tag parameters
    validate_github_params(&state, &req.config).await?;

    template.name = req.name;
    template.description = req.description;
    template.config = req.config;
    template.updated_at = Utc::now();

    state.db.update_template(&template).await?;

    tracing::info!(
        "User '{}' updated template '{}' (id={})",
        auth.username,
        template.name,
        template.id,
    );

    Ok(Json(template))
}

/// DELETE /api/templates/:id
pub async fn delete_template(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteTemplateResponse>, AppError> {
    // Only users with the ManageTemplates capability (or admins) may delete templates.
    auth.require_capability(&state, crate::types::GlobalCapability::ManageTemplates)
        .await?;

    if builtin::is_builtin(id) {
        return Err(AppError::Forbidden(
            "Built-in templates cannot be deleted.".into(),
        ));
    }

    let template = state.db.require_template(id).await?;

    if template.created_by != auth.user_id && !auth.is_admin() {
        return Err(AppError::Forbidden(
            "Only the template creator or an admin can delete this template".into(),
        ));
    }

    state.db.delete_template(id).await?;

    tracing::info!(
        "User '{}' deleted template '{}' (id={})",
        auth.username,
        template.name,
        id,
    );

    Ok(Json(DeleteTemplateResponse {
        deleted: true,
        id: id.to_string(),
    }))
}

/// GET /api/templates/fetch-options — proxies an external JSON API to
/// return dropdown options (avoids CORS, enforces timeouts/size limits).
pub async fn fetch_options(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Query(q): Query<FetchOptionsQuery>,
) -> Result<Json<FetchOptionsResponse>, AppError> {
    let vars: HashMap<String, String> = match &q.params {
        Some(json_str) if !json_str.is_empty() => serde_json::from_str(json_str).map_err(|e| {
            AppError::BadRequest(format!("Invalid JSON in 'params' query parameter: {}", e))
        })?,
        _ => HashMap::new(),
    };

    let options = fetch_options::fetch_and_extract(
        &state.http_client,
        &q.url,
        q.path.as_deref(),
        q.value_key.as_deref(),
        q.label_key.as_deref(),
        q.sort,
        q.limit,
        &vars,
    )
    .await
    .map_err(AppError::BadRequest)?;

    Ok(Json(FetchOptionsResponse {
        options,
        cached: false,
    }))
}
