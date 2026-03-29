use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use super::executors::execute_step;
use super::variables::{build_variables, check_condition};
use super::PipelineHandle;
use crate::templates::update_check::find_version_param_name;
use crate::types::*;
use crate::AppState;

/// Snapshot the current value of the version parameter (marked with
/// `is_version: true`) into `server.installed_version`.  Called after a
/// successful Install or Update pipeline.
fn snapshot_installed_version(server: &mut Server, server_id: Uuid) {
    if let Some(param_name) = find_version_param_name(&server.config.parameters) {
        let version = server
            .parameter_values
            .get(&param_name)
            .cloned()
            .or_else(|| {
                server
                    .config
                    .parameters
                    .iter()
                    .find(|p| p.name == param_name)
                    .and_then(|p| p.default.clone())
            });
        if let Some(v) = version {
            tracing::info!(
                "Recording installed_version='{}' for server {}",
                v,
                server_id
            );
            server.installed_version = Some(v);
        }
    }
}

pub async fn run_pipeline_task(
    state: Arc<AppState>,
    server_id: Uuid,
    phase: PhaseKind,
    steps: Vec<PipelineStep>,
    handle: Arc<PipelineHandle>,
    parameter_overrides: Option<HashMap<String, String>>,
) {
    let server_dir = state.server_dir(&server_id);

    let server = match state.db.get_server(server_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            tracing::error!("Pipeline: server {} not found", server_id);
            set_phase_failed(&handle, "Server not found".to_string());
            return;
        }
        Err(e) => {
            tracing::error!("Pipeline: failed to load server {}: {}", server_id, e);
            set_phase_failed(&handle, format!("Failed to load server: {}", e));
            return;
        }
    };

    // Clone overrides so we can apply them to server.parameter_values
    // after a successful Install/Update pipeline.
    let overrides_snapshot = parameter_overrides.clone();

    let mut vars = build_variables(&server, &server_dir, parameter_overrides.as_ref());
    let mut overall_failed = false;

    for (i, step) in steps.iter().enumerate() {
        let step_index = i as u32;

        match check_condition(&step.condition, &server_dir, &vars) {
            Ok(true) => {}
            Ok(false) => {
                {
                    let mut progress = handle.progress.lock();
                    if let Some(sp) = progress.steps.get_mut(i) {
                        sp.status = PhaseStatus::Skipped;
                        sp.message = Some("Condition not met, skipped".to_string());
                    }
                }
                handle.broadcast_progress();
                handle.emit_log(
                    phase,
                    step_index,
                    &step.name,
                    "Step skipped: condition not met".to_string(),
                    LogStream::Stdout,
                );
                continue;
            }
            Err(e) => {
                {
                    let mut progress = handle.progress.lock();
                    if let Some(sp) = progress.steps.get_mut(i) {
                        sp.status = PhaseStatus::Failed;
                        sp.message = Some(format!("Condition check failed: {}", e));
                        sp.completed_at = Some(Utc::now());
                    }
                }
                handle.broadcast_progress();
                if !step.continue_on_error {
                    overall_failed = true;
                    break;
                }
                continue;
            }
        }

        // Mark step as running
        {
            let mut progress = handle.progress.lock();
            if let Some(sp) = progress.steps.get_mut(i) {
                sp.status = PhaseStatus::Running;
                sp.started_at = Some(Utc::now());
            }
        }
        handle.broadcast_progress();

        handle.emit_log(
            phase,
            step_index,
            &step.name,
            format!("▶ Starting step: {}", step.name),
            LogStream::Stdout,
        );

        let result = execute_step(
            &handle,
            &state,
            server_id,
            phase,
            step_index,
            step,
            &server_dir,
            &mut vars,
        )
        .await;

        match result {
            Ok(()) => {
                {
                    let mut progress = handle.progress.lock();
                    if let Some(sp) = progress.steps.get_mut(i) {
                        sp.status = PhaseStatus::Completed;
                        sp.completed_at = Some(Utc::now());
                    }
                }
                handle.broadcast_progress();
                handle.emit_log(
                    phase,
                    step_index,
                    &step.name,
                    format!("✓ Step completed: {}", step.name),
                    LogStream::Stdout,
                );
            }
            Err(e) => {
                {
                    let mut progress = handle.progress.lock();
                    if let Some(sp) = progress.steps.get_mut(i) {
                        sp.status = PhaseStatus::Failed;
                        sp.message = Some(e.clone());
                        sp.completed_at = Some(Utc::now());
                    }
                }
                handle.broadcast_progress();
                handle.emit_log(
                    phase,
                    step_index,
                    &step.name,
                    format!("✗ Step failed: {} — {}", step.name, e),
                    LogStream::Stderr,
                );

                if !step.continue_on_error {
                    overall_failed = true;
                    break;
                }
            }
        }
    }

    // Finalize the phase
    let final_status = if overall_failed {
        PhaseStatus::Failed
    } else {
        PhaseStatus::Completed
    };

    {
        let mut progress = handle.progress.lock();
        progress.status = final_status;
        progress.completed_at = Some(Utc::now());
    }
    handle.broadcast_progress();

    // Update the server's installation state in the database
    if final_status == PhaseStatus::Completed {
        if let Ok(Some(mut server)) = state.db.get_server(server_id).await {
            let now = Utc::now();
            match phase {
                PhaseKind::Install | PhaseKind::Update => {
                    // Merge parameter overrides into the persisted
                    // parameter_values so that snapshot_installed_version
                    // (and future pipelines) see the values that were
                    // actually used during this run.
                    if let Some(ref overrides) = overrides_snapshot {
                        for (k, v) in overrides {
                            server.parameter_values.insert(k.clone(), v.clone());
                        }
                    }
                    if phase == PhaseKind::Install {
                        server.installed = true;
                        server.installed_at = Some(now);
                    } else {
                        server.updated_via_pipeline_at = Some(now);
                        // Clear the cached update-check result so the
                        // "update available" notice disappears.
                        state.update_cache.remove(&server_id);
                    }
                    snapshot_installed_version(&mut server, server_id);
                }
                PhaseKind::Uninstall => {
                    server.installed = false;
                    server.installed_at = None;
                }
                PhaseKind::Start => {
                    // No DB state change for pre-start hooks
                }
            }
            server.updated_at = now;
            if let Err(e) = state.db.update_server(&server).await {
                tracing::error!(
                    "Pipeline: failed to update server {} after successful {}: {}",
                    server_id,
                    match phase {
                        PhaseKind::Start => "start",
                        PhaseKind::Install => "install",
                        PhaseKind::Update => "update",
                        PhaseKind::Uninstall => "uninstall",
                    },
                    e
                );
            }
        }
    }

    let phase_label = match phase {
        PhaseKind::Start => "Start",
        PhaseKind::Install => "Install",
        PhaseKind::Update => "Update",
        PhaseKind::Uninstall => "Uninstall",
    };

    match final_status {
        PhaseStatus::Completed => {
            tracing::info!(
                "{} pipeline completed successfully for server {}",
                phase_label,
                server_id
            );
        }
        PhaseStatus::Failed => {
            tracing::warn!("{} pipeline failed for server {}", phase_label, server_id);
        }
        _ => {}
    }

    // Leave the handle in the active map so the final progress can be queried.
    // It will be replaced on the next run.
}

pub fn set_phase_failed(handle: &Arc<PipelineHandle>, message: String) {
    let mut progress = handle.progress.lock();
    progress.status = PhaseStatus::Failed;
    progress.completed_at = Some(Utc::now());
    for sp in progress.steps.iter_mut() {
        if sp.status == PhaseStatus::Pending {
            sp.status = PhaseStatus::Failed;
            sp.message = Some(message.clone());
            break;
        }
    }
    drop(progress);
    handle.broadcast_progress();
}
