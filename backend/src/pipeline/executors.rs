use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;

use super::archive::{
    detect_archive_format, extract_tar, extract_tar_bz2, extract_tar_gz, extract_tar_xz,
    extract_zip,
};
use super::variables::{resolve_path, substitute_variables};
use super::PipelineHandle;
use crate::security::ssrf::check_url_not_private;
use crate::server_management::process::wait_for_output_pattern;
use crate::templates::update_check::extract_version;
use crate::types::*;
use crate::utils::json_path::json_navigate;
use crate::AppState;

// SSRF protection is provided by crate::ssrf::{is_private_ip, check_url_not_private}.

/// Max response body size for ResolveVariable API checks (2 MB).
const RESOLVE_RESPONSE_MAX_BYTES: usize = 2 * 1024 * 1024;
/// Hard timeout on outbound HTTP requests for ResolveVariable.
const RESOLVE_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[allow(clippy::too_many_arguments)]
pub async fn execute_resolve_variable(
    handle: &Arc<PipelineHandle>,
    state: &Arc<AppState>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    vars: &mut HashMap<String, String>,
    url: &str,
    path: &Option<String>,
    pick: VersionPick,
    value_key: &Option<String>,
    variable: &str,
) -> Result<(), String> {
    let resolved_url = substitute_variables(url, vars);

    // SSRF protection: block requests to private/internal IP addresses.
    check_url_not_private(&resolved_url)?;

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Fetching {}", resolved_url),
        LogStream::Stdout,
    );

    let resp = state
        .http_client
        .get(&resolved_url)
        .timeout(RESOLVE_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("HTTP request to {} failed: {}", resolved_url, e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {} from {}", resp.status(), resolved_url));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body from {}: {}", resolved_url, e))?;

    if bytes.len() > RESOLVE_RESPONSE_MAX_BYTES {
        return Err(format!(
            "Response from {} exceeds 2 MB limit ({} bytes)",
            resolved_url,
            bytes.len()
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Response from {} is not valid JSON: {}", resolved_url, e))?;

    let navigated = json_navigate(&json, path.as_deref()).ok_or_else(|| {
        format!(
            "Path '{}' did not resolve in the JSON response from {}",
            path.as_deref().unwrap_or(""),
            resolved_url
        )
    })?;

    let value = extract_version(navigated, pick, value_key.as_deref()).ok_or_else(|| {
        format!(
            "Could not extract a value from path '{}' in response from {}",
            path.as_deref().unwrap_or("(root)"),
            resolved_url
        )
    })?;

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Resolved ${{{}}}: {}", variable, value),
        LogStream::Stdout,
    );

    vars.insert(variable.to_string(), value);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_download(
    handle: &Arc<PipelineHandle>,
    state: &Arc<AppState>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    url: &str,
    destination: &str,
    filename: &Option<String>,
    executable: bool,
) -> Result<(), String> {
    let url = substitute_variables(url, vars);

    // SSRF protection: block requests to private/internal IP addresses.
    check_url_not_private(&url)?;

    let dest_dir = resolve_path(server_dir, destination, vars)?;

    std::fs::create_dir_all(&dest_dir).map_err(|e| {
        format!(
            "Failed to create destination directory {:?}: {}",
            dest_dir, e
        )
    })?;

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Downloading {}", url),
        LogStream::Stdout,
    );

    let response = state
        .http_client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Download failed for '{}': {}", url, e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download returned HTTP {}: {}",
            response.status().as_u16(),
            response.status().canonical_reason().unwrap_or("Unknown")
        ));
    }

    let fname = if let Some(ref name) = filename {
        substitute_variables(name, vars)
    } else {
        url.rsplit('/')
            .next()
            .and_then(|s| {
                let s = s.split('?').next().unwrap_or(s);
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            })
            .unwrap_or_else(|| "download".to_string())
    };

    let file_path = dest_dir.join(&fname);

    let content_length = response.content_length();
    if let Some(len) = content_length {
        handle.emit_log(
            phase,
            step_index,
            step_name,
            format!("File size: {} bytes", len),
            LogStream::Stdout,
        );
    }

    // Stream the response body to disk in chunks instead of loading the
    // entire file into memory.  This is critical for large downloads
    // (e.g. multi-GB game server binaries) that would otherwise cause
    // excessive memory usage or OOM kills.
    {
        use futures::StreamExt;
        use tokio::io::AsyncWriteExt;

        let mut file = tokio::fs::File::create(&file_path)
            .await
            .map_err(|e| format!("Failed to create file {:?}: {}", file_path, e))?;

        let mut stream = response.bytes_stream();
        let mut total_written: u64 = 0;
        let mut last_progress: u64 = 0;
        // Report progress every 10 MB so the user sees activity on large downloads.
        const PROGRESS_INTERVAL: u64 = 10 * 1024 * 1024;

        while let Some(chunk_result) = stream.next().await {
            let chunk =
                chunk_result.map_err(|e| format!("Failed to read download stream: {}", e))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Failed to write to {:?}: {}", file_path, e))?;
            total_written += chunk.len() as u64;

            if total_written - last_progress >= PROGRESS_INTERVAL {
                handle.emit_log(
                    phase,
                    step_index,
                    step_name,
                    format!("Downloaded {} bytes so far...", total_written),
                    LogStream::Stdout,
                );
                last_progress = total_written;
            }
        }

        file.flush()
            .await
            .map_err(|e| format!("Failed to flush {:?}: {}", file_path, e))?;

        handle.emit_log(
            phase,
            step_index,
            step_name,
            format!("Saved to {:?} ({} bytes)", file_path, total_written),
            LogStream::Stdout,
        );
    }

    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&file_path, perms).map_err(|e| {
            format!(
                "Failed to set executable permission on {:?}: {}",
                file_path, e
            )
        })?;
        handle.emit_log(
            phase,
            step_index,
            step_name,
            format!("Set {:?} as executable (755)", file_path),
            LogStream::Stdout,
        );
    }
    #[cfg(not(unix))]
    if executable {
        handle.emit_log(
            phase,
            step_index,
            step_name,
            "Warning: executable flag is only supported on Unix systems".to_string(),
            LogStream::Stderr,
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_extract(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    source: &str,
    destination: &Option<String>,
    format: &ArchiveFormat,
) -> Result<(), String> {
    let source_path = resolve_path(server_dir, source, vars)?;
    let dest_dir = match destination {
        Some(d) => resolve_path(server_dir, d, vars)?,
        None => server_dir.to_path_buf(),
    };

    std::fs::create_dir_all(&dest_dir).map_err(|e| {
        format!(
            "Failed to create extraction directory {:?}: {}",
            dest_dir, e
        )
    })?;

    let effective_format = match format {
        ArchiveFormat::Auto => {
            let name = source_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            detect_archive_format(name).ok_or_else(|| {
                format!(
                    "Cannot detect archive format from filename '{}'. Specify format explicitly.",
                    name
                )
            })?
        }
        other => other.clone(),
    };

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!(
            "Extracting {:?} to {:?} (format: {:?})",
            source_path, dest_dir, effective_format
        ),
        LogStream::Stdout,
    );

    let source_clone = source_path.clone();
    let dest_clone = dest_dir.clone();

    tokio::task::spawn_blocking(move || match effective_format {
        ArchiveFormat::Zip => extract_zip(&source_clone, &dest_clone),
        ArchiveFormat::TarGz => extract_tar_gz(&source_clone, &dest_clone),
        ArchiveFormat::TarBz2 => extract_tar_bz2(&source_clone, &dest_clone),
        ArchiveFormat::TarXz => extract_tar_xz(&source_clone, &dest_clone),
        ArchiveFormat::Tar => extract_tar(&source_clone, &dest_clone),
        ArchiveFormat::Auto => unreachable!("Auto format should have been resolved"),
    })
    .await
    .map_err(|e| format!("Extraction task panicked: {}", e))??;

    handle.emit_log(
        phase,
        step_index,
        step_name,
        "Extraction complete".to_string(),
        LogStream::Stdout,
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_move(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    source: &str,
    destination: &str,
) -> Result<(), String> {
    let src = resolve_path(server_dir, source, vars)?;
    let dst = resolve_path(server_dir, destination, vars)?;

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Moving {:?} -> {:?}", src, dst),
        LogStream::Stdout,
    );

    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent directory {:?}: {}", parent, e))?;
    }

    match std::fs::rename(&src, &dst) {
        Ok(()) => {}
        Err(_) => {
            if src.is_dir() {
                copy_dir_recursive(&src, &dst)?;
                std::fs::remove_dir_all(&src).map_err(|e| {
                    format!(
                        "Failed to remove source directory {:?} after copy: {}",
                        src, e
                    )
                })?;
            } else {
                std::fs::copy(&src, &dst)
                    .map_err(|e| format!("Failed to copy {:?} to {:?}: {}", src, dst, e))?;
                std::fs::remove_file(&src).map_err(|e| {
                    format!("Failed to remove source file {:?} after copy: {}", src, e)
                })?;
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_copy(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    source: &str,
    destination: &str,
    recursive: bool,
) -> Result<(), String> {
    let src = resolve_path(server_dir, source, vars)?;
    let dst = resolve_path(server_dir, destination, vars)?;

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Copying {:?} -> {:?}", src, dst),
        LogStream::Stdout,
    );

    if src.is_dir() {
        if !recursive {
            return Err(format!(
                "Source {:?} is a directory but recursive is false",
                src
            ));
        }
        copy_dir_recursive(&src, &dst)?;
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directory {:?}: {}", parent, e))?;
        }
        std::fs::copy(&src, &dst)
            .map_err(|e| format!("Failed to copy {:?} to {:?}: {}", src, dst, e))?;
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create directory {:?}: {}", dst, e))?;

    for entry in walkdir::WalkDir::new(src).min_depth(1) {
        let entry = entry.map_err(|e| format!("Failed to walk directory: {}", e))?;
        let relative = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| format!("Failed to compute relative path: {}", e))?;
        let target = dst.join(relative);

        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)
                .map_err(|e| format!("Failed to create directory {:?}: {}", target, e))?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent {:?}: {}", parent, e))?;
            }
            std::fs::copy(entry.path(), &target)
                .map_err(|e| format!("Failed to copy {:?} to {:?}: {}", entry.path(), target, e))?;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_delete(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    path: &str,
    recursive: bool,
) -> Result<(), String> {
    let target = resolve_path(server_dir, path, vars)?;

    if !target.exists() {
        handle.emit_log(
            phase,
            step_index,
            step_name,
            format!("Path {:?} does not exist, skipping delete", target),
            LogStream::Stdout,
        );
        return Ok(());
    }

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Deleting {:?} (recursive: {})", target, recursive),
        LogStream::Stdout,
    );

    if target.is_dir() {
        if recursive {
            std::fs::remove_dir_all(&target)
                .map_err(|e| format!("Failed to remove directory {:?}: {}", target, e))?;
        } else {
            std::fs::remove_dir(&target).map_err(|e| {
                format!(
                    "Failed to remove directory {:?} (not recursive, must be empty): {}",
                    target, e
                )
            })?;
        }
    } else {
        std::fs::remove_file(&target)
            .map_err(|e| format!("Failed to remove file {:?}: {}", target, e))?;
    }

    Ok(())
}

pub async fn execute_create_dir(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    path: &str,
) -> Result<(), String> {
    let target = resolve_path(server_dir, path, vars)?;

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Creating directory {:?}", target),
        LogStream::Stdout,
    );

    std::fs::create_dir_all(&target)
        .map_err(|e| format!("Failed to create directory {:?}: {}", target, e))?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_run_command(
    handle: &Arc<PipelineHandle>,
    state: &Arc<AppState>,
    server_id: Uuid,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    command: &str,
    args: &[String],
    working_dir: &Option<String>,
    env: &HashMap<String, String>,
) -> Result<(), String> {
    // Phase 1 — Check if RunCommand is allowed
    let settings = state
        .db
        .get_settings()
        .await
        .map_err(|e| format!("Failed to load settings: {}", e))?;

    if !settings.allow_run_commands {
        return Err(
            "RunCommand steps are disabled. An administrator must enable 'Allow pipeline commands' in Settings to use templates that execute shell commands.".to_string()
        );
    }

    let cmd = substitute_variables(command, vars);
    let substituted_args: Vec<String> =
        args.iter().map(|a| substitute_variables(a, vars)).collect();
    let work_dir = match working_dir {
        Some(wd) => resolve_path(server_dir, wd, vars)?,
        None => server_dir.to_path_buf(),
    };
    let substituted_env: HashMap<String, String> = env
        .iter()
        .map(|(k, v)| (k.clone(), substitute_variables(v, vars)))
        .collect();

    // Phase 1 — Audit logging (before execution)
    let server_name = vars.get("SERVER_NAME").cloned().unwrap_or_default();
    let start_time = std::time::Instant::now();

    tracing::info!(
        server_id = %server_id,
        server_name = %server_name,
        pipeline = ?phase,
        step_index = step_index,
        step_name = step_name,
        command = %cmd,
        args = ?substituted_args,
        working_dir = ?work_dir,
        "[AUDIT] RunCommand execution started"
    );

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Running: {} {}", cmd, substituted_args.join(" ")),
        LogStream::Stdout,
    );

    // Phase 2 — Get server config for isolation settings
    let server = state
        .db
        .get_server(server_id)
        .await
        .map_err(|e| format!("Failed to load server config: {}", e))?
        .ok_or_else(|| format!("Server {} not found", server_id))?;

    // Phase 2 — Determine sandboxing mode
    let sandbox_mode = &settings.run_command_sandbox;
    let should_sandbox = !matches!(sandbox_mode.as_str(), "off");

    // Phase 2 — Build sandbox configuration
    let sandbox = if should_sandbox {
        Some(crate::sandbox::PreExecSandbox::new(
            server_dir,
            &server.config.isolation,
        ))
    } else {
        None
    };

    let mut cmd_builder = Command::new(&cmd);
    cmd_builder
        .args(&substituted_args)
        .envs(&substituted_env)
        .current_dir(&work_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    // Phase 2 & 3 — Apply sandbox and namespaces in pre_exec hook
    #[cfg(unix)]
    if let Some(sandbox_ref) = sandbox {
        let sandbox_mode_for_hook = sandbox_mode.clone();
        let use_namespaces = settings.run_command_use_namespaces;
        unsafe {
            cmd_builder.pre_exec(move || {
                // Phase 3 — Apply namespace isolation if enabled
                #[cfg(target_os = "linux")]
                if use_namespaces {
                    if let Err(e) = crate::sandbox::namespaces::apply_namespaces() {
                        if sandbox_mode_for_hook == "strict" {
                            return Err(std::io::Error::other(format!(
                                "Namespace isolation failed in strict mode: {}",
                                e
                            )));
                        }
                        // In auto mode, log warning but proceed
                        let msg = format!("[anyserver] RunCommand namespace warning: {}\n", e);
                        libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
                    }
                }

                // Put the command in its own process group for clean termination
                libc::setsid();

                // Phase 2 — Apply sandbox
                if let Err(e) = sandbox_ref.apply() {
                    if sandbox_mode_for_hook == "strict" {
                        // In strict mode, fail if sandboxing fails
                        return Err(std::io::Error::other(format!(
                            "Sandboxing failed in strict mode: {}",
                            e
                        )));
                    }
                    // In auto mode, log warning but proceed
                    let msg = format!("[anyserver] RunCommand sandbox warning: {}\n", e);
                    libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
                }
                Ok(())
            });
        }
    }

    let mut child = cmd_builder
        .spawn()
        .map_err(|e| format!("Failed to spawn command '{}': {}", cmd, e))?;

    let stdout_handle_ref = Arc::clone(handle);
    let stdout_step_name = step_name.to_string();
    let stdout_task = child.stdout.take().map(|stdout| {
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                stdout_handle_ref.emit_log(
                    phase,
                    step_index,
                    &stdout_step_name,
                    line,
                    LogStream::Stdout,
                );
            }
        })
    });

    let stderr_handle_ref = Arc::clone(handle);
    let stderr_step_name = step_name.to_string();
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                stderr_handle_ref.emit_log(
                    phase,
                    step_index,
                    &stderr_step_name,
                    line,
                    LogStream::Stderr,
                );
            }
        })
    });

    // Phase 2 — Enforce timeout
    let timeout_secs = settings.run_command_default_timeout_secs;
    let timeout_duration = Duration::from_secs(timeout_secs as u64);

    let wait_result = tokio::time::timeout(timeout_duration, child.wait()).await;

    let status = match wait_result {
        Ok(result) => result.map_err(|e| format!("Failed to wait for command '{}': {}", cmd, e))?,
        Err(_) => {
            // Timeout occurred — kill the process
            let _ = child.kill().await;
            return Err(format!(
                "Command '{}' timed out after {} seconds",
                cmd, timeout_secs
            ));
        }
    };

    if let Some(t) = stdout_task {
        let _ = t.await;
    }
    if let Some(t) = stderr_task {
        let _ = t.await;
    }

    let duration_ms = start_time.elapsed().as_millis();
    let exit_code = status.code().unwrap_or(-1);

    // Phase 1 — Audit logging (after execution)
    tracing::info!(
        server_id = %server_id,
        server_name = %server_name,
        pipeline = ?phase,
        step_index = step_index,
        step_name = step_name,
        command = %cmd,
        exit_code = exit_code,
        duration_ms = duration_ms,
        success = status.success(),
        "[AUDIT] RunCommand execution completed"
    );

    if !status.success() {
        return Err(format!(
            "Command '{}' exited with code {}",
            cmd,
            status
                .code()
                .map_or("unknown".to_string(), |c| c.to_string())
        ));
    }

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Command '{}' completed successfully", cmd),
        LogStream::Stdout,
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_write_file(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    path: &str,
    content: &str,
) -> Result<(), String> {
    let file_path = resolve_path(server_dir, path, vars)?;
    let substituted_content = substitute_variables(content, vars);

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!(
            "Writing file {:?} ({} bytes)",
            file_path,
            substituted_content.len()
        ),
        LogStream::Stdout,
    );

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent directory {:?}: {}", parent, e))?;
    }

    tokio::fs::write(&file_path, substituted_content.as_bytes())
        .await
        .map_err(|e| format!("Failed to write file {:?}: {}", file_path, e))?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_set_permissions(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    path: &str,
    mode: &str,
) -> Result<(), String> {
    let file_path = resolve_path(server_dir, path, vars)?;

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Setting permissions on {:?} to {}", file_path, mode),
        LogStream::Stdout,
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode_u32 = u32::from_str_radix(mode, 8).map_err(|e| {
            format!(
                "Invalid permission mode '{}': {}. Expected octal like '755'.",
                mode, e
            )
        })?;
        let perms = std::fs::Permissions::from_mode(mode_u32);
        std::fs::set_permissions(&file_path, perms)
            .map_err(|e| format!("Failed to set permissions on {:?}: {}", file_path, e))?;
    }

    #[cfg(not(unix))]
    {
        let _ = (file_path, mode);
        handle.emit_log(
            phase,
            step_index,
            step_name,
            "Warning: SetPermissions is only supported on Unix systems".to_string(),
            LogStream::Stderr,
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_glob(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    pattern: &str,
    destination: &str,
) -> Result<(), String> {
    let substituted_pattern = substitute_variables(pattern, vars);
    let full_pattern = server_dir.join(&substituted_pattern);
    let dst = resolve_path(server_dir, destination, vars)?;

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Glob matching {:?} -> {:?}", full_pattern, dst),
        LogStream::Stdout,
    );

    let parent = full_pattern
        .parent()
        .ok_or_else(|| "Glob pattern has no parent directory".to_string())?;

    if !parent.exists() {
        return Err(format!("Glob parent directory {:?} does not exist", parent));
    }

    let pattern_name = full_pattern
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Glob pattern has no filename component".to_string())?;

    let mut matches: Vec<PathBuf> = Vec::new();
    let entries = std::fs::read_dir(parent)
        .map_err(|e| format!("Failed to read directory {:?}: {}", parent, e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read dir entry: {}", e))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if glob_match(pattern_name, &name_str) {
            matches.push(entry.path());
        }
    }

    if matches.is_empty() {
        return Err(format!(
            "No files matched glob pattern '{}'",
            substituted_pattern
        ));
    }

    if matches.len() == 1 {
        let src = &matches[0];
        handle.emit_log(
            phase,
            step_index,
            step_name,
            format!("Matched: {:?} -> {:?}", src, dst),
            LogStream::Stdout,
        );

        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directory {:?}: {}", parent, e))?;
        }

        std::fs::rename(src, &dst)
            .map_err(|e| format!("Failed to rename {:?} to {:?}: {}", src, dst, e))?;
    } else {
        std::fs::create_dir_all(&dst)
            .map_err(|e| format!("Failed to create destination directory {:?}: {}", dst, e))?;

        for src in &matches {
            let fname = src.file_name().ok_or_else(|| "No filename".to_string())?;
            let target = dst.join(fname);
            handle.emit_log(
                phase,
                step_index,
                step_name,
                format!("Matched: {:?} -> {:?}", src, target),
                LogStream::Stdout,
            );
            std::fs::rename(src, &target)
                .map_err(|e| format!("Failed to rename {:?} to {:?}: {}", src, target, e))?;
        }
    }

    Ok(())
}

