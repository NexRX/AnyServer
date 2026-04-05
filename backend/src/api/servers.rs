use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use crate::auth::{hash_password, AuthUser};
use crate::error::AppError;
use crate::server_management::stats::ServerResourceStats;
use crate::types::*;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DirectoryProcess {
    pub pid: u32,
    pub command: String,
    pub args: Vec<String>,
}

fn list_processes_in_directory(dir: &std::path::Path) -> Vec<DirectoryProcess> {
    let Ok(canon_dir) = dir.canonicalize() else {
        return vec![];
    };

    let my_pid = std::process::id();
    let mut found = Vec::new();

    let Ok(proc_entries) = std::fs::read_dir("/proc") else {
        return vec![];
    };

    for entry in proc_entries.flatten() {
        let name = entry.file_name();
        let Some(pid_str) = name.to_str() else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };

        if pid == my_pid {
            continue;
        }

        let proc_path = std::path::PathBuf::from("/proc").join(pid_str);

        let cwd_link = proc_path.join("cwd");
        let is_in_dir = std::fs::read_link(&cwd_link)
            .ok()
            .and_then(|p| p.canonicalize().ok())
            .is_some_and(|cwd| cwd.starts_with(&canon_dir));

        let has_open_fds = if !is_in_dir {
            let fd_dir = proc_path.join("fd");
            std::fs::read_dir(&fd_dir).is_ok_and(|fds| {
                fds.flatten().any(|fd_entry| {
                    std::fs::read_link(fd_entry.path())
                        .ok()
                        .and_then(|p| p.canonicalize().ok())
                        .is_some_and(|target| target.starts_with(&canon_dir))
                })
            })
        } else {
            false
        };

        if is_in_dir || has_open_fds {
            let comm = std::fs::read_to_string(proc_path.join("comm"))
                .unwrap_or_default()
                .trim()
                .to_string();

            let args = std::fs::read(proc_path.join("cmdline"))
                .ok()
                .map(|bytes| {
                    bytes
                        .split(|&b| b == 0)
                        .filter(|s| !s.is_empty())
                        .map(|s| String::from_utf8_lossy(s).into_owned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            found.push(DirectoryProcess {
                pid,
                command: comm,
                args,
            });
        }
    }

    found
}

/// Try to kill a process via `pidfd_open` + `pidfd_send_signal` (Linux 5.3+).
///
/// Returns:
/// - `Some(true)` if the signal was sent successfully via pidfd
/// - `Some(false)` if pidfd_open succeeded but the signal failed (e.g. permission)
/// - `None` if pidfd_open is not available (ENOSYS) — caller should fall back
fn try_kill_via_pidfd(pid: u32) -> Option<bool> {
    // pidfd_open(pid, flags) — syscall 434 on x86_64, 434 on aarch64
    #[cfg(target_arch = "x86_64")]
    const SYS_PIDFD_OPEN: libc::c_long = 434;
    #[cfg(target_arch = "aarch64")]
    const SYS_PIDFD_OPEN: libc::c_long = 434;

    // pidfd_send_signal(pidfd, sig, info, flags) — syscall 424 on x86_64, 424 on aarch64
    #[cfg(target_arch = "x86_64")]
    const SYS_PIDFD_SEND_SIGNAL: libc::c_long = 424;
    #[cfg(target_arch = "aarch64")]
    const SYS_PIDFD_SEND_SIGNAL: libc::c_long = 424;

    let fd = unsafe { libc::syscall(SYS_PIDFD_OPEN, pid as libc::c_int, 0 as libc::c_int) };

    if fd < 0 {
        let err = std::io::Error::last_os_error();
        return match err.raw_os_error() {
            Some(libc::ENOSYS) => None, // kernel too old — fall back
            Some(libc::ESRCH) => {
                // Process already exited — safe to skip
                tracing::debug!("pidfd_open: PID {} already exited, skipping", pid);
                Some(false)
            }
            _ => {
                tracing::debug!("pidfd_open failed for PID {}: {}", pid, err);
                None // fall back to re-verify approach
            }
        };
    }

    let result = unsafe {
        libc::syscall(
            SYS_PIDFD_SEND_SIGNAL,
            fd as libc::c_int,
            libc::SIGKILL,
            std::ptr::null::<libc::c_void>(),
            0 as libc::c_uint,
        )
    };

    unsafe { libc::close(fd as libc::c_int) };

    Some(result == 0)
}

/// Re-verify that `pid` still has its cwd inside `server_dir` before killing.
/// Returns `true` if the process was successfully signalled, `false` otherwise.
fn kill_with_reverify(pid: u32, server_dir: &std::path::Path) -> bool {
    let proc_cwd = format!("/proc/{}/cwd", pid);
    let still_ours = std::fs::read_link(&proc_cwd)
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .is_some_and(|cwd| cwd.starts_with(server_dir));

    if !still_ours {
        // Also check open FDs as a secondary signal
        let fd_dir = format!("/proc/{}/fd", pid);
        let has_our_fds = std::fs::read_dir(&fd_dir).is_ok_and(|fds| {
            fds.flatten().any(|fd_entry| {
                std::fs::read_link(fd_entry.path())
                    .ok()
                    .and_then(|p| p.canonicalize().ok())
                    .is_some_and(|target| target.starts_with(server_dir))
            })
        });

        if !has_our_fds {
            tracing::debug!(
                "PID {} no longer associated with server directory, skipping kill",
                pid
            );
            return false;
        }
    }

    let result = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    result == 0
}

fn kill_listed_processes(processes: &[DirectoryProcess]) -> Vec<(u32, String, bool)> {
    // Determine the server directory from the first process's /proc/pid/cwd
    // for the re-verify fallback path. We only need this if pidfd is unavailable.
    let server_dir: Option<std::path::PathBuf> = processes.first().and_then(|p| {
        let cwd_link = format!("/proc/{}/cwd", p.pid);
        std::fs::read_link(cwd_link)
            .ok()
            .and_then(|p| p.canonicalize().ok())
    });

    let mut results = Vec::with_capacity(processes.len());

    for proc in processes {
        let success = match try_kill_via_pidfd(proc.pid) {
            Some(ok) => ok,
            None => {
                // pidfd not available — fall back to re-verify + individual kill.
                // If we can't determine the server dir, still attempt the kill
                // (matches old behaviour) but log a warning.
                if let Some(ref dir) = server_dir {
                    kill_with_reverify(proc.pid, dir)
                } else {
                    tracing::warn!(
                        "Cannot re-verify PID {} (no server dir context) — \
                         sending SIGKILL without verification",
                        proc.pid
                    );
                    unsafe { libc::kill(proc.pid as i32, libc::SIGKILL) == 0 }
                }
            }
        };
        results.push((proc.pid, proc.command.clone(), success));
    }

    results
}

pub(crate) fn kill_processes_in_directory(dir: &std::path::Path) -> Vec<(u32, String)> {
    let found = list_processes_in_directory(dir);
    kill_listed_processes(&found)
        .into_iter()
        .map(|(pid, comm, _)| (pid, comm))
        .collect()
}
use crate::utils::blocking;
use crate::AppState;

fn validate_parameters(
    definitions: &[ConfigParameter],
    values: &std::collections::HashMap<String, String>,
) -> Result<(), AppError> {
    for param in definitions {
        let value = values.get(&param.name);
        let is_empty = value.is_none_or(|v| v.trim().is_empty());

        if param.required && is_empty {
            return Err(AppError::BadRequest(format!(
                "Parameter '{}' ({}) is required",
                param.name, param.label,
            )));
        }

        if let Some(val) = value {
            if val.trim().is_empty() {
                continue;
            }

            // Sanitize: reject null bytes and excessively long values
            // at the API boundary before they reach the pipeline engine.
            if let Err(e) = crate::pipeline::variables::sanitize_parameter_value(val) {
                return Err(AppError::BadRequest(format!(
                    "Parameter '{}': {}",
                    param.name, e,
                )));
            }

            if matches!(param.param_type, ConfigParameterType::Select)
                && !param.options.is_empty()
                && !param.options.contains(val)
            {
                return Err(AppError::BadRequest(format!(
                    "Parameter '{}' value '{}' is not one of the allowed options: {}",
                    param.name,
                    val,
                    param.options.join(", "),
                )));
            }

            if let Some(ref pattern) = param.regex {
                let re = regex::Regex::new(pattern).map_err(|e| {
                    AppError::Internal(format!(
                        "Invalid regex '{}' on parameter '{}': {}",
                        pattern, param.name, e,
                    ))
                })?;
                if !re.is_match(val) {
                    return Err(AppError::BadRequest(format!(
                        "Parameter '{}' value '{}' does not match the required pattern: {}",
                        param.name, val, pattern,
                    )));
                }
            }
        }
    }
    Ok(())
}

async fn build_server_with_status(
    state: &AppState,
    mut server: Server,
    auth: &AuthUser,
) -> Result<ServerWithStatus, AppError> {
    let runtime = state.process_manager.get_runtime(&server.id);

    let permission =
        auth.effective_permission(state, &server)
            .await?
            .unwrap_or(EffectivePermission {
                level: PermissionLevel::Viewer,
                is_global_admin: false,
            });

    let phase_progress = state.pipeline_manager.get_progress(&server.id);

    server.config.sftp_password = None;

    Ok(ServerWithStatus {
        server,
        runtime,
        permission,
        phase_progress,
    })
}

/// Enrich a list of servers with runtime status and permissions using a
/// pre-fetched permission map, avoiding the N+1 query pattern.
///
/// For admin users `user_permissions` should be an empty map — the admin
/// shortcut is handled inline.  Non-admin users who neither own a server
/// nor have an explicit permission row are excluded from the result.
fn enrich_servers_with_status(
    state: &AppState,
    servers: Vec<Server>,
    auth: &AuthUser,
    user_permissions: &HashMap<Uuid, PermissionLevel>,
) -> Vec<ServerWithStatus> {
    let start = std::time::Instant::now();

    let mut result = Vec::with_capacity(servers.len());
    for mut server in servers {
        let permission = if auth.is_admin() {
            EffectivePermission {
                level: PermissionLevel::Owner,
                is_global_admin: true,
            }
        } else if server.owner_id == auth.user_id {
            EffectivePermission {
                level: PermissionLevel::Owner,
                is_global_admin: false,
            }
        } else if let Some(&level) = user_permissions.get(&server.id) {
            EffectivePermission {
                level,
                is_global_admin: false,
            }
        } else {
            // User has no access — skip this server entirely.
            continue;
        };

        let runtime = state.process_manager.get_runtime(&server.id);
        let phase_progress = state.pipeline_manager.get_progress(&server.id);

        server.config.sftp_password = None;

        result.push(ServerWithStatus {
            server,
            runtime,
            permission,
            phase_progress,
        });
    }

    tracing::debug!(
        "Enriched {} servers in {:?} ({:.1} ms/server)",
        result.len(),
        start.elapsed(),
        start.elapsed().as_secs_f64() * 1000.0 / result.len().max(1) as f64,
    );

    result
}

/// GET /api/servers
pub async fn list_servers(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListServersParams>,
) -> Result<Json<PaginatedServerListResponse>, AppError> {
    let user_id = if auth.is_admin() {
        None
    } else {
        Some(&auth.user_id)
    };

    let per_page = params.per_page.clamp(1, 100);
    let page = params.page.max(1);

    // Pre-fetch all permissions for this user in ONE query to avoid N+1.
    // Admin users get Owner on everything via the inline shortcut, so we
    // skip the query entirely for them.
    let user_permissions = if auth.is_admin() {
        HashMap::new()
    } else {
        state
            .db
            .list_permissions_for_user_batch(&auth.user_id)
            .await?
    };

    if let Some(ref status_filter) = params.status {
        // ── Status filter path ──────────────────────────────────────────
        // Runtime status lives in-memory, not in the DB, so we must fetch
        // ALL accessible servers, build their runtime info, filter by
        // status, and then paginate the filtered result in application code.
        let all_servers = state
            .db
            .list_servers_all_filtered(
                params.search.as_deref(),
                &params.sort,
                &params.order,
                user_id,
            )
            .await?;

        let all_with_status =
            enrich_servers_with_status(&state, all_servers, &auth, &user_permissions);

        let status_lower = status_filter.to_lowercase();
        let filtered: Vec<ServerWithStatus> = all_with_status
            .into_iter()
            .filter(|s| {
                serde_json::to_value(s.runtime.status)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .is_some_and(|name| name == status_lower)
            })
            .collect();

        let total = filtered.len() as u64;
        let total_pages = ((total as f64) / (per_page as f64)).ceil() as u32;

        let offset = ((page - 1) * per_page) as usize;
        let servers: Vec<ServerWithStatus> = filtered
            .into_iter()
            .skip(offset)
            .take(per_page as usize)
            .collect();

        Ok(Json(PaginatedServerListResponse {
            servers,
            total,
            page,
            per_page,
            total_pages,
        }))
    } else {
        // ── No status filter — efficient DB-level pagination ────────────
        let (servers, total) = state
            .db
            .list_servers_paginated(
                page,
                per_page,
                params.search.as_deref(),
                params.status.as_deref(),
                &params.sort,
                &params.order,
                user_id,
            )
            .await?;

        let with_status = enrich_servers_with_status(&state, servers, &auth, &user_permissions);

        let total_pages = ((total as f64) / (per_page as f64)).ceil() as u32;

        Ok(Json(PaginatedServerListResponse {
            servers: with_status,
            total,
            page,
            per_page,
            total_pages,
        }))
    }
}

/// GET /api/servers/:id
pub async fn get_server(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ServerWithStatus>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Viewer)
        .await?;

    Ok(Json(build_server_with_status(&state, server, &auth).await?))
}

/// POST /api/servers
pub async fn create_server(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(mut req): Json<CreateServerRequest>,
) -> Result<Json<ServerWithStatus>, AppError> {
    // Only users with the CreateServers capability (or admins) may create servers.
    auth.require_capability(&state, crate::types::GlobalCapability::CreateServers)
        .await?;

    if req.config.name.trim().is_empty() {
        return Err(AppError::BadRequest("Server name is required".into()));
    }
    if req.config.binary.trim().is_empty() {
        return Err(AppError::BadRequest("Binary path is required".into()));
    }

    validate_parameters(&req.config.parameters, &req.parameter_values)?;

    if let Some(password) = &req.config.sftp_password {
        if !password.is_empty() {
            req.config.sftp_password = Some(hash_password(password)?);
        }
    }

    let now = Utc::now();
    let server = Server {
        id: Uuid::new_v4(),
        owner_id: auth.user_id,
        config: req.config,
        created_at: now,
        updated_at: now,
        parameter_values: req.parameter_values,
        installed: false,
        installed_at: None,
        updated_via_pipeline_at: None,
        installed_version: None,
        source_template_id: req.source_template_id,
    };

    let server_dir = state.server_dir(&server.id);
    std::fs::create_dir_all(&server_dir).map_err(|e| {
        AppError::Internal(format!(
            "Failed to create server directory {:?}: {}",
            server_dir, e
        ))
    })?;

    state.db.insert_server(&server).await?;

    let perm = ServerPermission {
        user_id: auth.user_id,
        server_id: server.id,
        level: PermissionLevel::Owner,
    };
    state.db.set_permission(&perm).await?;

    tracing::info!(
        "User '{}' created server '{}' (id={}), data dir={:?}",
        auth.username,
        server.config.name,
        server.id,
        server_dir,
    );

    Ok(Json(build_server_with_status(&state, server, &auth).await?))
}

/// PUT /api/servers/:id
pub async fn update_server(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(mut req): Json<UpdateServerRequest>,
) -> Result<Json<ServerWithStatus>, AppError> {
    let mut server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Manager)
        .await?;

    if req.config.name.trim().is_empty() {
        return Err(AppError::BadRequest("Server name is required".into()));
    }
    if req.config.binary.trim().is_empty() {
        return Err(AppError::BadRequest("Binary path is required".into()));
    }

    validate_parameters(&req.config.parameters, &req.parameter_values)?;

    // None = keep existing, Some("") = clear, Some(pw) = hash new password.
    match &req.config.sftp_password {
        None => {
            req.config.sftp_password = server.config.sftp_password.clone();
        }
        Some(password) if !password.is_empty() => {
            req.config.sftp_password = Some(hash_password(password)?);
        }
        Some(_) => {
            req.config.sftp_password = None;
        }
    }

    server.config = req.config;
    server.parameter_values = req.parameter_values;
    server.updated_at = Utc::now();

    state.db.update_server(&server).await?;

    tracing::info!(
        "User '{}' updated server '{}' (id={})",
        auth.username,
        server.config.name,
        server.id,
    );

    Ok(Json(build_server_with_status(&state, server, &auth).await?))
}

