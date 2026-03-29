use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use crate::auth::{hash_password, validate_password, validate_username, AuthUser};
use crate::error::AppError;
use crate::types::*;
use crate::AppState;

/// Character set for invite codes — 32 unambiguous alphanumeric characters.
/// Excludes confusable glyphs: I/O/0/1.
const CODE_CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH: usize = 8;

/// Normalize a user-supplied invite code: strip whitespace and dashes,
/// convert to uppercase.  This lets users type `abcd-efgh`, `ABCD EFGH`,
/// or `abcdefgh` and all resolve to the same stored code.
pub fn normalize_code(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// Generate a unique 8-character alphanumeric invite code (stored without
/// dashes).  The display format is `XXXX-XXXX` but the DB stores the raw
/// 8-char string.
async fn generate_unique_code(state: &AppState) -> Result<String, AppError> {
    for _ in 0..100 {
        // Generate the code without holding the RNG across an await point.
        let code = {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            (0..CODE_LENGTH)
                .map(|_| {
                    let idx = rng.gen_range(0..CODE_CHARSET.len());
                    CODE_CHARSET[idx] as char
                })
                .collect::<String>()
        };
        if !state.db.code_exists(&code).await? {
            return Ok(code);
        }
    }

    Err(AppError::Internal(
        "Failed to generate a unique invite code after 100 attempts".into(),
    ))
}

/// POST /api/admin/invite-codes
///
/// Create a new invite code. Admin only.
pub async fn create_invite_code(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateInviteCodeRequest>,
) -> Result<Json<CreateInviteCodeResponse>, AppError> {
    auth.require_fresh_admin(&state).await?;

    let code = generate_unique_code(&state).await?;
    let now = Utc::now();
    let expires_at = now + req.expiry.to_duration();

    // Validate that referenced servers exist
    for grant in &req.assigned_permissions {
        state.db.require_server(grant.server_id).await?;
    }

    // Format for display: XXXX-XXXX
    let display_code = if code.len() == CODE_LENGTH {
        format!("{}-{}", &code[..4], &code[4..])
    } else {
        code.clone()
    };

    let invite = InviteCode {
        id: Uuid::new_v4(),
        code: code.clone(),
        created_by: auth.user_id,
        assigned_role: req.assigned_role,
        assigned_permissions: req.assigned_permissions,
        assigned_capabilities: req.assigned_capabilities,
        expires_at,
        redeemed_by: None,
        redeemed_at: None,
        created_at: now,
        label: req.label,
    };

    state.db.insert_invite_code(&invite).await?;

    tracing::info!(
        "Admin '{}' created invite code {} (expires at {})",
        auth.username,
        display_code,
        expires_at,
    );

    let creator = state.db.get_user(auth.user_id).await?;
    let creator_username = creator.map(|u| u.username);

    Ok(Json(CreateInviteCodeResponse {
        invite: to_public(&invite, creator_username, None),
    }))
}

/// GET /api/admin/invite-codes
///
/// List all invite codes. Admin only.
pub async fn list_invite_codes(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<InviteCodeListResponse>, AppError> {
    auth.require_fresh_admin(&state).await?;

    let codes = state.db.list_invite_codes().await?;
    let mut public_codes = Vec::with_capacity(codes.len());

    for invite in codes {
        let creator = state.db.get_user(invite.created_by).await?;
        let creator_username = creator.map(|u| u.username);
        let redeemer_username = if let Some(rid) = invite.redeemed_by {
            state.db.get_user(rid).await?.map(|u| u.username)
        } else {
            None
        };
        public_codes.push(to_public(&invite, creator_username, redeemer_username));
    }

    Ok(Json(InviteCodeListResponse {
        invites: public_codes,
    }))
}

/// GET /api/admin/invite-codes/:id
///
/// Get a single invite code by ID. Admin only.
pub async fn get_invite_code(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<InviteCodePublic>, AppError> {
    auth.require_fresh_admin(&state).await?;

    let invite = state
        .db
        .get_invite_code(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Invite code {} not found", id)))?;

    let creator = state.db.get_user(invite.created_by).await?;
    let creator_username = creator.map(|u| u.username);
    let redeemer_username = if let Some(rid) = invite.redeemed_by {
        state.db.get_user(rid).await?.map(|u| u.username)
    } else {
        None
    };

    Ok(Json(to_public(
        &invite,
        creator_username,
        redeemer_username,
    )))
}

/// PUT /api/admin/invite-codes/:id/permissions
///
/// Update the permissions on a pending (unredeemed, unexpired) invite code. Admin only.
pub async fn update_invite_permissions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateInvitePermissionsRequest>,
) -> Result<Json<InviteCodePublic>, AppError> {
    auth.require_fresh_admin(&state).await?;

    let invite = state
        .db
        .get_invite_code(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Invite code {} not found", id)))?;

    if invite.redeemed_by.is_some() {
        return Err(AppError::BadRequest(
            "Cannot update permissions on a redeemed invite code".into(),
        ));
    }

    if invite.expires_at < Utc::now() {
        return Err(AppError::BadRequest(
            "Cannot update permissions on an expired invite code".into(),
        ));
    }

    // Validate that referenced servers exist
    for grant in &req.assigned_permissions {
        state.db.require_server(grant.server_id).await?;
    }

    let role_str = format!("{:?}", req.assigned_role).to_lowercase();
    state
        .db
        .update_invite_permissions(&id, &role_str, &req.assigned_permissions)
        .await?;

    // Re-fetch to return the updated version
    let updated = state
        .db
        .get_invite_code(id)
        .await?
        .ok_or_else(|| AppError::Internal("Invite code disappeared after update".into()))?;

    let creator = state.db.get_user(updated.created_by).await?;
    let creator_username = creator.map(|u| u.username);

    tracing::info!(
        "Admin '{}' updated permissions on invite code {} (id={})",
        auth.username,
        updated.code,
        id,
    );

    Ok(Json(to_public(&updated, creator_username, None)))
}

/// DELETE /api/admin/invite-codes/:id
///
/// Delete an invite code. Admin only.
pub async fn delete_invite_code(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteInviteCodeResponse>, AppError> {
    auth.require_fresh_admin(&state).await?;

    let invite = state
        .db
        .get_invite_code(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Invite code {} not found", id)))?;

    state.db.delete_invite_code(id).await?;

    tracing::info!(
        "Admin '{}' deleted invite code {} (id={})",
        auth.username,
        invite.code,
        id,
    );

    Ok(Json(DeleteInviteCodeResponse {
        deleted: true,
        id: id.to_string(),
    }))
}

/// POST /api/auth/redeem-invite
///
/// Redeem an invite code to create a new account. Public route (no auth required).
pub async fn redeem_invite_code(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RedeemInviteCodeRequest>,
) -> Result<Response, AppError> {
    let settings = state.db.get_settings().await?;
    if !settings.setup_complete {
        return Err(AppError::BadRequest(
            "Initial setup has not been completed yet".into(),
        ));
    }

    // Validate username and password
    let username = req.username.trim().to_lowercase();
    validate_username(&username)?;
    validate_password(&req.password)?;

    // Check if username is taken
    if state.db.username_exists(&username).await? {
        return Err(AppError::Conflict(format!(
            "Username '{}' is already taken",
            username
        )));
    }

    // Normalize the submitted code (strip dashes/spaces, uppercase)
    let normalized = normalize_code(&req.code);

    // Constant error message for all code-lookup failures — prevents an
    // attacker from distinguishing "code not found" from "code expired"
    // or "code already redeemed".
    const INVALID_CODE_MSG: &str = "Invalid or expired invite code";

    // Look up the invite code
    let invite = match state.db.get_invite_code_by_code(&normalized).await? {
        Some(inv) => inv,
        None => {
            return Err(AppError::BadRequest(INVALID_CODE_MSG.into()));
        }
    };

    // Check if already redeemed — guard on both `redeemed_by` AND `redeemed_at`.
    // `redeemed_by` is subject to `ON DELETE SET NULL` when the redeemer is deleted,
    // but `redeemed_at` is a plain column unaffected by the FK cascade, so it
    // reliably indicates a code that was ever redeemed.
    if invite.redeemed_by.is_some() || invite.redeemed_at.is_some() {
        return Err(AppError::BadRequest(INVALID_CODE_MSG.into()));
    }

    // Check if expired
    if invite.expires_at < Utc::now() {
        return Err(AppError::BadRequest(INVALID_CODE_MSG.into()));
    }

    // Create the user account with capabilities from the invite code
    let password_hash = hash_password(&req.password)?;
    let user = User {
        id: Uuid::new_v4(),
        username: username.clone(),
        password_hash,
        role: invite.assigned_role,
        created_at: Utc::now(),
        token_generation: 0,
        global_capabilities: invite.assigned_capabilities.clone(),
    };

    state.db.insert_user(&user).await?;

    // Atomically mark the invite code as redeemed.
    // The DB-level UPDATE uses `WHERE redeemed_at IS NULL` so only one concurrent
    // request can claim the code. If we lose the race, clean up the orphaned user.
    let was_claimed = state.db.redeem_invite_code(&normalized, &user.id).await?;
    if !was_claimed {
        // Another request claimed the code between our check and this UPDATE.
        // Remove the user we just created so it doesn't linger as an orphan.
        state.db.delete_user(user.id).await.ok();
        return Err(AppError::Conflict(
            "This invite code has already been used".into(),
        ));
    }

    // Grant the assigned server permissions
    for grant in &invite.assigned_permissions {
        let perm = ServerPermission {
            user_id: user.id,
            server_id: grant.server_id,
            level: grant.level,
        };
        if let Err(e) = state.db.set_permission(&perm).await {
            tracing::warn!(
                "Failed to grant permission on server {} to new user '{}': {}",
                grant.server_id,
                username,
                e
            );
        }
    }

    tracing::info!(
        "User '{}' (id={}) registered via invite code {} (created by {})",
        username,
        user.id,
        invite.code,
        invite.created_by,
    );

    // Issue tokens (same flow as login/register)
    issue_tokens_for_user(&state, &user).await
}

/// GET /api/admin/user-permissions
///
/// List all users with their server permission summaries. Admin only.
pub async fn list_user_permissions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<UserPermissionListResponse>, AppError> {
    auth.require_fresh_admin(&state).await?;

    let summaries = state.db.list_user_permission_summaries().await?;
    let servers = state.db.list_servers().await?;

    let users: Vec<UserPermissionSummary> = summaries
        .into_iter()
        .map(|(user, perms)| {
            let mut server_permissions: Vec<UserServerPermission> = perms
                .into_iter()
                .map(|(perm, server_name)| UserServerPermission {
                    server_id: perm.server_id,
                    server_name,
                    level: perm.level,
                    is_implicit: false,
                })
                .collect();

            // Add implicit owner permissions for servers they own
            for server in &servers {
                if server.owner_id == user.id
                    && !server_permissions.iter().any(|p| p.server_id == server.id)
                {
                    server_permissions.push(UserServerPermission {
                        server_id: server.id,
                        server_name: server.config.name.clone(),
                        level: PermissionLevel::Owner,
                        is_implicit: true,
                    });
                }
            }

            // Global admins get implicit owner on all servers
            if user.role == Role::Admin {
                for server in &servers {
                    if !server_permissions.iter().any(|p| p.server_id == server.id) {
                        server_permissions.push(UserServerPermission {
                            server_id: server.id,
                            server_name: server.config.name.clone(),
                            level: PermissionLevel::Owner,
                            is_implicit: true,
                        });
                    }
                }
            }

            UserPermissionSummary {
                user_id: user.id,
                username: user.username,
                role: user.role,
                server_permissions,
            }
        })
        .collect();

    Ok(Json(UserPermissionListResponse { users }))
}

// ─── Helpers ───

fn to_public(
    invite: &InviteCode,
    creator_username: Option<String>,
    redeemer_username: Option<String>,
) -> InviteCodePublic {
    let now = Utc::now();
    // A code is active only if it has never been redeemed AND hasn't expired.
    // We check `redeemed_at` (survives user deletion) in addition to `redeemed_by`
    // so that codes whose redeemer was deleted still show as inactive.
    let is_active =
        invite.redeemed_by.is_none() && invite.redeemed_at.is_none() && invite.expires_at > now;

    InviteCodePublic {
        id: invite.id,
        code: invite.code.clone(),
        created_by: invite.created_by,
        created_by_username: creator_username,
        assigned_role: invite.assigned_role,
        assigned_permissions: invite.assigned_permissions.clone(),
        assigned_capabilities: invite.assigned_capabilities.clone(),
        expires_at: invite.expires_at,
        redeemed_by: invite.redeemed_by,
        redeemed_by_username: redeemer_username,
        redeemed_at: invite.redeemed_at,
        created_at: invite.created_at,
        label: invite.label.clone(),
        is_active,
    }
}

/// Issue access + refresh tokens and return the response with the refresh cookie.
/// Delegates to the canonical implementation in `api::auth` so the cookie path,
/// Secure flag, and SameSite policy stay in sync across all auth flows.
async fn issue_tokens_for_user(state: &AppState, user: &User) -> Result<Response, AppError> {
    super::auth::issue_tokens_response(state, user, None).await
}