pub fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match text[pos..].find(part) {
            Some(found) => {
                if i == 0 && found != 0 {
                    return false;
                }
                pos += found + part.len();
            }
            None => return false,
        }
    }

    if !pattern.ends_with('*') && pos != text.len() {
        return false;
    }

    true
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_edit_file(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    path: &str,
    operation: &FileOperation,
) -> Result<(), String> {
    let file_path = resolve_path(server_dir, path, vars)?;

    let existing = if file_path.exists() {
        tokio::fs::read_to_string(&file_path)
            .await
            .map_err(|e| format!("Failed to read file {:?}: {}", file_path, e))?
    } else {
        String::new()
    };

    let new_content = match operation {
        FileOperation::Overwrite { content } => {
            let c = substitute_variables(content, vars);
            handle.emit_log(
                phase,
                step_index,
                step_name,
                format!("Overwriting {:?} ({} bytes)", file_path, c.len()),
                LogStream::Stdout,
            );
            c
        }
        FileOperation::Append { content } => {
            let c = substitute_variables(content, vars);
            handle.emit_log(
                phase,
                step_index,
                step_name,
                format!("Appending {} bytes to {:?}", c.len(), file_path),
                LogStream::Stdout,
            );
            let mut result = existing;
            result.push_str(&c);
            result
        }
        FileOperation::Prepend { content } => {
            let c = substitute_variables(content, vars);
            handle.emit_log(
                phase,
                step_index,
                step_name,
                format!("Prepending {} bytes to {:?}", c.len(), file_path),
                LogStream::Stdout,
            );
            let mut result = c;
            result.push_str(&existing);
            result
        }
        FileOperation::FindReplace { find, replace, all } => {
            let f = substitute_variables(find, vars);
            let r = substitute_variables(replace, vars);
            let count = existing.matches(&f).count();
            handle.emit_log(
                phase,
                step_index,
                step_name,
                format!(
                    "Find-replace in {:?}: {:?} -> {:?} ({} match{}, replace {})",
                    file_path,
                    f,
                    r,
                    count,
                    if count == 1 { "" } else { "es" },
                    if *all { "all" } else { "first" },
                ),
                LogStream::Stdout,
            );
            if *all {
                existing.replace(&f, &r)
            } else {
                existing.replacen(&f, &r, 1)
            }
        }
        FileOperation::RegexReplace {
            pattern,
            replace,
            all,
        } => {
            let pat = substitute_variables(pattern, vars);
            let rep = substitute_variables(replace, vars);
            let re = regex::Regex::new(&pat)
                .map_err(|e| format!("Invalid regex pattern '{}': {}", pat, e))?;
            handle.emit_log(
                phase,
                step_index,
                step_name,
                format!(
                    "Regex-replace in {:?}: /{:?}/ -> {:?} (replace {})",
                    file_path,
                    pat,
                    rep,
                    if *all { "all" } else { "first" },
                ),
                LogStream::Stdout,
            );
            if *all {
                re.replace_all(&existing, rep.as_str()).into_owned()
            } else {
                re.replace(&existing, rep.as_str()).into_owned()
            }
        }
        FileOperation::InsertAfter { pattern, content } => {
            let pat = substitute_variables(pattern, vars);
            let c = substitute_variables(content, vars);
            handle.emit_log(
                phase,
                step_index,
                step_name,
                format!("Insert after line containing {:?} in {:?}", pat, file_path),
                LogStream::Stdout,
            );
            let lines: Vec<&str> = existing.lines().collect();
            let mut inserted = false;
            let mut result_lines: Vec<String> = Vec::with_capacity(lines.len() + 1);
            for line in &lines {
                result_lines.push(line.to_string());
                if !inserted && line.contains(&pat) {
                    result_lines.push(c.clone());
                    inserted = true;
                }
            }
            if !inserted {
                return Err(format!(
                    "Pattern {:?} not found in any line of {:?}",
                    pat, file_path
                ));
            }
            let mut result = result_lines.join("\n");
            if existing.ends_with('\n') {
                result.push('\n');
            }
            result
        }
        FileOperation::InsertBefore { pattern, content } => {
            let pat = substitute_variables(pattern, vars);
            let c = substitute_variables(content, vars);
            handle.emit_log(
                phase,
                step_index,
                step_name,
                format!("Insert before line containing {:?} in {:?}", pat, file_path),
                LogStream::Stdout,
            );
            let lines: Vec<&str> = existing.lines().collect();
            let mut inserted = false;
            let mut result_lines: Vec<String> = Vec::with_capacity(lines.len() + 1);
            for line in &lines {
                if !inserted && line.contains(&pat) {
                    result_lines.push(c.clone());
                    inserted = true;
                }
                result_lines.push(line.to_string());
            }
            if !inserted {
                return Err(format!(
                    "Pattern {:?} not found in any line of {:?}",
                    pat, file_path
                ));
            }
            let mut result = result_lines.join("\n");
            if existing.ends_with('\n') {
                result.push('\n');
            }
            result
        }
        FileOperation::ReplaceLine {
            pattern,
            content,
            all,
        } => {
            let pat = substitute_variables(pattern, vars);
            let c = substitute_variables(content, vars);
            handle.emit_log(
                phase,
                step_index,
                step_name,
                format!(
                    "Replace line(s) containing {:?} in {:?} (replace {})",
                    pat,
                    file_path,
                    if *all { "all" } else { "first" },
                ),
                LogStream::Stdout,
            );
            let lines: Vec<&str> = existing.lines().collect();
            let mut replaced = false;
            let mut result_lines: Vec<String> = Vec::with_capacity(lines.len());
            for line in &lines {
                if line.contains(&pat) && (*all || !replaced) {
                    result_lines.push(c.clone());
                    replaced = true;
                } else {
                    result_lines.push(line.to_string());
                }
            }
            if !replaced {
                return Err(format!(
                    "Pattern {:?} not found in any line of {:?}",
                    pat, file_path
                ));
            }
            let mut result = result_lines.join("\n");
            if existing.ends_with('\n') {
                result.push('\n');
            }
            result
        }
    };

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent directory {:?}: {}", parent, e))?;
    }

    tokio::fs::write(&file_path, new_content.as_bytes())
        .await
        .map_err(|e| format!("Failed to write file {:?}: {}", file_path, e))?;

    Ok(())
}

