use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::types::*;
use crate::AppState;

/// GET /api/servers/:id/permissions
pub async fn list_permissions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(server_id): Path<Uuid>,
) -> Result<Json<ServerPermissionsResponse>, AppError> {
    let server = state.db.require_server(server_id).await?;

    auth.require_level(&state, &server, PermissionLevel::Admin)
        .await?;

    let raw_perms = state.db.list_permissions_for_server(&server_id).await?;

    let owner_has_explicit = raw_perms.iter().any(|p| p.user_id == server.owner_id);

    let mut entries: Vec<ServerPermissionEntry> = Vec::new();

    if !owner_has_explicit {
        if let Some(owner) = state.db.get_user(server.owner_id).await? {
            entries.push(ServerPermissionEntry {
                user: owner.into(),
                level: PermissionLevel::Owner,
            });
        }
    }

    for perm in raw_perms {
        if let Some(user) = state.db.get_user(perm.user_id).await? {
            entries.push(ServerPermissionEntry {
                user: user.into(),
                level: perm.level,
            });
        }
    }

    entries.sort_by(|a, b| {
        b.level
            .cmp(&a.level)
            .then_with(|| a.user.username.cmp(&b.user.username))
    });

    Ok(Json(ServerPermissionsResponse {
        permissions: entries,
    }))
}

/// POST /api/servers/:id/permissions
pub async fn set_permission(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(server_id): Path<Uuid>,
    Json(req): Json<SetPermissionRequest>,
) -> Result<Json<ServerPermissionEntry>, AppError> {
    let server = state.db.require_server(server_id).await?;

    let caller_perm = auth
        .require_level(&state, &server, PermissionLevel::Admin)
        .await?;

    if req.user_id == server.owner_id {
        return Err(AppError::BadRequest(
            "Cannot change the server owner's permission. \
             Ownership is inherent and cannot be revoked. \
             Transfer ownership instead."
                .into(),
        ));
    }

    if req.user_id == auth.user_id {
        return Err(AppError::BadRequest(
            "You cannot change your own permission level on a server. \
             Ask another admin to modify your access."
                .into(),
        ));
    }

    let target_user = state
        .db
        .get_user(req.user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", req.user_id)))?;

    if req.level == PermissionLevel::Owner && !caller_perm.is_global_admin {
        return Err(AppError::Forbidden(
            "Only global admins can grant Owner-level permission. \
             Server-level admins can grant up to Admin level."
                .into(),
        ));
    }

    if !caller_perm.is_global_admin && req.level > caller_perm.level {
        return Err(AppError::Forbidden(format!(
            "You cannot grant a permission level ({:?}) higher than your own ({:?})",
            req.level, caller_perm.level,
        )));
    }

    let perm = ServerPermission {
        user_id: req.user_id,
        server_id,
        level: req.level,
    };
    state.db.set_permission(&perm).await?;

    tracing::info!(
        "User '{}' granted {:?} permission to user '{}' on server '{}' ({})",
        auth.username,
        req.level,
        target_user.username,
        server.config.name,
        server_id,
    );

    Ok(Json(ServerPermissionEntry {
        user: target_user.into(),
        level: req.level,
    }))
}

/// POST /api/servers/:id/permissions/remove
pub async fn remove_permission(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(server_id): Path<Uuid>,
    Json(req): Json<RemovePermissionRequest>,
) -> Result<Json<RemovePermissionResponse>, AppError> {
    let server = state.db.require_server(server_id).await?;

    let caller_perm = auth
        .require_level(&state, &server, PermissionLevel::Admin)
        .await?;

    if req.user_id == server.owner_id {
        return Err(AppError::BadRequest(
            "Cannot remove the server owner's permission. Ownership is inherent.".into(),
        ));
    }

    if req.user_id == auth.user_id {
        return Err(AppError::BadRequest(
            "You cannot remove your own permission. Ask another admin to modify your access."
                .into(),
        ));
    }

    if !caller_perm.is_global_admin {
        if let Some(target_level) = state
            .db
            .get_effective_permission(&req.user_id, &server_id)
            .await?
        {
            if target_level >= caller_perm.level {
                return Err(AppError::Forbidden(
                    "You cannot remove a user with equal or higher permission than your own."
                        .into(),
                ));
            }
        }
    }

    let target_username = state
        .db
        .get_user(req.user_id)
        .await?
        .map(|u| u.username)
        .unwrap_or_else(|| req.user_id.to_string());

    let removed = state.db.remove_permission(&req.user_id, &server_id).await?;

    if !removed {
        return Err(AppError::NotFound(format!(
            "User '{}' does not have an explicit permission on this server",
            target_username,
        )));
    }

    tracing::info!(
        "User '{}' removed permission for user '{}' on server '{}' ({})",
        auth.username,
        target_username,
        server.config.name,
        server_id,
    );

    Ok(Json(RemovePermissionResponse {
        removed: true,
        user_id: req.user_id.to_string(),
        server_id: server_id.to_string(),
    }))
}
