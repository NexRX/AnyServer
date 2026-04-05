use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::types::*;
use crate::AppState;

// ─── User search (any authenticated user) ─────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SearchUsersQuery {
    /// Username prefix / substring to search for (case-insensitive).
    #[serde(default)]
    pub q: String,
}

/// `GET /api/users/search?q=<query>`
///
/// Lightweight user search accessible to **any authenticated user**.
/// Returns up to 20 matching users (id + username + role only).
/// Used by server owners to find users when granting per-server permissions.
pub async fn search_users(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Query(query): Query<SearchUsersQuery>,
) -> Result<Json<UserListResponse>, AppError> {
    let term = query.q.trim().to_lowercase();

    let all_users = state.db.list_users().await?;

    let mut matches: Vec<UserPublic> = all_users
        .into_iter()
        .filter(|u| {
            if term.is_empty() {
                true
            } else {
                u.username.to_lowercase().contains(&term)
            }
        })
        .take(20)
        .map(UserPublic::from)
        .collect();

    // Sort exact-prefix matches first, then alphabetically
    matches.sort_by(|a, b| {
        let a_starts = a.username.to_lowercase().starts_with(&term);
        let b_starts = b.username.to_lowercase().starts_with(&term);
        b_starts
            .cmp(&a_starts)
            .then_with(|| a.username.cmp(&b.username))
    });

    Ok(Json(UserListResponse { users: matches }))
}

/// PUT /api/admin/users/:id/capabilities
pub async fn update_capabilities(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUserCapabilitiesRequest>,
) -> Result<Json<UserPublic>, AppError> {
    auth.require_fresh_admin(&state).await?;

    let mut user = state.db.require_user(id).await?;

    if user.role == Role::Admin {
        return Err(AppError::BadRequest(
            "Admin users implicitly have all capabilities. \
             Changing capabilities on an admin has no effect."
                .into(),
        ));
    }

    let old_caps = user.global_capabilities.clone();
    user.global_capabilities = req.global_capabilities;
    state.db.update_user(&user).await?;

    tracing::info!(
        "Admin '{}' updated capabilities of user '{}' from {:?} to {:?}",
        auth.username,
        user.username,
        old_caps,
        user.global_capabilities
    );

    Ok(Json(user.into()))
}

/// GET /api/admin/users
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<UserListResponse>, AppError> {
    auth.require_fresh_admin(&state).await?;

    let users = state.db.list_users().await?;
    let public: Vec<UserPublic> = users.into_iter().map(UserPublic::from).collect();

    Ok(Json(UserListResponse { users: public }))
}

/// GET /api/admin/users/:id
pub async fn get_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<UserPublic>, AppError> {
    auth.require_fresh_admin(&state).await?;

    let user = state.db.require_user(id).await?;

    Ok(Json(user.into()))
}

/// PUT /api/admin/users/:id/role
pub async fn update_role(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUserRoleRequest>,
) -> Result<Json<UserPublic>, AppError> {
    auth.require_fresh_admin(&state).await?;

    if id == auth.user_id && req.role != Role::Admin {
        return Err(AppError::BadRequest(
            "You cannot demote yourself. Ask another admin to change your role.".into(),
        ));
    }

    let mut user = state.db.require_user(id).await?;

    let old_role = user.role;
    user.role = req.role;
    state.db.update_user(&user).await?;

    tracing::info!(
        "Admin '{}' changed role of user '{}' from {:?} to {:?}",
        auth.username,
        user.username,
        old_role,
        user.role
    );

    Ok(Json(user.into()))
}

/// DELETE /api/admin/users/:id
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteUserResponse>, AppError> {
    auth.require_fresh_admin(&state).await?;

    if id == auth.user_id {
        return Err(AppError::BadRequest(
            "You cannot delete your own account while logged in. \
             Ask another admin to remove your account."
                .into(),
        ));
    }

    let user = state.db.require_user(id).await?;

    if user.role == Role::Admin {
        let admins: Vec<_> = state
            .db
            .list_users()
            .await?
            .into_iter()
            .filter(|u| u.role == Role::Admin)
            .collect();

        if admins.len() <= 1 {
            return Err(AppError::BadRequest(
                "Cannot delete the last admin user. Promote another user to admin first.".into(),
            ));
        }
    }

    state.db.delete_user(id).await?;

    tracing::info!(
        "Admin '{}' deleted user '{}' (id={})",
        auth.username,
        user.username,
        id
    );

    Ok(Json(DeleteUserResponse {
        deleted: true,
        id: id.to_string(),
        username: user.username,
    }))
}