pub async fn execute_set_env(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    vars: &HashMap<String, String>,
    variables: &HashMap<String, String>,
) -> Result<(), String> {
    let substituted: HashMap<String, String> = variables
        .iter()
        .map(|(k, v)| (k.clone(), substitute_variables(v, vars)))
        .collect();

    {
        let mut config = handle.process_config.lock();
        for (k, v) in &substituted {
            config.env.insert(k.clone(), v.clone());
        }
    }

    for (k, v) in &substituted {
        handle.emit_log(
            phase,
            step_index,
            step_name,
            format!("Set env {}={}", k, v),
            LogStream::Stdout,
        );
    }

    Ok(())
}

pub async fn execute_set_working_dir(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    vars: &HashMap<String, String>,
    path: &str,
) -> Result<(), String> {
    let resolved = substitute_variables(path, vars);

    {
        let mut config = handle.process_config.lock();
        config.working_dir = Some(resolved.clone());
    }

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Set working directory: {}", resolved),
        LogStream::Stdout,
    );

    Ok(())
}

pub async fn execute_set_stop_command(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    vars: &HashMap<String, String>,
    command: &str,
) -> Result<(), String> {
    let resolved = substitute_variables(command, vars);

    {
        let mut config = handle.process_config.lock();
        config.stop_command = Some(resolved.clone());
    }

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Set stop command: {}", resolved),
        LogStream::Stdout,
    );

    Ok(())
}