/// DELETE /api/servers/:id
pub async fn delete_server(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteServerResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level_verified(&state, &server, PermissionLevel::Admin)
        .await?;

    let _ = crate::server_management::process::stop_server(&state, id).await;
    state.process_manager.handles.remove(&id);
    state.db.delete_server(id).await?;

    let server_dir = state.server_dir(&id);
    if server_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&server_dir) {
            tracing::warn!("Failed to remove server directory {:?}: {}", server_dir, e);
        }
    }

    tracing::info!(
        "User '{}' deleted server '{}' (id={}) and removed files at {:?}",
        auth.username,
        server.config.name,
        id,
        server_dir,
    );

    Ok(Json(DeleteServerResponse {
        deleted: true,
        id: id.to_string(),
    }))
}

/// POST /api/servers/:id/reset
pub async fn reset_server(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ResetServerResponse>, AppError> {
    let mut server = state.db.require_server(id).await?;

    auth.require_level_verified(&state, &server, PermissionLevel::Admin)
        .await?;

    let _ = crate::server_management::process::stop_server(&state, id).await;

    let server_dir = state.server_dir(&id);
    let killed = blocking({
        let dir = server_dir.clone();
        move || Ok(kill_processes_in_directory(&dir))
    })
    .await?;
    if !killed.is_empty() {
        tracing::info!(
            "Reset: killed {} orphaned process(es) in {:?}: {:?}",
            killed.len(),
            server_dir,
            killed,
        );
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    if server_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&server_dir) {
            tracing::warn!(
                "Reset: failed to remove server directory {:?}: {}",
                server_dir,
                e
            );
            return Err(AppError::Internal(format!(
                "Failed to remove server files: {}. Some processes may still be holding file locks.",
                e
            )));
        }
    }

    std::fs::create_dir_all(&server_dir).map_err(|e| {
        AppError::Internal(format!(
            "Failed to recreate server directory {:?}: {}",
            server_dir, e
        ))
    })?;

    server.installed = false;
    server.installed_at = None;
    server.updated_via_pipeline_at = None;
    server.updated_at = Utc::now();
    state.db.update_server(&server).await?;

    state.pipeline_manager.active.remove(&id);

    tracing::info!(
        "User '{}' reset server '{}' (id={}). All files removed, marked as uninstalled.",
        auth.username,
        server.config.name,
        id,
    );

    Ok(Json(ResetServerResponse {
        reset: true,
        id: id.to_string(),
        killed_processes: killed.len(),
    }))
}

