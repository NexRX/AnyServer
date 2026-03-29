use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::pipeline;
use crate::server_management::process;
use crate::templates::update_check::find_version_param_name;
use crate::types::*;
use crate::AppState;

async fn run_pipeline_phase(
    state: Arc<AppState>,
    auth: AuthUser,
    id: Uuid,
    phase: PhaseKind,
    req: Option<RunPhaseRequest>,
    default_steps_fn: fn(&ServerConfig) -> &Vec<PipelineStep>,
) -> Result<Json<RunPhaseResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Manager)
        .await?;

    let (steps, parameter_overrides) = match req {
        Some(r) => (
            r.steps_override
                .unwrap_or_else(|| default_steps_fn(&server.config).clone()),
            r.parameter_overrides,
        ),
        None => (default_steps_fn(&server.config).clone(), None),
    };

    let phase_label = match phase {
        PhaseKind::Start => "start",
        PhaseKind::Install => "install",
        PhaseKind::Update => "update",
        PhaseKind::Uninstall => "uninstall",
    };

    if steps.is_empty() {
        return Err(AppError::BadRequest(format!(
            "No {phase_label} steps defined. Add {phase_label}_steps to the server config \
             or provide steps_override in the request body."
        )));
    }

    let runtime = state.process_manager.get_runtime(&id);
    if runtime.status == ServerStatus::Running || runtime.status == ServerStatus::Starting {
        return Err(AppError::Conflict(format!(
            "Cannot {phase_label} while the server is running. Stop the server first."
        )));
    }

    // For Update pipelines, automatically inject the latest version from
    // the update-check cache when the caller did not explicitly override
    // the version parameter.  This ensures that clicking "Update" in the
    // UI actually downloads the *new* version rather than re-downloading
    // the currently installed one.
    let parameter_overrides = if phase == PhaseKind::Update {
        inject_latest_version(&state, id, &server.config.parameters, parameter_overrides)
    } else {
        parameter_overrides
    };

    pipeline::run_phase(&state, id, phase, steps, parameter_overrides)?;

    tracing::info!(
        "User '{}' started {} pipeline for server '{}' (id={})",
        auth.username,
        phase_label,
        server.config.name,
        id,
    );

    Ok(Json(RunPhaseResponse {
        server_id: id,
        phase,
        status: PhaseStatus::Running,
        message: format!("{} pipeline started", capitalize(phase_label)),
    }))
}

/// If the server has a version parameter (is_version == true) and the
/// update cache contains a newer version, inject it into the parameter
/// overrides map so the pipeline downloads the right artefact.
fn inject_latest_version(
    state: &Arc<AppState>,
    server_id: Uuid,
    parameters: &[ConfigParameter],
    overrides: Option<HashMap<String, String>>,
) -> Option<HashMap<String, String>> {
    let version_param = match find_version_param_name(parameters) {
        Some(name) => name,
        None => return overrides, // no version parameter → nothing to inject
    };

    // If the caller already provided an explicit override for the version
    // parameter, respect it.
    if let Some(ref ov) = overrides {
        if ov.contains_key(&version_param) {
            return overrides;
        }
    }

    // Look up the cached update-check result.
    let latest = state.update_cache.get(&server_id).and_then(|r| {
        if r.update_available {
            r.latest_version.clone()
        } else {
            None
        }
    });

    match latest {
        Some(version) => {
            tracing::info!(
                "Auto-injecting version parameter '{}' = '{}' for update pipeline on server {}",
                version_param,
                version,
                server_id,
            );
            let mut map = overrides.unwrap_or_default();
            map.insert(version_param, version);
            Some(map)
        }
        None => overrides,
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().chain(c).collect(),
    }
}

/// POST /api/servers/:id/install
pub async fn install(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<Option<RunPhaseRequest>>,
) -> Result<Json<RunPhaseResponse>, AppError> {
    run_pipeline_phase(state, auth, id, PhaseKind::Install, req, |c| {
        &c.install_steps
    })
    .await
}

/// POST /api/servers/:id/update
pub async fn update(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<Option<RunPhaseRequest>>,
) -> Result<Json<RunPhaseResponse>, AppError> {
    run_pipeline_phase(state, auth, id, PhaseKind::Update, req, |c| &c.update_steps).await
}

/// POST /api/servers/:id/uninstall
pub async fn uninstall(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<Option<RunPhaseRequest>>,
) -> Result<Json<RunPhaseResponse>, AppError> {
    run_pipeline_phase(state, auth, id, PhaseKind::Uninstall, req, |c| {
        &c.uninstall_steps
    })
    .await
}

/// POST /api/servers/:id/kill — immediate SIGKILL without graceful stop flow.
pub async fn kill(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ServerRuntime>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Operator)
        .await?;

    process::kill_server(&state, id).await?;
    let runtime = state.process_manager.get_runtime(&id);

    tracing::info!(
        "User '{}' force-killed server '{}' (id={})",
        auth.username,
        server.config.name,
        id,
    );

    Ok(Json(runtime))
}

/// GET /api/servers/:id/phase-status
pub async fn phase_status(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<PhaseStatusResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Viewer)
        .await?;

    let progress = state.pipeline_manager.get_progress(&id);

    Ok(Json(PhaseStatusResponse {
        server_id: id,
        progress,
        installed: server.installed,
        installed_at: server.installed_at,
        updated_via_pipeline_at: server.updated_via_pipeline_at,
    }))
}

/// POST /api/servers/:id/cancel-phase
pub async fn cancel_phase(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<CancelPhaseResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Manager)
        .await?;

    let handle = state
        .pipeline_manager
        .active
        .get(&id)
        .ok_or_else(|| AppError::NotFound("No pipeline is active for this server".into()))?;

    {
        let progress = handle.progress.lock();
        if progress.status != PhaseStatus::Running {
            return Err(AppError::Conflict(
                "Pipeline is not currently running".into(),
            ));
        }
    }

    {
        let mut task_guard = handle.task_handle.lock();
        if let Some(task) = task_guard.take() {
            task.abort();
        }
    }

    {
        let mut progress = handle.progress.lock();
        progress.status = PhaseStatus::Failed;
        progress.completed_at = Some(chrono::Utc::now());
        for sp in progress.steps.iter_mut() {
            if sp.status == PhaseStatus::Running || sp.status == PhaseStatus::Pending {
                sp.status = PhaseStatus::Failed;
                sp.message = Some("Cancelled by user".to_string());
                sp.completed_at = Some(chrono::Utc::now());
            }
        }
    }

    let final_progress = handle.progress.lock().clone();
    let _ = handle.log_tx.send(WsMessage::PhaseProgress(final_progress));

    tracing::info!(
        "User '{}' cancelled pipeline for server '{}' (id={})",
        auth.username,
        server.config.name,
        id,
    );

    Ok(Json(CancelPhaseResponse {
        cancelled: true,
        server_id: id.to_string(),
    }))
}