pub async fn execute_set_stop_signal(
    handle: &Arc<PipelineHandle>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    signal: &crate::types::StopSignal,
) -> Result<(), String> {
    {
        let mut config = handle.process_config.lock();
        config.stop_signal = Some(*signal);
    }

    let label = match signal {
        crate::types::StopSignal::Sigterm => "SIGTERM",
        crate::types::StopSignal::Sigint => "SIGINT (Ctrl+C)",
    };

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Set stop signal: {}", label),
        LogStream::Stdout,
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_wait_for_output(
    handle: &Arc<PipelineHandle>,
    state: &Arc<AppState>,
    server_id: Uuid,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    vars: &HashMap<String, String>,
    pattern: &str,
    timeout_secs: u32,
) -> Result<(), String> {
    let resolved = substitute_variables(pattern, vars);

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!(
            "Waiting for \"{}\" in console output (timeout {}s)...",
            resolved, timeout_secs
        ),
        LogStream::Stdout,
    );

    // Subscribe BEFORE reading the buffer to avoid a race where the line
    // appears between the buffer check and the subscription.
    let rx = match state.process_manager.subscribe(&server_id) {
        Some(rx) => rx,
        None => {
            // No process handle exists — the server has never been started.
            // We can still check the buffer (which will be empty), but there's
            // nothing to subscribe to.  Fall through with a warning.
            handle.emit_log(
                phase,
                step_index,
                step_name,
                "No active process found — cannot subscribe to log output. \
                 Will check existing buffer only."
                    .to_string(),
                LogStream::Stderr,
            );
            let buffer = state.process_manager.get_log_buffer(&server_id);
            let found = wait_for_output_pattern(
                // Create a dummy channel so the helper can still run its timeout
                // logic (it will just time out immediately if nothing is in the
                // buffer).
                {
                    let (tx, rx) = tokio::sync::broadcast::channel::<WsMessage>(1);
                    drop(tx); // close immediately — helper will see Closed
                    rx
                },
                &buffer,
                &resolved,
                timeout_secs,
            )
            .await;
            if found {
                handle.emit_log(
                    phase,
                    step_index,
                    step_name,
                    format!("Pattern \"{}\" found in existing log buffer.", resolved),
                    LogStream::Stdout,
                );
                return Ok(());
            } else {
                return Err(format!(
                    "Timed out waiting for pattern \"{}\" after {}s (no active process)",
                    resolved, timeout_secs
                ));
            }
        }
    };

    let buffer = state.process_manager.get_log_buffer(&server_id);
    let found = wait_for_output_pattern(rx, &buffer, &resolved, timeout_secs).await;

    if found {
        handle.emit_log(
            phase,
            step_index,
            step_name,
            format!("Pattern \"{}\" found in console output.", resolved),
            LogStream::Stdout,
        );
        Ok(())
    } else {
        Err(format!(
            "Timed out waiting for pattern \"{}\" after {}s",
            resolved, timeout_secs
        ))
    }
}