/// POST /api/servers/:id/mark-installed
///
/// Marks a server as installed without running the install pipeline.
/// Requires at least Admin permission on the server.
pub async fn mark_installed(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<MarkInstalledResponse>, AppError> {
    let mut server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Admin)
        .await?;

    if server.installed {
        return Err(AppError::Conflict(
            "Server is already marked as installed".into(),
        ));
    }

    let now = Utc::now();
    server.installed = true;
    server.installed_at = Some(now);
    server.updated_at = now;
    state.db.update_server(&server).await?;

    tracing::info!(
        "User '{}' marked server '{}' (id={}) as installed (skipped install pipeline)",
        auth.username,
        server.config.name,
        id,
    );

    Ok(Json(MarkInstalledResponse {
        server_id: id,
        installed: true,
        installed_at: Some(now),
    }))
}

/// GET /api/servers/:id/directory-processes
pub async fn list_directory_processes(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Admin)
        .await?;

    let server_dir = state.server_dir(&id);
    let found = blocking(move || Ok(list_processes_in_directory(&server_dir))).await?;

    tracing::info!(
        "User '{}' listed directory processes for server '{}' (id={}) — {} found",
        auth.username,
        server.config.name,
        id,
        found.len(),
    );

    let process_list: Vec<serde_json::Value> = found
        .iter()
        .map(|p| {
            serde_json::json!({
                "pid": p.pid,
                "command": p.command,
                "args": p.args,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "count": found.len(),
        "processes": process_list
    })))
}

