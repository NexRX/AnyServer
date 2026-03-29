use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::types::*;
use crate::AppState;

/// GET /api/servers/:id/sandbox
///
/// Get the sandbox profile for a server instance.
/// Returns the profile (or defaults) along with host capabilities.
/// Requires Admin-level permission on the server.
pub async fn get_sandbox_profile(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(server_id): Path<Uuid>,
) -> Result<Json<SandboxProfileResponse>, AppError> {
    let server = state.db.require_server(server_id).await?;
    auth.require_level(&state, &server, PermissionLevel::Admin)
        .await?;

    let feature_enabled = state.db.is_sandbox_management_enabled().await?;

    let profile = state
        .db
        .get_sandbox_profile(&server_id)
        .await?
        .unwrap_or_else(|| SandboxProfile {
            server_id,
            updated_at: Utc::now(),
            ..Default::default()
        });

    let capabilities = probe_sandbox_capabilities(feature_enabled);

    Ok(Json(SandboxProfileResponse {
        profile,
        capabilities,
    }))
}

/// PUT /api/servers/:id/sandbox
///
/// Update the sandbox profile for a server instance.
/// Requires the sandbox management feature to be enabled site-wide (owner only).
/// Requires Admin-level permission on the server.
pub async fn update_sandbox_profile(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(server_id): Path<Uuid>,
    Json(req): Json<UpdateSandboxProfileRequest>,
) -> Result<Json<SandboxProfileResponse>, AppError> {
    let server = state.db.require_server(server_id).await?;
    auth.require_level(&state, &server, PermissionLevel::Admin)
        .await?;

    let feature_enabled = state.db.is_sandbox_management_enabled().await?;
    if !feature_enabled {
        return Err(AppError::Forbidden(
            "Sandbox management is not enabled. The site owner must enable this feature first."
                .into(),
        ));
    }

    // Validate seccomp_mode
    match req.seccomp_mode.as_str() {
        "off" | "basic" | "strict" => {}
        other => {
            return Err(AppError::BadRequest(format!(
                "Invalid seccomp_mode '{}'. Must be one of: off, basic, strict",
                other
            )));
        }
    }

    // Validate paths don't contain obviously dangerous entries
    for path in req.extra_read_paths.iter().chain(req.extra_rw_paths.iter()) {
        if path.is_empty() {
            return Err(AppError::BadRequest(
                "Extra paths must not be empty strings".into(),
            ));
        }
        if !path.starts_with('/') {
            return Err(AppError::BadRequest(format!(
                "Extra path '{}' must be an absolute path (start with /)",
                path
            )));
        }
    }

    let now = Utc::now();
    let profile = SandboxProfile {
        server_id,
        enabled: req.enabled,
        landlock_enabled: req.landlock_enabled,
        no_new_privs: req.no_new_privs,
        fd_cleanup: req.fd_cleanup,
        non_dumpable: req.non_dumpable,
        namespace_isolation: req.namespace_isolation,
        pids_max: req.pids_max,
        extra_read_paths: req.extra_read_paths,
        extra_rw_paths: req.extra_rw_paths,
        network_isolation: req.network_isolation,
        seccomp_mode: req.seccomp_mode,
        updated_at: now,
    };

    state.db.upsert_sandbox_profile(&profile).await?;

    tracing::info!(
        "User '{}' updated sandbox profile for server '{}' ({})",
        auth.username,
        server.config.name,
        server_id,
    );

    let capabilities = probe_sandbox_capabilities(feature_enabled);

    Ok(Json(SandboxProfileResponse {
        profile,
        capabilities,
    }))
}

/// DELETE /api/servers/:id/sandbox
///
/// Reset the sandbox profile to defaults by deleting the custom profile.
/// Requires the sandbox management feature to be enabled site-wide.
/// Requires Admin-level permission on the server.
pub async fn reset_sandbox_profile(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(server_id): Path<Uuid>,
) -> Result<Json<SandboxProfileResponse>, AppError> {
    let server = state.db.require_server(server_id).await?;
    auth.require_level(&state, &server, PermissionLevel::Admin)
        .await?;

    let feature_enabled = state.db.is_sandbox_management_enabled().await?;
    if !feature_enabled {
        return Err(AppError::Forbidden(
            "Sandbox management is not enabled. The site owner must enable this feature first."
                .into(),
        ));
    }

    state.db.delete_sandbox_profile(&server_id).await?;

    tracing::info!(
        "User '{}' reset sandbox profile for server '{}' ({}) to defaults",
        auth.username,
        server.config.name,
        server_id,
    );

    let default = SandboxProfile {
        server_id,
        updated_at: Utc::now(),
        ..Default::default()
    };

    let capabilities = probe_sandbox_capabilities(feature_enabled);

    Ok(Json(SandboxProfileResponse {
        profile: default,
        capabilities,
    }))
}

/// GET /api/admin/sandbox/capabilities
///
/// Get the sandbox capabilities of the host system and feature flag status.
/// Admin only.
pub async fn get_sandbox_capabilities(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<SandboxCapabilities>, AppError> {
    auth.require_fresh_admin(&state).await?;

    let feature_enabled = state.db.is_sandbox_management_enabled().await?;
    let capabilities = probe_sandbox_capabilities(feature_enabled);

    Ok(Json(capabilities))
}

/// PUT /api/admin/sandbox/feature
///
/// Toggle the sandbox management feature flag site-wide.
/// Only the site owner (first admin / admin role) can do this.
/// This is the feature flag that gates access to per-server sandbox configuration.
pub async fn toggle_sandbox_feature(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<ToggleSandboxFeatureRequest>,
) -> Result<Json<ToggleSandboxFeatureResponse>, AppError> {
    // Only admin can toggle this — acts as "owner" since there's no separate owner role
    auth.require_fresh_admin(&state).await?;

    state.db.set_sandbox_management_enabled(req.enabled).await?;

    tracing::info!(
        "Admin '{}' {} sandbox management feature",
        auth.username,
        if req.enabled { "enabled" } else { "disabled" },
    );

    Ok(Json(ToggleSandboxFeatureResponse {
        sandbox_management_enabled: req.enabled,
    }))
}

// ─── Helpers ───

/// Probe the host system for sandbox capabilities.
fn probe_sandbox_capabilities(feature_enabled: bool) -> SandboxCapabilities {
    #[cfg(target_os = "linux")]
    {
        let landlock_abi = crate::sandbox::landlock::probe_abi_version();
        let namespaces_available = crate::sandbox::namespaces::probe_namespace_support();

        SandboxCapabilities {
            landlock_available: landlock_abi.is_some(),
            landlock_abi_version: landlock_abi,
            namespaces_available,
            no_new_privs_available: true,
            fd_cleanup_available: true,
            non_dumpable_available: true,
            rlimit_nproc_available: true,
            feature_enabled,
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        SandboxCapabilities {
            landlock_available: false,
            landlock_abi_version: None,
            namespaces_available: false,
            no_new_privs_available: false,
            fd_cleanup_available: false,
            non_dumpable_available: false,
            rlimit_nproc_available: false,
            feature_enabled,
        }
    }
}