/// Execute a DownloadGithubReleaseAsset step.
///
/// This resolves the release tag from a parameter, fetches the release details,
/// finds a matching asset, and downloads it.
#[allow(clippy::too_many_arguments)]
pub async fn execute_download_github_release_asset(
    handle: &Arc<PipelineHandle>,
    state: &Arc<AppState>,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    config: &crate::types::ServerConfig,
    tag_param: &str,
    asset_matcher: &str,
    destination: &str,
    filename: &Option<String>,
    executable: bool,
) -> Result<(), String> {
    // 1. Validate that tag_param references a valid github_release_tag parameter
    let param = config
        .parameters
        .iter()
        .find(|p| p.name == tag_param)
        .ok_or_else(|| {
            format!(
                "Parameter '{}' not found in template configuration",
                tag_param
            )
        })?;

    if !matches!(param.param_type, ConfigParameterType::GithubReleaseTag) {
        return Err(format!(
            "Parameter '{}' is not a github_release_tag type (found {:?})",
            tag_param, param.param_type
        ));
    }

    let github_repo = param.github_repo.as_ref().ok_or_else(|| {
        format!(
            "Parameter '{}' is missing required github_repo field",
            tag_param
        )
    })?;

    // 2. Get the tag value from vars
    let tag = vars
        .get(tag_param)
        .ok_or_else(|| format!("No value provided for parameter '{}'", tag_param))?;

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!(
            "Fetching release '{}' from GitHub repo '{}'",
            tag, github_repo
        ),
        LogStream::Stdout,
    );

    // 3. Get GitHub token if configured
    let github_settings = state
        .db
        .get_github_settings()
        .await
        .map_err(|e| format!("Failed to get GitHub settings: {}", e))?;

    let token = github_settings.and_then(|s| s.api_token);

    // 4. Fetch the release and its assets
    let release = crate::integrations::github::fetch_release_by_tag(
        &state.http_client,
        github_repo,
        tag,
        token.as_deref(),
    )
    .await
    .map_err(|e| format!("Failed to fetch GitHub release: {}", e))?;

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("Found release with {} asset(s)", release.assets.len()),
        LogStream::Stdout,
    );

    // 5. Find the matching asset
    let asset = crate::integrations::github::find_asset_by_matcher(&release.assets, asset_matcher)
        .map_err(|e| format!("Failed to find matching asset: {}", e))?;

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!(
            "Found matching asset: {} ({} bytes)",
            asset.name, asset.size
        ),
        LogStream::Stdout,
    );

    // 6. Download the asset using the existing download logic
    let dest_dir = resolve_path(server_dir, destination, vars)?;
    std::fs::create_dir_all(&dest_dir).map_err(|e| {
        format!(
            "Failed to create destination directory {:?}: {}",
            dest_dir, e
        )
    })?;

    let fname = if let Some(ref name) = filename {
        substitute_variables(name, vars)
    } else {
        asset.name.clone()
    };

    let file_path = dest_dir.join(&fname);

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!(
            "Downloading {} to {:?}",
            asset.browser_download_url, file_path
        ),
        LogStream::Stdout,
    );

    // Stream download
    let response = state
        .http_client
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download returned HTTP {}: {}",
            response.status().as_u16(),
            response.status().canonical_reason().unwrap_or("Unknown")
        ));
    }

    {
        use futures::StreamExt;
        use tokio::io::AsyncWriteExt;

        let mut file = tokio::fs::File::create(&file_path)
            .await
            .map_err(|e| format!("Failed to create file {:?}: {}", file_path, e))?;

        let mut stream = response.bytes_stream();
        let mut total_written: u64 = 0;
        let mut last_progress: u64 = 0;
        const PROGRESS_INTERVAL: u64 = 10 * 1024 * 1024;

        while let Some(chunk_result) = stream.next().await {
            let chunk =
                chunk_result.map_err(|e| format!("Failed to read download stream: {}", e))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Failed to write to {:?}: {}", file_path, e))?;
            total_written += chunk.len() as u64;

            if total_written - last_progress >= PROGRESS_INTERVAL {
                handle.emit_log(
                    phase,
                    step_index,
                    step_name,
                    format!("Downloaded {} bytes so far...", total_written),
                    LogStream::Stdout,
                );
                last_progress = total_written;
            }
        }

        file.flush()
            .await
            .map_err(|e| format!("Failed to flush {:?}: {}", file_path, e))?;

        handle.emit_log(
            phase,
            step_index,
            step_name,
            format!("Saved to {:?} ({} bytes)", file_path, total_written),
            LogStream::Stdout,
        );
    }

    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&file_path, perms).map_err(|e| {
            format!(
                "Failed to set executable permission on {:?}: {}",
                file_path, e
            )
        })?;
        handle.emit_log(
            phase,
            step_index,
            step_name,
            format!("Set {:?} as executable (755)", file_path),
            LogStream::Stdout,
        );
    }
    #[cfg(not(unix))]
    if executable {
        handle.emit_log(
            phase,
            step_index,
            step_name,
            "Warning: executable flag is only supported on Unix systems".to_string(),
            LogStream::Stderr,
        );
    }

    Ok(())
}