/// POST /api/servers/:id/kill-directory-processes
pub async fn kill_directory_processes(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<KillDirectoryProcessesResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level_verified(&state, &server, PermissionLevel::Admin)
        .await?;

    let server_dir = state.server_dir(&id);
    let (found, results) = blocking({
        let dir = server_dir.clone();
        move || {
            let found = list_processes_in_directory(&dir);
            let results = kill_listed_processes(&found);
            Ok((found, results))
        }
    })
    .await?;

    let _ = found; // listing used internally by kill_listed_processes
    if results.is_empty() {
        tracing::info!(
            "User '{}' ran kill-directory-processes for server '{}' (id={}) — no processes found",
            auth.username,
            server.config.name,
            id,
        );
    } else {
        tracing::warn!(
            "User '{}' killed {} process(es) in {:?} for server '{}' (id={}): {:?}",
            auth.username,
            results.len(),
            &server_dir,
            server.config.name,
            id,
            results,
        );
    }

    let process_list: Vec<KillProcessResult> = results
        .iter()
        .map(|(pid, comm, success)| KillProcessResult {
            pid: *pid,
            command: comm.clone(),
            success: *success,
        })
        .collect();

    let killed_count = results.iter().filter(|(_, _, ok)| *ok).count();
    let failed_count = results.len() - killed_count;

    Ok(Json(KillDirectoryProcessesResponse {
        killed: killed_count,
        failed: failed_count,
        processes: process_list,
    }))
}

