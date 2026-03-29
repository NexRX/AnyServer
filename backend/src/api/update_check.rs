use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::templates::update_check::{
    build_check_variables, find_version_param_name, get_installed_version, perform_check,
};
use crate::types::*;
use crate::AppState;

#[derive(Debug, serde::Deserialize)]
pub struct CheckUpdateQuery {
    #[serde(default)]
    pub force: bool,
}

/// GET /api/servers/:id/check-update
pub async fn check_update(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(server_id): Path<Uuid>,
    Query(query): Query<CheckUpdateQuery>,
) -> Result<Json<UpdateCheckResult>, AppError> {
    let server = state.db.require_server(server_id).await?;

    auth.require_level(&state, &server, PermissionLevel::Viewer)
        .await?;

    let update_check = server
        .config
        .update_check
        .as_ref()
        .ok_or_else(|| {
            AppError::BadRequest("This server does not have update checking configured".into())
        })?
        .clone();

    if !query.force {
        if let Some(cached) = state.update_cache.get(&server_id) {
            let age = Utc::now()
                .signed_duration_since(cached.checked_at)
                .num_seconds();
            if age >= 0 && (age as u64) < update_check.cache_secs {
                return Ok(Json(cached.clone()));
            }
        }
    }

    let vars = build_check_variables(&server.parameter_values, &server.config.parameters);

    let installed_version = get_installed_version(
        &server.installed_version,
        &server.parameter_values,
        &server.config.parameters,
    );

    let source_template_id = server.source_template_id;
    let version_param_name = find_version_param_name(&server.config.parameters);

    let template_default_value = if let Some(tid) = source_template_id {
        let param_name = version_param_name.as_deref();

        if let Some(tmpl) = crate::templates::builtin::get(tid) {
            if let Some(param_name) = param_name {
                tmpl.config
                    .parameters
                    .iter()
                    .find(|p| p.name == param_name)
                    .and_then(|p| p.default.clone())
            } else {
                None
            }
        } else {
            match state.db.get_template(tid).await {
                Ok(Some(tmpl)) => {
                    if let Some(param_name) = param_name {
                        tmpl.config
                            .parameters
                            .iter()
                            .find(|p| p.name == param_name)
                            .and_then(|p| p.default.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
    } else {
        None
    };

    let template_lookup = move || -> Result<Option<String>, String> {
        if source_template_id.is_none() {
            return Err(
                "Server was not created from a template (no source_template_id)".to_string(),
            );
        }
        if version_param_name.is_none() {
            return Err("No version parameter defined on this server's config".to_string());
        }
        Ok(template_default_value)
    };

    let result = perform_check(
        &state.http_client,
        server_id,
        &update_check,
        installed_version,
        &vars,
        template_lookup,
    )
    .await;

    state.update_cache.insert(server_id, result.clone());

    Ok(Json(result))
}

/// GET /api/servers/update-status
pub async fn update_status(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<UpdateCheckStatusResponse>, AppError> {
    // Use an access-scoped query so we don't perform O(n) permission
    // checks for every server in the system (N+1 elimination).
    let user_id = if auth.is_admin() {
        None
    } else {
        Some(&auth.user_id)
    };

    let servers = state
        .db
        .list_servers_all_filtered(None, "name", "asc", user_id)
        .await?;

    let mut results = Vec::new();

    for server in &servers {
        if let Some(cached) = state.update_cache.get(&server.id) {
            results.push(cached.clone());
        }
    }

    Ok(Json(UpdateCheckStatusResponse { results }))
}