/// Execute a SteamCMD install or update step.
///
/// This function:
/// 1. Checks that `steamcmd` is available on PATH (errors clearly if not)
/// 2. Resolves the app ID from the step override or the server's `steam_app_id`
/// 3. Runs `steamcmd +force_install_dir <server_dir> +login anonymous +app_update <app_id> validate +quit`
/// 4. Streams stdout/stderr to the pipeline log
#[allow(clippy::too_many_arguments)]
pub async fn execute_steamcmd(
    handle: &Arc<PipelineHandle>,
    state: &Arc<AppState>,
    server_id: Uuid,
    phase: PhaseKind,
    step_index: u32,
    step_name: &str,
    server_dir: &Path,
    vars: &HashMap<String, String>,
    step_app_id: Option<u32>,
    anonymous: bool,
    extra_args: &[String],
) -> Result<(), String> {
    // 1. Find steamcmd binary
    let steamcmd_bin = crate::utils::steamcmd::steamcmd_path().map_err(|msg| {
        format!(
            "SteamCMD is required but not available on this host.\n\
             \n\
             Error: {}\n\
             \n\
             To install SteamCMD:\n\
             • Debian/Ubuntu: sudo apt install steamcmd\n\
             • Arch Linux: yay -S steamcmd (AUR)\n\
             • NixOS: nix-env -iA nixpkgs.steamcmd (or add to shell.nix / nix-shell)\n\
             • Docker: add 'steamcmd' to your Dockerfile\n\
             • Manual: https://developer.valvesoftware.com/wiki/SteamCMD\n\
             \n\
             After installing, ensure 'steamcmd' is on PATH and restart AnyServer.",
            msg
        )
    })?;

    // 2. Resolve app ID: step override > server config > error
    let app_id = if let Some(id) = step_app_id {
        id
    } else {
        let server = state
            .db
            .get_server(server_id)
            .await
            .map_err(|e| format!("Failed to load server config: {}", e))?
            .ok_or_else(|| format!("Server {} not found", server_id))?;

        server.config.steam_app_id.ok_or_else(|| {
            "No Steam app ID specified. Set steam_app_id in the server config \
             or provide an app_id override in the SteamCMD step."
                .to_string()
        })?
    };

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!(
            "Running SteamCMD: app_update {} (anonymous={}, steamcmd={})",
            app_id, anonymous, steamcmd_bin
        ),
        LogStream::Stdout,
    );

    // 3. Build the command
    //    steamcmd +force_install_dir <dir> +login anonymous +app_update <id> validate [extra_args...] +quit
    assert!(
        server_dir.is_absolute(),
        "BUG: server_dir must be an absolute path, got: {}",
        server_dir.display()
    );
    let server_dir_str = server_dir.to_string_lossy().to_string();

    let mut args: Vec<String> = vec!["+force_install_dir".into(), server_dir_str, "+login".into()];

    if anonymous {
        args.push("anonymous".into());
    } else {
        // Non-anonymous login is not yet supported; would need credentials.
        return Err("Non-anonymous SteamCMD login is not yet supported. \
             Set anonymous=true or omit the field."
            .to_string());
    }

    args.push("+app_update".into());
    args.push(app_id.to_string());
    args.push("validate".into());

    // Append any extra args (e.g. "-beta experimental")
    for arg in extra_args {
        let substituted = substitute_variables(arg, vars);
        let safe_arg = super::variables::sanitize_steamcmd_arg(&substituted)
            .map_err(|e| format!("Unsafe SteamCMD extra arg: {}", e))?;
        args.push(safe_arg);
    }

    args.push("+quit".into());

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("$ {} {}", steamcmd_bin, args.join(" ")),
        LogStream::Stdout,
    );

    // 4. Spawn the process
    let mut cmd_builder = Command::new(&steamcmd_bin);
    cmd_builder
        .args(&args)
        .current_dir(server_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd_builder
        .spawn()
        .map_err(|e| format!("Failed to spawn steamcmd: {}", e))?;

    // Stream stdout
    let stdout_handle_ref = Arc::clone(handle);
    let stdout_step_name = step_name.to_string();
    let stdout_task = child.stdout.take().map(|stdout| {
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                stdout_handle_ref.emit_log(
                    phase,
                    step_index,
                    &stdout_step_name,
                    line,
                    LogStream::Stdout,
                );
            }
        })
    });

    // Stream stderr
    let stderr_handle_ref = Arc::clone(handle);
    let stderr_step_name = step_name.to_string();
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                stderr_handle_ref.emit_log(
                    phase,
                    step_index,
                    &stderr_step_name,
                    line,
                    LogStream::Stderr,
                );
            }
        })
    });

    // SteamCMD can be slow (downloading large game servers), use a generous timeout
    let timeout_duration = Duration::from_secs(30 * 60); // 30 minutes
    let wait_result = tokio::time::timeout(timeout_duration, child.wait()).await;

    let status = match wait_result {
        Ok(result) => result.map_err(|e| format!("Failed to wait for steamcmd: {}", e))?,
        Err(_) => {
            let _ = child.kill().await;
            return Err("SteamCMD timed out after 30 minutes".to_string());
        }
    };

    if let Some(t) = stdout_task {
        let _ = t.await;
    }
    if let Some(t) = stderr_task {
        let _ = t.await;
    }

    if !status.success() {
        return Err(format!(
            "SteamCMD exited with code {}",
            status
                .code()
                .map_or("unknown".to_string(), |c| c.to_string())
        ));
    }

    handle.emit_log(
        phase,
        step_index,
        step_name,
        format!("SteamCMD app_update {} completed successfully", app_id),
        LogStream::Stdout,
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_step(
    handle: &Arc<PipelineHandle>,
    state: &Arc<AppState>,
    server_id: Uuid,
    phase: PhaseKind,
    step_index: u32,
    step: &PipelineStep,
    server_dir: &Path,
    vars: &mut HashMap<String, String>,
) -> Result<(), String> {
    match &step.action {
        StepAction::Download {
            url,
            destination,
            filename,
            executable,
        } => {
            execute_download(
                handle,
                state,
                phase,
                step_index,
                &step.name,
                server_dir,
                vars,
                url,
                destination,
                filename,
                *executable,
            )
            .await
        }
        StepAction::Extract {
            source,
            destination,
            format,
        } => {
            execute_extract(
                handle,
                phase,
                step_index,
                &step.name,
                server_dir,
                vars,
                source,
                destination,
                format,
            )
            .await
        }
        StepAction::MoveAction {
            source,
            destination,
        } => {
            execute_move(
                handle,
                phase,
                step_index,
                &step.name,
                server_dir,
                vars,
                source,
                destination,
            )
            .await
        }
        StepAction::Copy {
            source,
            destination,
            recursive,
        } => {
            execute_copy(
                handle,
                phase,
                step_index,
                &step.name,
                server_dir,
                vars,
                source,
                destination,
                *recursive,
            )
            .await
        }
        StepAction::Delete { path, recursive } => {
            execute_delete(
                handle, phase, step_index, &step.name, server_dir, vars, path, *recursive,
            )
            .await
        }
        StepAction::CreateDir { path } => {
            execute_create_dir(
                handle, phase, step_index, &step.name, server_dir, vars, path,
            )
            .await
        }
        StepAction::RunCommand {
            command,
            args,
            working_dir,
            env,
        } => {
            execute_run_command(
                handle,
                state,
                server_id,
                phase,
                step_index,
                &step.name,
                server_dir,
                vars,
                command,
                args,
                working_dir,
                env,
            )
            .await
        }
        StepAction::WriteFile { path, content } => {
            execute_write_file(
                handle, phase, step_index, &step.name, server_dir, vars, path, content,
            )
            .await
        }
        StepAction::EditFile { path, operation } => {
            execute_edit_file(
                handle, phase, step_index, &step.name, server_dir, vars, path, operation,
            )
            .await
        }
        StepAction::SetPermissions { path, mode } => {
            execute_set_permissions(
                handle, phase, step_index, &step.name, server_dir, vars, path, mode,
            )
            .await
        }
        StepAction::Glob {
            pattern,
            destination,
        } => {
            execute_glob(
                handle,
                phase,
                step_index,
                &step.name,
                server_dir,
                vars,
                pattern,
                destination,
            )
            .await
        }
        StepAction::SetEnv { variables } => {
            execute_set_env(handle, phase, step_index, &step.name, vars, variables).await
        }
        StepAction::SetWorkingDir { path } => {
            execute_set_working_dir(handle, phase, step_index, &step.name, vars, path).await
        }
        StepAction::SetStopCommand { command } => {
            execute_set_stop_command(handle, phase, step_index, &step.name, vars, command).await
        }
        StepAction::SetStopSignal { signal } => {
            execute_set_stop_signal(handle, phase, step_index, &step.name, signal).await
        }
        StepAction::SendInput { text } => {
            // SendInput is only meaningful in stop_steps (executed by stop_server).
            // In regular pipelines, just log and skip.
            let resolved = substitute_variables(text, vars);
            handle.emit_log(
                phase,
                step_index,
                &step.name,
                format!("SendInput (no-op outside stop pipeline): {}", resolved),
                LogStream::Stdout,
            );
            Ok(())
        }
        StepAction::ResolveVariable {
            url,
            path,
            pick,
            value_key,
            variable,
        } => {
            execute_resolve_variable(
                handle, state, phase, step_index, &step.name, vars, url, path, *pick, value_key,
                variable,
            )
            .await
        }
        StepAction::SendSignal { signal } => {
            // SendSignal is only meaningful in stop_steps (executed by stop_server).
            // In regular pipelines, just log and skip.
            let label = match signal {
                crate::types::StopSignal::Sigterm => "SIGTERM",
                crate::types::StopSignal::Sigint => "SIGINT (Ctrl+C)",
            };
            handle.emit_log(
                phase,
                step_index,
                &step.name,
                format!("SendSignal (no-op outside stop pipeline): {}", label),
                LogStream::Stdout,
            );
            Ok(())
        }
        StepAction::Sleep { seconds } => {
            handle.emit_log(
                phase,
                step_index,
                &step.name,
                format!("Sleeping for {}s...", seconds),
                LogStream::Stdout,
            );
            tokio::time::sleep(std::time::Duration::from_secs(*seconds as u64)).await;
            handle.emit_log(
                phase,
                step_index,
                &step.name,
                format!("Sleep complete ({}s)", seconds),
                LogStream::Stdout,
            );
            Ok(())
        }
        StepAction::WaitForOutput {
            pattern,
            timeout_secs,
        } => {
            execute_wait_for_output(
                handle,
                state,
                server_id,
                phase,
                step_index,
                &step.name,
                vars,
                pattern,
                *timeout_secs,
            )
            .await
        }
        StepAction::DownloadGithubReleaseAsset {
            tag_param,
            asset_matcher,
            destination,
            filename,
            executable,
        } => {
            // Need to get the server config to validate the parameter reference
            let server = state
                .db
                .get_server(server_id)
                .await
                .map_err(|e| format!("Failed to get server config: {}", e))?
                .ok_or_else(|| format!("Server {} not found", server_id))?;

            execute_download_github_release_asset(
                handle,
                state,
                phase,
                step_index,
                &step.name,
                server_dir,
                vars,
                &server.config,
                tag_param,
                asset_matcher,
                destination,
                filename,
                *executable,
            )
            .await
        }
        StepAction::SteamCmdInstall {
            app_id,
            anonymous,
            extra_args,
        }
        | StepAction::SteamCmdUpdate {
            app_id,
            anonymous,
            extra_args,
        } => {
            execute_steamcmd(
                handle, state, server_id, phase, step_index, &step.name, server_dir, vars, *app_id,
                *anonymous, extra_args,
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match_basic() {
        assert!(glob_match("foo", "foo"));
        assert!(!glob_match("foo", "bar"));
    }

    #[test]
    fn test_glob_match_star() {
        assert!(glob_match("*.jar", "server.jar"));
        assert!(glob_match("server-*", "server-1.20.4"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("server-*.jar", "server-1.20.4.jar"));
        assert!(!glob_match("*.jar", "server.zip"));
    }

    #[test]
    fn test_glob_match_prefix_suffix() {
        assert!(glob_match("paper-*", "paper-1.20.4-497.jar"));
        assert!(!glob_match("paper-*", "spigot-1.20.4.jar"));
        assert!(glob_match("*-497.jar", "paper-1.20.4-497.jar"));
    }
}