/// POST /api/servers/:id/start
/// Helper: authenticate as Operator, perform an action, log it, and return the runtime.
async fn server_control_action<F, Fut>(
    state: &Arc<AppState>,
    auth: &AuthUser,
    id: Uuid,
    verb: &str,
    action: F,
) -> Result<Json<ServerRuntime>, AppError>
where
    F: FnOnce(Arc<AppState>, Uuid) -> Fut,
    Fut: std::future::Future<Output = Result<(), AppError>>,
{
    let server = state.db.require_server(id).await?;
    auth.require_level(state, &server, PermissionLevel::Operator)
        .await?;

    action(Arc::clone(state), id).await?;
    let runtime = state.process_manager.get_runtime(&id);

    tracing::info!(
        "User '{}' {} server '{}' (id={})",
        auth.username,
        verb,
        server.config.name,
        id,
    );

    Ok(Json(runtime))
}

pub async fn start(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ServerRuntime>, AppError> {
    server_control_action(&state, &auth, id, "started", |s, id| async move {
        crate::server_management::process::start_server(&s, id).await
    })
    .await
}

/// POST /api/servers/:id/stop
pub async fn stop(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ServerRuntime>, AppError> {
    server_control_action(&state, &auth, id, "stopped", |s, id| async move {
        crate::server_management::process::stop_server(&s, id).await
    })
    .await
}

/// POST /api/servers/:id/cancel-stop
pub async fn cancel_stop(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<CancelStopResponse>, AppError> {
    let server = state.db.require_server(id).await?;
    auth.require_level(&state, &server, PermissionLevel::Operator)
        .await?;

    crate::server_management::process::cancel_stop_server(&state, id)?;

    tracing::info!(
        "User '{}' cancelled shutdown for server '{}' (id={})",
        auth.username,
        server.config.name,
        id,
    );

    Ok(Json(CancelStopResponse {
        cancelled: true,
        server_id: id,
    }))
}

/// POST /api/servers/:id/restart
pub async fn restart(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ServerRuntime>, AppError> {
    server_control_action(&state, &auth, id, "restarted", |s, id| async move {
        let _ = crate::server_management::process::stop_server(&s, id).await;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        crate::server_management::process::start_server(&s, id).await
    })
    .await
}

/// POST /api/servers/:id/cancel-restart
pub async fn cancel_restart(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ServerRuntime>, AppError> {
    server_control_action(
        &state,
        &auth,
        id,
        "cancelled auto-restart for",
        |s, id| async move { crate::server_management::process::cancel_restart(&s, id).await },
    )
    .await
}

/// POST /api/servers/:id/command
pub async fn send_command(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SendCommandRequest>,
) -> Result<Json<SendCommandResponse>, AppError> {
    if req.command.is_empty() {
        return Err(AppError::BadRequest("Command cannot be empty".into()));
    }

    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Operator)
        .await?;

    crate::server_management::process::send_command(&state, id, &req.command).await?;

    Ok(Json(SendCommandResponse {
        sent: true,
        command: req.command,
    }))
}

/// POST /api/servers/:id/sigint — sends SIGINT directly without the
/// graceful stop flow (no timeout, no SIGKILL fallback).
pub async fn send_sigint(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<SendSignalResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Operator)
        .await?;

    let handle = crate::server_management::process::get_handle(&state, &id)
        .ok_or_else(|| AppError::Conflict("Server has no active process".into()))?;

    let pid = {
        let rt = handle.runtime.lock();
        if rt.status != crate::types::ServerStatus::Running
            && rt.status != crate::types::ServerStatus::Starting
        {
            return Err(AppError::Conflict("Server is not running".into()));
        }
        rt.pid
    };

    if let Some(pid) = pid {
        tracing::info!(
            "User '{}' sent SIGINT (Ctrl+C) to server '{}' (id={}, pid={})",
            auth.username,
            server.config.name,
            id,
            pid,
        );
        unsafe {
            libc::kill(-(pid as i32), libc::SIGINT);
        }
        Ok(Json(SendSignalResponse {
            sent: true,
            signal: "SIGINT".to_string(),
            pid,
        }))
    } else {
        Err(AppError::Conflict("Server process has no PID".into()))
    }
}

/// GET /api/servers/:id/stats
pub async fn get_server_stats(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ServerResourceStats>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Viewer)
        .await?;

    let stats = state.stats_collector.get(&id).unwrap_or_else(|| {
        let dir = state.server_dir(&id);
        ServerResourceStats {
            server_id: id,
            cpu_percent: None,
            memory_rss_bytes: None,
            memory_swap_bytes: None,
            disk_usage_bytes: crate::server_management::stats::dir_size_public(&dir),
            timestamp: chrono::Utc::now(),
        }
    });

    Ok(Json(stats))
}
