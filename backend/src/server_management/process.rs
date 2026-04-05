use std::collections::VecDeque;
use std::future::Future;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use chrono::Utc;
use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::pipeline;
use crate::types::{LogLine, LogStream, PhaseKind, ServerRuntime, ServerStatus, WsMessage};
use crate::AppState;

// ─── Robust script/binary execution resolution ──────────────────────────
//
// When a user points `binary` at a shell script, `Command::new(path)` calls
// `execve()` directly.  Unlike a login shell, `execve()` has no fallback
// logic — it needs either a valid ELF binary or a script whose shebang
// points to an existing interpreter.  In practice, scripts downloaded from
// the internet (game server modpacks, etc.) regularly have:
//
//   • Windows CRLF line endings  →  shebang becomes "#!/bin/sh\r"
//   • No shebang at all          →  ENOEXEC from the kernel
//   • A shebang referencing an interpreter at a path that doesn't exist
//     on the host (e.g. #!/bin/bash on a system where bash lives elsewhere)
//
// `resolve_execution` inspects the file, fixes what it can in-place, and
// returns the (command, args) pair that will actually work — mirroring what
// a POSIX shell does under the hood.

/// Maximum file size (in bytes) for automatic CRLF repair.  Scripts larger
/// than this are left untouched and the user gets a warning instead.
const MAX_SCRIPT_SIZE: u64 = 1024 * 1024; // 1 MB

/// Read at most `max_bytes` from the beginning of a file.
///
/// This avoids reading entire large binaries (50–200+ MB game servers) into
/// memory just to check the first few bytes for ELF magic or a shebang.
fn read_file_header(path: &std::path::Path, max_bytes: usize) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let mut buffer = vec![0u8; max_bytes];
    let n = file.read(&mut buffer)?;
    buffer.truncate(n);
    Ok(buffer)
}

/// The resolved command and arguments to use for spawning.
struct ResolvedExec {
    command: String,
    args: Vec<String>,
}

impl ResolvedExec {
    /// Pass through the original binary and args unchanged.
    fn passthrough(binary: &str, args: &[String]) -> Self {
        Self {
            command: binary.to_string(),
            args: args.to_vec(),
        }
    }
}

/// Try to locate an executable by name, searching common paths and then
/// falling back to `which`.
fn find_on_system(name: &str) -> Option<String> {
    // Fast path — common well-known locations.
    for prefix in &[
        "/usr/bin",
        "/bin",
        "/usr/local/bin",
        "/run/current-system/sw/bin", // NixOS
        "/usr/lib",
    ] {
        let candidate = format!("{}/{}", prefix, name);
        if std::path::Path::new(&candidate).exists() {
            return Some(candidate);
        }
    }

    // Slow path — ask the OS via `which`.
    if let Ok(output) = std::process::Command::new("which").arg(name).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() && std::path::Path::new(&path).exists() {
                return Some(path);
            }
        }
    }

    None
}

/// Inspect a binary / script file and determine how to execute it.
///
/// This handles every common failure mode:
///
/// 1. **CRLF line endings** — stripped in-place so the shebang is valid.
/// 2. **Valid shebang** (`#!/bin/sh` where `/bin/sh` exists) — executed
///    directly via the kernel.
/// 3. **Shebang with missing interpreter** (`#!/bin/bash` but bash is
///    elsewhere) — the interpreter is located by name and invoked
///    explicitly.
/// 4. **No shebang** (plain text script) — run via `sh`, just like a
///    POSIX shell would.
/// 5. **ELF binary** — executed directly, no processing needed.
/// 6. **File doesn't exist** — returned as-is so `spawn()` produces a
///    clear "not found" error with our diagnostics.
fn resolve_execution(binary: &str, args: &[String], handle: &ProcessHandle) -> ResolvedExec {
    let path = std::path::Path::new(binary);

    // ── File doesn't exist — let spawn() fail with a clear error ──
    if !path.exists() {
        return ResolvedExec::passthrough(binary, args);
    }

    // ── Read only the file header (512 bytes) — enough for ELF magic,
    //    shebang line, and CRLF detection without pulling a 200 MB game
    //    server binary into memory. ──
    let header = match read_file_header(path, 512) {
        Ok(h) => h,
        Err(_) => {
            return ResolvedExec::passthrough(binary, args);
        }
    };

    // ── ELF binary — execute directly, nothing to fix ──
    if header.starts_with(b"\x7fELF") {
        return ResolvedExec::passthrough(binary, args);
    }

    // ── Everything below is a script (text file) ──

    // Check whether the file has the executable bit set.  If not, we
    // cannot rely on the kernel to honour the shebang — we'll need to
    // invoke the interpreter explicitly (or fall back to `sh`).
    let is_executable = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(path)
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        }
        #[cfg(not(unix))]
        {
            true // On non-Unix we can't check — assume executable.
        }
    };

    if !is_executable {
        handle.push_log(LogLine {
            seq: 0,
            timestamp: Utc::now(),
            line: format!(
                "Script '{}' is not executable — running via interpreter",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            stream: LogStream::Stdout,
        });
    }

    // Step 1: strip CRLF in-place if present.
    // We only read the full file for scripts (not ELF), and only when
    // the header indicates CRLF is present.
    if header.contains(&b'\r') {
        // Check file size before reading the whole thing.
        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        if file_size > MAX_SCRIPT_SIZE {
            handle.push_log(LogLine {
                seq: 0,
                timestamp: Utc::now(),
                line: format!(
                    "Warning: '{}' has Windows line endings but is too large ({} bytes) \
                     for automatic CRLF fix. Please convert it manually.",
                    path.display(),
                    file_size,
                ),
                stream: LogStream::Stderr,
            });
            // Fall through without fixing — parse shebang from the header as-is.
            return resolve_script(&header, binary, args, is_executable, handle);
        }

        // Script is small enough — read the full file and fix CRLF.
        let content = match std::fs::read(path) {
            Ok(c) => c,
            Err(_) => {
                return ResolvedExec::passthrough(binary, args);
            }
        };

        let fixed: Vec<u8> = content.iter().copied().filter(|&b| b != b'\r').collect();
        if std::fs::write(path, &fixed).is_ok() {
            handle.push_log(LogLine {
                seq: 0,
                timestamp: Utc::now(),
                line: format!(
                    "Fixed Windows line endings (CRLF → LF) in '{}'",
                    path.display()
                ),
                stream: LogStream::Stdout,
            });
        }
        // Re-read the first line from the fixed content for shebang parsing.
        return resolve_script(&fixed, binary, args, is_executable, handle);
    }

    // No CRLF — the header is sufficient for shebang parsing.
    resolve_script(&header, binary, args, is_executable, handle)
}

/// Given the (possibly CRLF-fixed) content of a script, determine the
/// right (command, args) to use.
///
/// `is_executable` indicates whether the file has the Unix execute bit.
/// When `false` we cannot rely on the kernel to process the shebang, so
/// we invoke the interpreter explicitly.
fn resolve_script(
    content: &[u8],
    binary: &str,
    args: &[String],
    is_executable: bool,
    handle: &ProcessHandle,
) -> ResolvedExec {
    // Find the first line.
    let first_newline = content
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(content.len());
    let first_line = String::from_utf8_lossy(&content[..first_newline]);

    // ── No shebang — run via sh, just like a POSIX shell would ──
    if !first_line.starts_with("#!") {
        handle.push_log(LogLine {
            seq: 0,
            timestamp: Utc::now(),
            line: format!(
                "Script '{}' has no #! shebang — running via sh",
                std::path::Path::new(binary)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
            stream: LogStream::Stdout,
        });
        return run_via_sh(binary, args);
    }

    // ── Parse the shebang ──
    let shebang_body = first_line[2..].trim();
    let parts: Vec<&str> = shebang_body.split_whitespace().collect();
    let interpreter = match parts.first() {
        Some(s) => *s,
        None => {
            // Empty shebang `#!` — fall back to sh.
            handle.push_log(LogLine {
                seq: 0,
                timestamp: Utc::now(),
                line: "Script has an empty #! shebang — running via sh".to_string(),
                stream: LogStream::Stdout,
            });
            return run_via_sh(binary, args);
        }
    };

    // Helper: invoke an interpreter explicitly, bypassing the kernel shebang
    // mechanism.  Produces: <interpreter> [shebang_extra_args...] <script> [user_args...]
    let invoke_explicitly = |interp: String| -> ResolvedExec {
        let mut new_args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
        new_args.push(binary.to_string());
        new_args.extend(args.iter().cloned());
        ResolvedExec {
            command: interp,
            args: new_args,
        }
    };

    // ── Interpreter exists at the stated path ──
    if std::path::Path::new(interpreter).exists() {
        if is_executable {
            // Happy path — kernel will process the shebang directly.
            return ResolvedExec::passthrough(binary, args);
        }
        // File isn't executable — invoke the interpreter explicitly so the
        // kernel doesn't need the +x bit on the script itself.
        return invoke_explicitly(interpreter.to_string());
    }

    // ── Interpreter doesn't exist at the stated path — try to find it by name ──
    let interp_name = std::path::Path::new(interpreter)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if !interp_name.is_empty() {
        if let Some(found) = find_on_system(interp_name) {
            handle.push_log(LogLine {
                seq: 0,
                timestamp: Utc::now(),
                line: format!(
                    "Shebang interpreter '{}' not found — using '{}' instead",
                    interpreter, found
                ),
                stream: LogStream::Stdout,
            });
            return invoke_explicitly(found);
        }
    }

    // ── Could not locate the interpreter at all — last resort: sh ──
    handle.push_log(LogLine {
        seq: 0,
        timestamp: Utc::now(),
        line: format!(
            "Shebang interpreter '{}' not found and could not be located — falling back to sh",
            interpreter
        ),
        stream: LogStream::Stdout,
    });
    run_via_sh(binary, args)
}

/// Build a `ResolvedExec` that runs `sh <script> [args...]`.
fn run_via_sh(binary: &str, args: &[String]) -> ResolvedExec {
    let sh = find_on_system("sh").unwrap_or_else(|| "/bin/sh".to_string());
    let mut new_args = vec![binary.to_string()];
    new_args.extend(args.iter().cloned());
    ResolvedExec {
        command: sh,
        args: new_args,
    }
}

// ─── PID file helpers ───────────────────────────────────────────────────
//
// We persist the PID of every running server process to a file at
// `data/servers/<uuid>/.anyserver.pid`.  This survives AnyServer crashes
// and lets the reconciliation step on the next startup detect orphaned
// processes that are still alive.

/// Return the path to the PID file for a given server.
pub fn pid_file_path(data_dir: &Path, server_id: &Uuid) -> PathBuf {
    data_dir
        .join("servers")
        .join(server_id.to_string())
        .join(".anyserver.pid")
}

/// Write the PID of a running server process to its PID file.
pub fn write_pid_file(data_dir: &Path, server_id: &Uuid, pid: u32) {
    let path = pid_file_path(data_dir, server_id);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&path, pid.to_string()) {
        tracing::warn!(
            "Failed to write PID file for server {} at {}: {}",
            server_id,
            path.display(),
            e,
        );
    }
}

/// Read the PID from a server's PID file, if it exists and is valid.
pub fn read_pid_file(data_dir: &Path, server_id: &Uuid) -> Option<u32> {
    let path = pid_file_path(data_dir, server_id);
    match std::fs::read_to_string(&path) {
        Ok(contents) => contents.trim().parse::<u32>().ok(),
        Err(_) => None,
    }
}

/// Remove a server's PID file (best-effort — ignores errors).
pub fn remove_pid_file(data_dir: &Path, server_id: &Uuid) {
    let path = pid_file_path(data_dir, server_id);
    let _ = std::fs::remove_file(&path);
}

/// Check whether a process with the given PID is still alive.
///
/// Uses `kill(pid, 0)` which checks for process existence without
/// sending a signal.  Returns `false` if the process doesn't exist
/// or we lack permission to signal it.
pub fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // kill(pid, 0) returns 0 if the process exists and we have
        // permission to send it a signal; -1 with ESRCH if it doesn't.
        let ret = unsafe { libc::kill(pid as i32, 0) };
        if ret == 0 {
            return true;
        }
        // EPERM means the process exists but we can't signal it — still alive.
        let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        errno == libc::EPERM
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Reconcile orphaned server processes on startup.
///
/// For every server in the database that has a recorded PID file, check
/// whether the process is still alive:
///
/// - **Alive** — create a `ProcessHandle` with status `Running` and the
///   known PID.  Because the original stdout/stderr pipes are gone, the
///   console will show a message explaining that output is unavailable
///   until the next restart.
/// - **Dead** — clean up the stale PID file and leave the server as
///   `Stopped`.
///
/// This **must** run before auto-start so that auto-start doesn't try to
/// double-launch an already-running server.
pub async fn reconcile_processes(state: &Arc<AppState>) {
    let servers = match state.db.list_servers().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to list servers for process reconciliation: {}", e);
            return;
        }
    };

    for server in &servers {
        let server_id = server.id;
        let pid = match read_pid_file(&state.data_dir, &server_id) {
            Some(p) => p,
            None => continue, // No PID file — nothing to reconcile.
        };

        if is_process_alive(pid) {
            tracing::info!(
                "Reconciliation: server {} has an orphaned process (PID {}) — re-adopting as Running",
                server_id,
                pid,
            );

            let (tx, _) = broadcast::channel::<WsMessage>(512);
            // Spawn a log writer for reconciled processes if disk logging is enabled.
            let log_file_sender = if server.config.log_to_disk {
                let server_dir = state.server_dir(&server_id);
                let max_bytes = (server.config.max_log_size_mb as u64) * 1024 * 1024;
                Some(super::log_writer::spawn_log_writer(&server_dir, max_bytes))
            } else {
                None
            };

            let handle = Arc::new(ProcessHandle {
                runtime: Mutex::new(ServerRuntime {
                    server_id,
                    status: ServerStatus::Running,
                    pid: Some(pid),
                    started_at: None, // We don't know when it originally started.
                    restart_count: 0,
                    next_restart_at: None,
                }),
                stdin: tokio::sync::Mutex::new(None),
                log_tx: tx,
                log_buffer: Mutex::new(VecDeque::with_capacity(LOG_BUFFER_SIZE)),
                log_seq: AtomicU32::new(0),
                monitor_handle: Mutex::new(None),
                global_tx: state.process_manager.global_tx.clone(),
                stop_command: Mutex::new(None),
                stop_signal: Mutex::new(None),
                stop_cancel: Mutex::new(CancellationToken::new()),
                restart_cancel: Mutex::new(CancellationToken::new()),
                log_file_sender,
                exit_notify: Arc::new(tokio::sync::Notify::new()),
            });

            // Push an informational message so the console isn't empty.
            handle.push_log(LogLine {
                seq: 0, // will be overwritten by push_log
                timestamp: Utc::now(),
                line: format!(
                    "Process was already running (PID {}) — console output unavailable until next restart.",
                    pid,
                ),
                stream: LogStream::Stderr,
            });

            state.process_manager.handles.insert(server_id, handle);
        } else {
            tracing::info!(
                "Reconciliation: server {} has a stale PID file (PID {} is dead) — cleaning up",
                server_id,
                pid,
            );
            remove_pid_file(&state.data_dir, &server_id);
        }
    }
}

const LOG_BUFFER_SIZE: usize = 1000;

/// Holds the live state for a single managed process.
pub struct ProcessHandle {
    pub runtime: Mutex<ServerRuntime>,
    /// Stdin uses tokio::sync::Mutex so we can hold it across .await without
    /// breaking Send bounds on the resulting futures.
    pub stdin: tokio::sync::Mutex<Option<tokio::process::ChildStdin>>,
    pub log_tx: broadcast::Sender<WsMessage>,
    pub log_buffer: Mutex<VecDeque<LogLine>>,
    /// Monotonically increasing sequence number for log lines.
    /// Clients use this to deduplicate after WebSocket reconnection
    /// and to detect gaps in the log stream.
    pub log_seq: AtomicU32,
    /// Handle to the background monitor task so we can abort it on stop.
    pub monitor_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Clone of the global broadcast channel so status changes are also
    /// forwarded to dashboard-level subscribers.
    pub global_tx: broadcast::Sender<WsMessage>,
    /// Runtime stop command override — set by `SetStopCommand` pipeline steps.
    /// Takes priority over `ServerConfig.stop_command` when present.
    pub stop_command: Mutex<Option<String>>,
    /// Runtime stop signal override — set by `SetStopSignal` pipeline steps.
    /// Takes priority over `ServerConfig.stop_signal` when present.
    pub stop_signal: Mutex<Option<crate::types::StopSignal>>,
    /// Per-attempt cancellation token for the stop sequence.  Replaced with
    /// a fresh token at the start of each `stop_server` call so that stale
    /// cancellations from a previous attempt never affect a new one.
    pub stop_cancel: Mutex<CancellationToken>,
    /// Per-attempt cancellation token for pending auto-restart countdown.
    /// Replaced with a fresh token at the start of each restart countdown.
    pub restart_cancel: Mutex<CancellationToken>,
    /// Optional sender for persisting console output to disk.
    /// `None` when `log_to_disk` is disabled for this server.
    pub log_file_sender: Option<super::log_writer::LogFileSender>,
    /// Fired by the monitor task when the child process exits.  Allows
    /// `stop_server` / `kill_server` to wake immediately instead of
    /// polling status every 50–100 ms.
    pub exit_notify: Arc<tokio::sync::Notify>,
}

impl ProcessHandle {
    fn push_log(&self, mut log: LogLine) {
        // Assign monotonically increasing sequence number.
        log.seq = self.log_seq.fetch_add(1, Ordering::Relaxed);
        {
            let mut buf = self.log_buffer.lock();
            if buf.len() >= LOG_BUFFER_SIZE {
                buf.pop_front();
            }
            buf.push_back(log.clone());
        }
        // Persist to disk if log-to-disk is enabled for this server.
        if let Some(ref sender) = self.log_file_sender {
            sender.send(&log);
        }
        // Ignore send errors (no active receivers is fine)
        let _ = self.log_tx.send(WsMessage::Log(log));
    }

    fn broadcast_status(&self) {
        let rt = self.runtime.lock().clone();
        let _ = self.log_tx.send(WsMessage::StatusChange(rt.clone()));
        // Also broadcast on the global channel so dashboard clients get live updates.
        let _ = self.global_tx.send(WsMessage::StatusChange(rt));
    }

    /// Broadcast a `StopProgress` message on both the per-server and global
    /// channels so WebSocket clients can render a countdown timer and phase.
    fn broadcast_stop_progress(&self, progress: crate::types::StopProgress) {
        let msg = WsMessage::StopProgress(progress.clone());
        let _ = self.log_tx.send(msg);
        let _ = self.global_tx.send(WsMessage::StopProgress(progress));
    }
}

/// Central registry of all running/managed processes.
pub struct ProcessManager {
    pub handles: DashMap<Uuid, Arc<ProcessHandle>>,
    /// Global broadcast channel for status changes across all servers.
    /// Dashboard-level WebSocket clients subscribe to this.
    pub global_tx: broadcast::Sender<WsMessage>,
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessManager {
    pub fn new() -> Self {
        let (global_tx, _) = broadcast::channel::<WsMessage>(256);
        Self {
            handles: DashMap::new(),
            global_tx,
        }
    }

    /// Subscribe to the global status broadcast channel.
    pub fn subscribe_global(&self) -> broadcast::Receiver<WsMessage> {
        self.global_tx.subscribe()
    }

    /// Get the current runtime info for a server.
    /// Returns a default "Stopped" runtime if the server has no active handle.
    pub fn get_runtime(&self, server_id: &Uuid) -> ServerRuntime {
        match self.handles.get(server_id) {
            Some(h) => h.runtime.lock().clone(),
            None => ServerRuntime {
                server_id: *server_id,
                status: ServerStatus::Stopped,
                pid: None,
                started_at: None,
                restart_count: 0,
                next_restart_at: None,
            },
        }
    }

    /// Subscribe to real-time log and status events for a server.
    pub fn subscribe(&self, server_id: &Uuid) -> Option<broadcast::Receiver<WsMessage>> {
        self.handles.get(server_id).map(|h| h.log_tx.subscribe())
    }

    /// Get the recent log buffer for a server.
    pub fn get_log_buffer(&self, server_id: &Uuid) -> Vec<LogLine> {
        match self.handles.get(server_id) {
            Some(h) => h.log_buffer.lock().iter().cloned().collect(),
            None => vec![],
        }
    }

    /// Ensure a ProcessHandle exists for a server so that broadcast
    /// subscribers (e.g. WebSocket clients) always have a channel to
    /// listen on.  If a handle already exists its `log_tx` is returned
    /// unchanged; otherwise a minimal "stopped" handle is created and
    /// inserted.
    pub fn ensure_handle(&self, server_id: Uuid) -> broadcast::Sender<WsMessage> {
        // Fast path – handle already exists.
        if let Some(existing) = self.handles.get(&server_id) {
            return existing.log_tx.clone();
        }

        // Slow path – create a minimal handle.
        let (tx, _) = broadcast::channel::<WsMessage>(512);
        let handle = Arc::new(ProcessHandle {
            runtime: Mutex::new(ServerRuntime {
                server_id,
                status: ServerStatus::Stopped,
                pid: None,
                started_at: None,
                restart_count: 0,
                next_restart_at: None,
            }),
            stdin: tokio::sync::Mutex::new(None),
            log_tx: tx.clone(),
            log_buffer: Mutex::new(VecDeque::with_capacity(LOG_BUFFER_SIZE)),
            log_seq: AtomicU32::new(0),
            monitor_handle: Mutex::new(None),
            global_tx: self.global_tx.clone(),
            stop_command: Mutex::new(None),
            stop_signal: Mutex::new(None),
            stop_cancel: Mutex::new(CancellationToken::new()),
            restart_cancel: Mutex::new(CancellationToken::new()),
            log_file_sender: None,
            exit_notify: Arc::new(tokio::sync::Notify::new()),
        });
        self.handles.insert(server_id, handle);
        tx
    }
}

/// Shared helper: wait until a log line matching `pattern` (case-insensitive
/// substring) appears on the broadcast channel, or until `timeout_secs`
/// elapses.
///
/// **Important:** the caller must subscribe to the broadcast channel
/// *before* retrieving the log buffer snapshot, then pass both here.
/// This eliminates the race where a line appears between the buffer
/// check and the subscription.
///
/// Returns `true` if the pattern was found, `false` on timeout / channel
/// closure.
pub async fn wait_for_output_pattern(
    mut rx: broadcast::Receiver<WsMessage>,
    existing_buffer: &[LogLine],
    pattern: &str,
    timeout_secs: u32,
) -> bool {
    let lc_pattern = pattern.to_lowercase();

    // 1. Check the existing buffer first — the line may already have appeared.
    for log in existing_buffer {
        if log.line.to_lowercase().contains(&lc_pattern) {
            return true;
        }
    }

    // 2. Listen on the broadcast channel until the pattern shows up or we
    //    hit the deadline.
    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }

        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(WsMessage::Log(log_line))) => {
                if log_line.line.to_lowercase().contains(&lc_pattern) {
                    return true;
                }
            }
            Ok(Ok(_)) => {
                // StatusChange, PhaseProgress, etc. — ignore.
            }
            Ok(Err(broadcast::error::RecvError::Lagged(n))) => {
                tracing::debug!("wait_for_output_pattern: lagged {} messages", n);
                // Continue — we might still catch the pattern.
            }
            Ok(Err(broadcast::error::RecvError::Closed)) => {
                tracing::debug!("wait_for_output_pattern: broadcast channel closed");
                return false;
            }
            Err(_) => {
                // Timeout elapsed.
                return false;
            }
        }
    }
}

/// Helper to clone a handle out of the DashMap in a small synchronous scope
/// so the non-Send `Ref` is dropped before any `.await`.
pub fn get_handle(state: &AppState, server_id: &Uuid) -> Option<Arc<ProcessHandle>> {
    state
        .process_manager
        .handles
        .get(server_id)
        .map(|r| r.value().clone())
}

/// Start a server process. Creates the ProcessHandle, spawns the binary,
/// wires up stdout/stderr streaming, and launches the exit-monitor task.
///
/// Returns a boxed future rather than being `async fn` so that the recursive
/// call from the monitor task's auto-restart path produces a finite,
/// `Send`-compatible type (breaking the infinite async state-machine cycle).
pub fn start_server(
    state: &Arc<AppState>,
    server_id: Uuid,
) -> Pin<Box<dyn Future<Output = Result<(), crate::error::AppError>> + Send + '_>> {
    let state = state.clone();
    Box::pin(async move {
        let server = state.db.get_server(server_id).await?.ok_or_else(|| {
            crate::error::AppError::NotFound(format!("Server {} not found", server_id))
        })?;

        // Don't double-start — scope the DashMap access so Ref drops immediately.
        {
            if let Some(existing) = get_handle(&state, &server_id) {
                let status = existing.runtime.lock().status;
                if status == ServerStatus::Running || status == ServerStatus::Starting {
                    return Err(crate::error::AppError::Conflict(
                        "Server is already running".into(),
                    ));
                }
                if status == ServerStatus::Stopping {
                    return Err(crate::error::AppError::Conflict(
                        "Cannot start server while it is still stopping. \
                         Cancel the stop first, or wait for it to complete."
                            .into(),
                    ));
                }
            }
        }

        let server_dir = state.server_dir(&server_id);
        std::fs::create_dir_all(&server_dir)?;
        // Canonicalize to an absolute path so that the binary and work_dir
        // paths derived from it survive the `current_dir` chdir in the
        // child process.  Without this, relative paths like
        // `./data/servers/<uuid>/start.sh` would be resolved relative to
        // the *new* working directory after chdir, not the parent's CWD.
        let server_dir = std::fs::canonicalize(&server_dir)?;

        // ── Run start pipeline (if any) ──
        //
        // The start pipeline is the modular configuration point for the
        // server process.  Steps like `SetEnv`, `SetWorkingDir`, and
        // `SetStopCommand` accumulate a `ProcessConfig` on the pipeline
        // handle, which we read after the pipeline completes and merge
        // with any defaults from `ServerConfig`.
        let pipeline_config = if !server.config.start_steps.is_empty() {
            tracing::info!(
                "Running {} start step(s) for server {}",
                server.config.start_steps.len(),
                server_id,
            );

            pipeline::run_phase(
                &state,
                server_id,
                PhaseKind::Start,
                server.config.start_steps.clone(),
                None,
            )?;

            // Wait for the start pipeline to finish before spawning the binary.
            // Poll the pipeline manager until it's no longer running (or a
            // reasonable timeout of 10 minutes).
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(600);
            loop {
                if !state.pipeline_manager.is_running(&server_id) {
                    break;
                }
                if tokio::time::Instant::now() >= deadline {
                    tracing::warn!(
                        "Start pipeline for server {} timed out after 10 minutes",
                        server_id,
                    );
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }

            // Check if the start pipeline succeeded
            if let Some(progress) = state.pipeline_manager.get_progress(&server_id) {
                if progress.status == crate::types::PhaseStatus::Failed {
                    return Err(crate::error::AppError::Internal(
                        "Start pipeline failed — server not started".into(),
                    ));
                }
            }

            // Read the accumulated ProcessConfig from the pipeline handle.
            state.pipeline_manager.get_process_config(&server_id)
        } else {
            None
        };

        // Build variable map for parameter substitution
        let vars = pipeline::variables::build_variables(&server, &server_dir, None);

        // ── Resolve working directory ──
        // Pipeline `SetWorkingDir` takes priority over `ServerConfig.working_dir`.
        let work_dir = {
            let effective_wd = pipeline_config
                .as_ref()
                .and_then(|pc| pc.working_dir.as_deref())
                .or(server.config.working_dir.as_deref());
            match effective_wd {
                Some(wd) => server_dir.join(pipeline::variables::substitute_variables(wd, &vars)),
                None => server_dir.clone(),
            }
        };
        std::fs::create_dir_all(&work_dir)?;

        let raw_binary = pipeline::variables::substitute_variables(&server.config.binary, &vars);
        let binary_path = if std::path::Path::new(&raw_binary).is_absolute() {
            raw_binary
        } else {
            let local_candidate = server_dir.join(&raw_binary);
            if local_candidate.exists() {
                // The binary exists inside the server directory — use it.
                local_candidate.to_string_lossy().to_string()
            } else if !raw_binary.contains('/') {
                // Bare command name (e.g. "java", "python3") — look it up on
                // the system PATH so we don't incorrectly prepend the server dir.
                find_on_system(&raw_binary).unwrap_or_else(|| {
                    // Fall back to the local path so spawn() produces a clear
                    // "not found" error with our existing diagnostics.
                    local_candidate.to_string_lossy().to_string()
                })
            } else {
                // Relative path with directory components (e.g. "bin/server") —
                // resolve against the server directory as before.
                local_candidate.to_string_lossy().to_string()
            }
        };

        // Substitute variables in args
        let resolved_args: Vec<String> = server
            .config
            .args
            .iter()
            .map(|a| pipeline::variables::substitute_variables(a, &vars))
            .collect();

        // ── Resolve environment variables ──
        // Start with ServerConfig.env defaults, then overlay any variables
        // set by `SetEnv` pipeline steps.
        let mut resolved_env: std::collections::HashMap<String, String> = server
            .config
            .env
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    pipeline::variables::substitute_variables(v, &vars),
                )
            })
            .collect();
        if let Some(ref pc) = pipeline_config {
            for (k, v) in &pc.env {
                resolved_env.insert(k.clone(), v.clone());
            }
        }

        // ── Auto-prepend JAVA_HOME/bin to PATH ──
        // When the user selects a Java runtime via the helper, only
        // `JAVA_HOME` is stored in the server config.  To make sure
        // shell scripts (and any sub-process that invokes bare `java`)
        // pick up the selected JDK, we prepend `$JAVA_HOME/bin` to the
        // inherited system PATH automatically.
        if let Some(java_home) = resolved_env.get("JAVA_HOME").cloned() {
            let java_bin_dir = std::path::PathBuf::from(&java_home).join("bin");
            if java_bin_dir.is_dir() {
                let current_path = std::env::var("PATH").unwrap_or_default();
                let new_path = format!("{}:{}", java_bin_dir.display(), current_path);
                // Only set PATH if the user hasn't explicitly provided one
                resolved_env.entry("PATH".to_string()).or_insert(new_path);
            }
        }

        // ── Resolve stop command ──
        // Pipeline `SetStopCommand` takes priority over `ServerConfig.stop_command`.
        let effective_stop_command = pipeline_config
            .as_ref()
            .and_then(|pc| pc.stop_command.clone())
            .or_else(|| server.config.stop_command.clone());

        // ── Resolve stop signal ──
        // Pipeline `SetStopSignal` takes priority over `ServerConfig.stop_signal`.
        let effective_stop_signal = pipeline_config
            .as_ref()
            .and_then(|pc| pc.stop_signal)
            .unwrap_or(server.config.stop_signal);

        // Preserve restart_count and broadcast channel from previous handle
        // if one exists.  Reusing the same `log_tx` keeps existing WebSocket
        // subscribers connected — they won't miss logs between the moment the
        // user clicks "Start" and the process actually spawns.
        let (prev_restart_count, existing_log_tx) = {
            match get_handle(&state, &server_id) {
                Some(h) => {
                    let count = h.runtime.lock().restart_count;
                    (count, Some(h.log_tx.clone()))
                }
                None => (0, None),
            }
        };

        let log_tx = existing_log_tx.unwrap_or_else(|| {
            let (tx, _) = broadcast::channel::<WsMessage>(512);
            tx
        });

        // Spawn a log writer if disk logging is enabled for this server.
        let log_file_sender = if server.config.log_to_disk {
            let server_dir = state.server_dir(&server_id);
            let max_bytes = (server.config.max_log_size_mb as u64) * 1024 * 1024;
            Some(super::log_writer::spawn_log_writer(&server_dir, max_bytes))
        } else {
            None
        };

        let handle = Arc::new(ProcessHandle {
            runtime: Mutex::new(ServerRuntime {
                server_id,
                status: ServerStatus::Starting,
                pid: None,
                started_at: Some(Utc::now()),
                restart_count: prev_restart_count,
                next_restart_at: None,
            }),
            stdin: tokio::sync::Mutex::new(None),
            log_tx,
            log_buffer: Mutex::new(VecDeque::with_capacity(LOG_BUFFER_SIZE)),
            log_seq: AtomicU32::new(0),
            monitor_handle: Mutex::new(None),
            global_tx: state.process_manager.global_tx.clone(),
            stop_command: Mutex::new(effective_stop_command),
            stop_signal: Mutex::new(Some(effective_stop_signal)),
            stop_cancel: Mutex::new(CancellationToken::new()),
            restart_cancel: Mutex::new(CancellationToken::new()),
            log_file_sender,
            exit_notify: Arc::new(tokio::sync::Notify::new()),
        });

        state
            .process_manager
            .handles
            .insert(server_id, Arc::clone(&handle));

        // Spawn the child process.
        // We pass the Arc<ProcessHandle> directly so spawn_and_monitor never touches DashMap.
        spawn_and_monitor(
            Arc::clone(&state),
            server_id,
            handle,
            binary_path,
            resolved_args,
            resolved_env,
            work_dir,
            server_dir,
            server.config.isolation.clone(),
        )
        .await?;

        Ok(())
    }) // end Box::pin(async move { ... })
}

/// Actually spawns the OS process, hooks up IO streams, and starts the monitor task.
/// All parameters are owned / Arc so the resulting future is Send + 'static.
/// The `handle` is passed in directly — we never call `DashMap::get` here.
#[allow(clippy::too_many_arguments)]
async fn spawn_and_monitor(
    state: Arc<AppState>,
    server_id: Uuid,
    handle: Arc<ProcessHandle>,
    binary: String,
    args: Vec<String>,
    env: std::collections::HashMap<String, String>,
    work_dir: PathBuf,
    server_dir: PathBuf,
    isolation: crate::types::IsolationConfig,
) -> Result<(), crate::error::AppError> {
    // ── Resolve how to execute the binary ──
    // This handles CRLF stripping, missing shebangs, broken interpreter
    // paths, and plain-text scripts — mirroring what a POSIX shell does.
    //
    // Runs on the blocking thread pool because resolve_execution()
    // performs synchronous filesystem I/O (stat, read, possibly write)
    // that must not block the async Tokio runtime threads.
    let resolved = {
        let binary_clone = binary.clone();
        let args_clone = args.clone();
        let handle_clone = Arc::clone(&handle);
        tokio::task::spawn_blocking(move || {
            resolve_execution(&binary_clone, &args_clone, &handle_clone)
        })
        .await
        .map_err(|e| {
            crate::error::AppError::Internal(format!("resolve_execution task panicked: {}", e))
        })?
    };

    // Debug logging for environment variables
    tracing::trace!(
        "Spawning server {} with {} custom environment variables",
        server_id,
        env.len()
    );
    for (k, v) in env.iter() {
        tracing::trace!("  ENV: {}={}", k, v);
    }

    let mut cmd = Command::new(&resolved.command);
    cmd.args(&resolved.args)
        .envs(env.iter())
        .current_dir(&work_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    // Put the child in its own process group (via setsid) so that
    // kill(-pid, SIGKILL) terminates the entire process tree — not
    // just the direct child.  Without this, killing a shell wrapper
    // leaves the actual game server running as an orphan.
    //
    // After setsid, apply the sandbox (Landlock, NO_NEW_PRIVS, FD
    // cleanup, RLIMIT_NPROC) so the child process is isolated before
    // it execs the server binary.
    let sandbox = crate::sandbox::PreExecSandbox::new(&server_dir, &isolation);
    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(move || {
            libc::setsid();
            sandbox.apply()?;
            Ok(())
        });
    }

    let mut child: Child = cmd.spawn().map_err(|e| {
        // Mark as crashed if spawn fails
        handle.runtime.lock().status = ServerStatus::Crashed;
        handle.broadcast_status();

        // ── Emit detailed diagnostics to the log buffer so the console
        //    shows *why* the process failed to start. ──
        let error_msg = format!("Failed to spawn process: {}", e);
        handle.push_log(LogLine {
            seq: 0,
            timestamp: Utc::now(),
            line: error_msg.clone(),
            stream: LogStream::Stderr,
        });
        handle.push_log(LogLine {
            seq: 0,
            timestamp: Utc::now(),
            line: format!("  Command: {} {:?}", resolved.command, resolved.args),
            stream: LogStream::Stderr,
        });
        handle.push_log(LogLine {
            seq: 0,
            timestamp: Utc::now(),
            line: format!("  Binary : {}", binary),
            stream: LogStream::Stderr,
        });
        handle.push_log(LogLine {
            seq: 0,
            timestamp: Utc::now(),
            line: format!("  WorkDir: {}", work_dir.display()),
            stream: LogStream::Stderr,
        });

        // Surface additional hints depending on what went wrong.
        let binary_path = std::path::Path::new(&binary);
        if !binary_path.exists() {
            // Distinguish between a bare command name (e.g. "java") that
            // couldn't be found on the system PATH vs. a local file that
            // is genuinely missing from the server directory.
            //
            // When a bare command like "java" fails PATH lookup, start_server
            // falls back to joining it with the server dir, producing e.g.
            // "/path/to/servers/<uuid>/java".  We detect this by checking
            // whether the file sits directly inside the server dir and has
            // no extension — a strong signal it was a bare command name.
            let file_name = binary_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let is_likely_bare_command = binary_path.parent() == Some(&server_dir)
                && !file_name.is_empty()
                && !file_name.contains('.');

            let hint = if is_likely_bare_command {
                format!(
                    "  Hint   : '{}' was not found on this system — is it installed and on the PATH?",
                    file_name
                )
            } else {
                format!(
                    "  Hint   : the file '{}' does not exist — did you run the install pipeline first?",
                    binary_path.display()
                )
            };
            handle.push_log(LogLine {
                seq: 0,
                timestamp: Utc::now(),
                line: hint,
                stream: LogStream::Stderr,
            });
        } else if !std::path::Path::new(&resolved.command).exists() {
            handle.push_log(LogLine {
                seq: 0,
                timestamp: Utc::now(),
                line: format!(
                    "  Hint   : resolved command '{}' does not exist",
                    resolved.command
                ),
                stream: LogStream::Stderr,
            });
        }

        crate::error::AppError::Internal(error_msg)
    })?;

    let pid = child.id();

    // Persist the PID to disk so reconciliation can find orphaned processes
    // after an unclean AnyServer shutdown.
    if let Some(pid_val) = pid {
        write_pid_file(&state.data_dir, &server_id, pid_val);
    }

    // Update handle to Running — scope the parking_lot lock so it drops before any .await
    {
        let mut rt = handle.runtime.lock();
        rt.status = ServerStatus::Running;
        rt.pid = pid;
        rt.started_at = Some(Utc::now());
    }

    // Hand off child stdin (tokio::sync::Mutex — safe to hold across .await)
    {
        let mut stdin_guard = handle.stdin.lock().await;
        *stdin_guard = child.stdin.take();
    }

    handle.broadcast_status();

    // ── Stream stdout ──
    if let Some(stdout) = child.stdout.take() {
        let h = Arc::clone(&handle);
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                h.push_log(LogLine {
                    seq: 0,
                    timestamp: Utc::now(),
                    line,
                    stream: LogStream::Stdout,
                });
            }
        });
    }

    // ── Stream stderr ──
    if let Some(stderr) = child.stderr.take() {
        let h = Arc::clone(&handle);
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                h.push_log(LogLine {
                    seq: 0,
                    timestamp: Utc::now(),
                    line,
                    stream: LogStream::Stderr,
                });
            }
        });
    }

    // ── Monitor task: waits for exit, handles auto-restart ──
    let state_clone = Arc::clone(&state);
    let handle_clone = Arc::clone(&handle);
    let data_dir_clone = state.data_dir.clone();
    let monitor = tokio::spawn(async move {
        let exit_status = child.wait().await;
        tracing::info!("Server {} process exited: {:?}", server_id, exit_status);

        // Clean up the PID file now that the process has exited.
        remove_pid_file(&data_dir_clone, &server_id);

        // Check if we should restart (need to check status before dropping the lock)
        let (should_check_restart, _current_status) = {
            let mut rt = handle_clone.runtime.lock();
            rt.pid = None;

            let status = rt.status;
            match status {
                ServerStatus::Stopping => {
                    // Graceful stop — don't restart
                    rt.status = ServerStatus::Stopped;
                    (false, status)
                }
                ServerStatus::Running | ServerStatus::Starting => {
                    // Unexpected exit
                    rt.status = ServerStatus::Crashed;
                    (true, status)
                }
                _ => (false, status),
            }
        };

        // Wake anyone waiting on process exit (stop_server, kill_server).
        handle_clone.exit_notify.notify_waiters();

        let should_restart = if should_check_restart {
            let server = state_clone.db.get_server(server_id).await.ok().flatten();
            if let Some(ref server) = server {
                // ── Alert: server crashed ──
                state_clone.alert_dispatcher.notify_server_crashed(
                    &state_clone,
                    server_id,
                    &server.config.name,
                );

                let max = server.config.max_restart_attempts;
                let restart_count = {
                    let rt = handle_clone.runtime.lock();
                    rt.restart_count
                };
                let will_restart = server.config.auto_restart && restart_count < max;

                if !will_restart && max > 0 {
                    // ── Alert: restart attempts exhausted ──
                    state_clone.alert_dispatcher.notify_restart_exhausted(
                        &state_clone,
                        server_id,
                        &server.config.name,
                        restart_count,
                        max,
                    );
                }

                will_restart
            } else {
                false
            }
        } else {
            false
        };

        // Broadcast new status
        handle_clone.broadcast_status();

        if should_restart {
            let server = state_clone.db.get_server(server_id).await.ok().flatten();
            if let Some(server) = server {
                let delay = server.config.restart_delay_secs;
                let attempt = handle_clone.runtime.lock().restart_count + 1;

                // Set next_restart_at timestamp for UI countdown
                let next_restart = Utc::now() + chrono::Duration::seconds(delay as i64);
                {
                    let mut rt = handle_clone.runtime.lock();
                    rt.next_restart_at = Some(next_restart);
                }
                handle_clone.broadcast_status();

                tracing::info!(
                    "Auto-restarting server {} in {}s (attempt {})",
                    server_id,
                    delay,
                    attempt,
                );

                // Create a fresh per-attempt cancellation token so that
                // stale cancellations from a previous crash cycle don't
                // immediately abort this countdown.
                let restart_token = CancellationToken::new();
                {
                    let mut lock = handle_clone.restart_cancel.lock();
                    lock.cancel(); // cancel any previous token
                    *lock = restart_token.clone();
                }

                // Wait for the restart delay, but allow cancellation.
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(delay as u64)) => {}
                    _ = restart_token.cancelled() => {
                        tracing::info!(
                            "Auto-restart for server {} was cancelled by user",
                            server_id
                        );
                        // Clear next_restart_at
                        {
                            let mut rt = handle_clone.runtime.lock();
                            rt.next_restart_at = None;
                        }
                        handle_clone.broadcast_status();
                        return;
                    }
                }

                // Clear next_restart_at
                {
                    let mut rt = handle_clone.runtime.lock();
                    rt.next_restart_at = None;
                }

                // Bump restart count
                {
                    handle_clone.runtime.lock().restart_count += 1;
                }

                if let Err(e) = start_server(&state_clone, server_id).await {
                    tracing::error!("Auto-restart failed for {}: {}", server_id, e);
                    // Mark as crashed
                    handle_clone.runtime.lock().status = ServerStatus::Crashed;
                    handle_clone.broadcast_status();
                }
            }
        }
    });

    // Store the monitor handle so we can abort it if needed
    *handle.monitor_handle.lock() = Some(monitor);

    Ok(())
}

/// Gracefully stop a server process: send the configured stop command (or
/// SIGTERM if none is configured), wait for the timeout, then SIGKILL the
/// entire process group if the process is still alive.
/// Helper: compute elapsed seconds since stop began.
fn elapsed_secs(start: tokio::time::Instant) -> f64 {
    start.elapsed().as_secs_f64()
}

/// Helper: build a `StopProgress` snapshot (no step info).
fn make_progress(
    server_id: Uuid,
    phase: crate::types::StopPhase,
    start: tokio::time::Instant,
    timeout: f64,
    grace: u32,
) -> crate::types::StopProgress {
    crate::types::StopProgress {
        server_id,
        phase,
        elapsed_secs: elapsed_secs(start),
        timeout_secs: timeout,
        grace_secs: grace,
        step_info: None,
    }
}

/// Helper: build a `StopProgress` snapshot with step-level detail.
fn make_step_progress(
    server_id: Uuid,
    start: tokio::time::Instant,
    timeout: f64,
    grace: u32,
    info: crate::types::StopStepInfo,
) -> crate::types::StopProgress {
    crate::types::StopProgress {
        server_id,
        phase: crate::types::StopPhase::RunningStopSteps,
        elapsed_secs: elapsed_secs(start),
        timeout_secs: timeout,
        grace_secs: grace,
        step_info: Some(info),
    }
}

/// Extract the per-step timeout for a given stop step action, if it
/// is a time-bounded action (`Sleep` or `WaitForOutput`).
fn step_action_timeout(action: &crate::types::StepAction) -> Option<u32> {
    match action {
        crate::types::StepAction::Sleep { seconds } => Some(*seconds),
        crate::types::StepAction::WaitForOutput { timeout_secs, .. } => Some(*timeout_secs),
        _ => None,
    }
}

/// Estimate the maximum duration of all stop steps by summing the
/// worst-case time for each step (Sleep seconds, WaitForOutput
/// timeout, everything else ≈ 0).
fn estimate_steps_secs(steps: &[crate::types::PipelineStep]) -> f64 {
    steps
        .iter()
        .map(|s| match &s.action {
            crate::types::StepAction::Sleep { seconds } => *seconds as f64,
            crate::types::StepAction::WaitForOutput { timeout_secs, .. } => *timeout_secs as f64,
            _ => 0.0,
        })
        .sum()
}

/// Execute configured stop steps for a server.
/// Returns the total timeout that should be used (estimated step duration + grace period).
async fn execute_stop_steps(
    state: &Arc<AppState>,
    server_id: Uuid,
    server: &crate::types::Server,
    handle: &Arc<ProcessHandle>,
    stop_start: tokio::time::Instant,
    stop_timeout: u32,
    cancel_token: CancellationToken,
) -> Result<f64, crate::error::AppError> {
    let total_timeout = estimate_steps_secs(&server.config.stop_steps) + stop_timeout as f64;
    let stop_step_count = server.config.stop_steps.len() as u32;
    let server_dir = state.server_dir(&server_id);
    let vars = pipeline::variables::build_variables(server, &server_dir, None);

    for (i, step) in server.config.stop_steps.iter().enumerate() {
        // Broadcast per-step progress before each step executes.
        handle.broadcast_stop_progress(make_step_progress(
            server_id,
            stop_start,
            total_timeout,
            stop_timeout,
            crate::types::StopStepInfo {
                index: i as u32,
                total: stop_step_count,
                name: step.name.clone(),
                step_timeout_secs: step_action_timeout(&step.action),
            },
        ));

        // Check cancellation before each step.
        if cancel_token.is_cancelled() {
            tracing::info!(
                "Shutdown cancelled for server {} during stop steps",
                server_id
            );
            revert_to_running(handle, server_id, stop_start, total_timeout, stop_timeout);
            return Err(crate::error::AppError::Conflict(
                "Shutdown cancelled".into(),
            ));
        }

        // Check if the process already exited — no point continuing.
        {
            let current = handle.runtime.lock().status;
            if current == ServerStatus::Stopped || current == ServerStatus::Crashed {
                tracing::info!(
                    "Server {} exited during stop step {} — skipping remaining steps",
                    server_id,
                    i
                );
                return Ok(total_timeout);
            }
        }

        match &step.action {
            crate::types::StepAction::SendInput { text } => {
                let resolved = pipeline::variables::substitute_variables(text, &vars);
                tracing::info!(
                    "Stop step {} ({}): sending input \"{}\" to server {}",
                    i,
                    step.name,
                    resolved,
                    server_id
                );
                let mut stdin_guard = handle.stdin.lock().await;
                if let Some(ref mut stdin) = *stdin_guard {
                    let line = format!("{}\n", resolved);
                    let _ = stdin.write_all(line.as_bytes()).await;
                    let _ = stdin.flush().await;
                }
            }
            crate::types::StepAction::SendSignal { signal } => {
                let pid = { handle.runtime.lock().pid };
                if let Some(pid) = pid {
                    let (sig, sig_name) = match signal {
                        crate::types::StopSignal::Sigint => (libc::SIGINT, "SIGINT"),
                        crate::types::StopSignal::Sigterm => (libc::SIGTERM, "SIGTERM"),
                    };
                    tracing::info!(
                        "Stop step {} ({}): sending {} to server {} (pid {})",
                        i,
                        step.name,
                        sig_name,
                        server_id,
                        pid
                    );
                    unsafe {
                        libc::kill(-(pid as i32), sig);
                    }
                }
            }
            crate::types::StepAction::Sleep { seconds } => {
                tracing::info!(
                    "Stop step {} ({}): sleeping {}s for server {}",
                    i,
                    step.name,
                    seconds,
                    server_id
                );
                // Sleep is cancellation- and exit-aware so that a user
                // cancel or an early process exit doesn't have to wait
                // for the full duration to elapse.
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(*seconds as u64)) => {}
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            "Stop step {} ({}): sleep interrupted by cancellation for server {}",
                            i, step.name, server_id
                        );
                    }
                    _ = handle.exit_notify.notified() => {
                        tracing::info!(
                            "Stop step {} ({}): sleep interrupted by process exit for server {}",
                            i, step.name, server_id
                        );
                    }
                }
            }
            crate::types::StepAction::WaitForOutput {
                pattern,
                timeout_secs,
            } => {
                let resolved_pattern = pipeline::variables::substitute_variables(pattern, &vars);
                tracing::info!(
                    "Stop step {} ({}): waiting for \"{}\" in console output (timeout {}s) for server {}",
                    i, step.name, resolved_pattern, timeout_secs, server_id
                );

                let rx = handle.log_tx.subscribe();
                let buffer: Vec<LogLine> = handle.log_buffer.lock().iter().cloned().collect();

                // WaitForOutput is cancellation-aware so the user doesn't
                // have to wait for the full timeout if they cancel the stop.
                let found = tokio::select! {
                    found = wait_for_output_pattern(rx, &buffer, &resolved_pattern, *timeout_secs) => found,
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            "Stop step {} ({}): wait interrupted by cancellation for server {}",
                            i, step.name, server_id
                        );
                        false
                    }
                };

                if found {
                    tracing::info!(
                        "Stop step {} ({}): pattern \"{}\" matched for server {}",
                        i,
                        step.name,
                        resolved_pattern,
                        server_id
                    );
                } else {
                    tracing::info!(
                        "Stop step {} ({}): timed out waiting for \"{}\" for server {} — continuing",
                        i, step.name, resolved_pattern, server_id
                    );
                }
            }
            other => {
                tracing::warn!(
                    "Stop step {} ({}): unsupported action {:?} in stop pipeline — skipping",
                    i,
                    step.name,
                    other
                );
            }
        }
    }
    Ok(total_timeout)
}

/// Execute legacy stop behavior (stop command or signal).
/// Returns the total timeout (just the grace period for legacy mode).
async fn execute_legacy_stop(
    state: &Arc<AppState>,
    server_id: Uuid,
    server: &crate::types::Server,
    handle: &Arc<ProcessHandle>,
    stop_start: tokio::time::Instant,
    stop_timeout: u32,
) -> f64 {
    let effective_stop_cmd = {
        let runtime_cmd = handle.stop_command.lock().clone();
        runtime_cmd.or_else(|| server.config.stop_command.clone())
    };

    let effective_signal = {
        let runtime_sig = *handle.stop_signal.lock();
        runtime_sig.unwrap_or(server.config.stop_signal)
    };

    let pid = { handle.runtime.lock().pid };
    let total_timeout = stop_timeout as f64;

    if let Some(ref stop_cmd) = effective_stop_cmd {
        handle.broadcast_stop_progress(make_progress(
            server_id,
            crate::types::StopPhase::SendingStopCommand,
            stop_start,
            total_timeout,
            stop_timeout,
        ));

        let server_dir = state.server_dir(&server_id);
        let vars = pipeline::variables::build_variables(server, &server_dir, None);
        let resolved_cmd = pipeline::variables::substitute_variables(stop_cmd, &vars);

        let mut stdin_guard = handle.stdin.lock().await;
        if let Some(ref mut stdin) = *stdin_guard {
            let cmd = format!("{}\n", resolved_cmd);
            let _ = stdin.write_all(cmd.as_bytes()).await;
            let _ = stdin.flush().await;
        }
    } else if let Some(pid) = pid {
        let (sig, sig_name) = match effective_signal {
            crate::types::StopSignal::Sigint => (libc::SIGINT, "SIGINT"),
            crate::types::StopSignal::Sigterm => (libc::SIGTERM, "SIGTERM"),
        };
        tracing::info!(
            "No stop command configured for server {} — sending {} to process group",
            server_id,
            sig_name
        );
        unsafe {
            libc::kill(-(pid as i32), sig);
        }
    }

    total_timeout
}

pub async fn stop_server(
    state: &Arc<AppState>,
    server_id: Uuid,
) -> Result<(), crate::error::AppError> {
    let server = state.db.get_server(server_id).await?.ok_or_else(|| {
        crate::error::AppError::NotFound(format!("Server {} not found", server_id))
    })?;

    let handle = get_handle(state, &server_id)
        .ok_or_else(|| crate::error::AppError::Conflict("Server has no active process".into()))?;

    {
        let rt = handle.runtime.lock();
        if rt.status != ServerStatus::Running && rt.status != ServerStatus::Starting {
            return Err(crate::error::AppError::Conflict(
                "Server is not running".into(),
            ));
        }
    }

    // Create a fresh per-attempt cancellation token.  Any previous token
    // is cancelled (no-op if already cancelled/dropped) so that stale
    // waiters from a prior stop attempt wake up and exit harmlessly.
    let cancel_token = CancellationToken::new();
    {
        let mut lock = handle.stop_cancel.lock();
        lock.cancel(); // cancel previous attempt's token (if any)
        *lock = cancel_token.clone();
    }

    {
        handle.runtime.lock().status = ServerStatus::Stopping;
    }
    handle.broadcast_status();

    let stop_timeout = server.config.stop_timeout_secs;
    let stop_start = tokio::time::Instant::now();

    // Execute stop steps or legacy stop behavior
    let _total_timeout = if !server.config.stop_steps.is_empty() {
        tracing::info!(
            "Running {} stop step(s) for server {} ...",
            server.config.stop_steps.len(),
            server_id
        );
        match execute_stop_steps(
            state,
            server_id,
            &server,
            &handle,
            stop_start,
            stop_timeout,
            cancel_token.clone(),
        )
        .await
        {
            Ok(timeout) => timeout,
            Err(_) => return Ok(()), // Cancelled
        }
    } else {
        execute_legacy_stop(state, server_id, &server, &handle, stop_start, stop_timeout).await
    };

    // Recalculate total_timeout for the grace period
    let total_timeout = elapsed_secs(stop_start) + stop_timeout as f64;

    handle.broadcast_stop_progress(make_progress(
        server_id,
        crate::types::StopPhase::WaitingForExit,
        stop_start,
        total_timeout,
        stop_timeout,
    ));

    // Wait for the process to exit gracefully.
    //
    // Instead of a tight poll loop we `select!` on three events:
    //   1. The grace-period timer fires  → break to SIGKILL
    //   2. The process exits (Notify)    → return early, nothing to kill
    //   3. The user cancels the stop     → revert to Running
    //
    // A 1-second ticker drives progress broadcasts for the UI.
    let timeout_secs = server.config.stop_timeout_secs as u64;
    let grace_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let mut progress_interval = tokio::time::interval(std::time::Duration::from_secs(1));
    progress_interval.tick().await; // consume the immediate first tick

    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(grace_deadline) => {
                // Grace period expired — fall through to SIGKILL.
                break;
            }
            _ = handle.exit_notify.notified() => {
                let current = handle.runtime.lock().status;
                if current == ServerStatus::Stopped || current == ServerStatus::Crashed {
                    return Ok(());
                }
                // Spurious wake (e.g. from a previous cycle) — keep waiting.
            }
            _ = cancel_token.cancelled() => {
                tracing::info!(
                    "Shutdown cancelled for server {} during grace period",
                    server_id
                );
                revert_to_running(&handle, server_id, stop_start, total_timeout, stop_timeout);
                return Ok(());
            }
            _ = progress_interval.tick() => {
                // Check if the process already exited between ticks.
                let current = handle.runtime.lock().status;
                if current == ServerStatus::Stopped || current == ServerStatus::Crashed {
                    return Ok(());
                }
                handle.broadcast_stop_progress(make_progress(
                    server_id,
                    crate::types::StopPhase::WaitingForExit,
                    stop_start,
                    total_timeout,
                    stop_timeout,
                ));
            }
        }
    }

    // SIGKILL phase
    handle.broadcast_stop_progress(make_progress(
        server_id,
        crate::types::StopPhase::SendingSigkill,
        stop_start,
        total_timeout,
        stop_timeout,
    ));

    let pid = { handle.runtime.lock().pid };
    let server_dir = state.server_dir(&server_id);
    let mut pgkill_succeeded = false;

    if let Some(pid) = pid {
        let current = handle.runtime.lock().status;
        if current == ServerStatus::Stopping {
            tracing::warn!(
                "Force-killing server {} (process group {}) after {}s timeout",
                server_id,
                pid,
                server.config.stop_timeout_secs
            );
            let ret = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
            if ret == 0 {
                pgkill_succeeded = true;
            } else {
                let err = std::io::Error::last_os_error();
                tracing::error!(
                    "kill(-{}, SIGKILL) failed for server {} (process group {}): {}",
                    pid,
                    server_id,
                    pid,
                    err,
                );
            }
        }
    } else {
        tracing::warn!(
            "No PID available for server {} at SIGKILL phase — \
             falling back to directory scan",
            server_id,
        );
    }

    // Give the monitor task a moment to pick up the exit.
    // Wait on the exit_notify instead of polling so we wake immediately.
    let exited = tokio::time::timeout(
        std::time::Duration::from_millis(2000),
        handle.exit_notify.notified(),
    )
    .await
    .is_ok();

    let current = handle.runtime.lock().status;
    if current != ServerStatus::Stopped && current != ServerStatus::Crashed {
        if !exited {
            // Process-group kill didn't work or the monitor didn't pick up
            // the exit.  Fall back to scanning /proc for any processes
            // whose cwd is the server directory and kill them individually.
            let dir = server_dir.clone();
            let fallback_killed = crate::utils::blocking(move || {
                Ok(crate::api::servers::kill_processes_in_directory(&dir))
            })
            .await
            .unwrap_or_default();

            if !fallback_killed.is_empty() {
                tracing::warn!(
                    "Fallback directory kill for server {} cleaned up {} process(es): {:?}",
                    server_id,
                    fallback_killed.len(),
                    fallback_killed,
                );
                // Give the monitor a short additional window to notice.
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            } else if !pgkill_succeeded {
                tracing::error!(
                    "SIGKILL failed and no orphan processes found in directory for server {} \
                     — the process may still be running outside our control",
                    server_id,
                );
            }
        }

        // Force status to Stopped so we don't stay in Stopping forever.
        let current = handle.runtime.lock().status;
        if current != ServerStatus::Stopped && current != ServerStatus::Crashed {
            tracing::warn!(
                "Monitor task did not detect exit for server {} — forcing Stopped",
                server_id
            );
            let mut rt = handle.runtime.lock();
            rt.status = ServerStatus::Stopped;
            rt.pid = None;
            drop(rt);
            handle.broadcast_status();
        }
    }

    Ok(())
}

/// Revert a server from `Stopping` back to `Running` when a shutdown is
/// cancelled.  Broadcasts both a `Cancelled` stop-progress message and a
/// `StatusChange` so the UI updates immediately.
fn revert_to_running(
    handle: &Arc<ProcessHandle>,
    server_id: Uuid,
    stop_start: tokio::time::Instant,
    total_timeout: f64,
    grace: u32,
) {
    use crate::types::{StopPhase, StopProgress};

    {
        let mut rt = handle.runtime.lock();
        if rt.status == ServerStatus::Stopping {
            rt.status = ServerStatus::Running;
        }
    }
    handle.broadcast_stop_progress(StopProgress {
        server_id,
        phase: StopPhase::Cancelled,
        elapsed_secs: stop_start.elapsed().as_secs_f64(),
        timeout_secs: total_timeout,
        grace_secs: grace,
        step_info: None,
    });
    handle.broadcast_status();
}

/// Cancel an in-progress graceful shutdown.  Sets the `stop_cancel` flag so
/// that the running `stop_server` future notices and reverts the server back
/// to `Running`.
///
/// Returns an error if the server is not currently in the `Stopping` state
/// or if SIGKILL has already been sent (past the point of no return).
pub fn cancel_stop_server(
    state: &Arc<AppState>,
    server_id: Uuid,
) -> Result<(), crate::error::AppError> {
    let handle = get_handle(state, &server_id)
        .ok_or_else(|| crate::error::AppError::Conflict("Server has no active process".into()))?;

    let current = handle.runtime.lock().status;
    if current != ServerStatus::Stopping {
        return Err(crate::error::AppError::Conflict(
            "Server is not currently stopping".into(),
        ));
    }

    tracing::info!("Cancel-stop requested for server {}", server_id);
    handle.stop_cancel.lock().cancel();

    Ok(())
}

/// Cancel a pending auto-restart.  Sets the `restart_cancel` flag so that
/// the monitor task notices and aborts the restart, leaving the server in
/// the `Crashed` state.
///
/// Returns an error if the server is not currently crashed with a pending
/// restart.
pub async fn cancel_restart(
    state: &Arc<AppState>,
    server_id: Uuid,
) -> Result<(), crate::error::AppError> {
    let handle = get_handle(state, &server_id)
        .ok_or_else(|| crate::error::AppError::Conflict("Server has no active process".into()))?;

    let (current_status, has_pending_restart) = {
        let rt = handle.runtime.lock();
        (rt.status, rt.next_restart_at.is_some())
    };

    if current_status != ServerStatus::Crashed {
        return Err(crate::error::AppError::Conflict(
            "Server is not in crashed state".into(),
        ));
    }

    if !has_pending_restart {
        return Err(crate::error::AppError::Conflict(
            "Server has no pending restart".into(),
        ));
    }

    tracing::info!("Cancel-restart requested for server {}", server_id);
    handle.restart_cancel.lock().cancel();

    // Clear next_restart_at immediately so UI updates
    {
        let mut rt = handle.runtime.lock();
        rt.next_restart_at = None;
    }
    handle.broadcast_status();

    Ok(())
}

/// Immediately SIGKILL a server's entire process group without any grace
/// period.  Use when a server is hung and won't respond to a normal stop.
///
/// Unlike `stop_server`, this does NOT send a stop command or SIGTERM first
/// — it goes straight to SIGKILL on the entire process tree.
pub async fn kill_server(
    state: &Arc<AppState>,
    server_id: Uuid,
) -> Result<(), crate::error::AppError> {
    let handle = get_handle(state, &server_id)
        .ok_or_else(|| crate::error::AppError::Conflict("Server has no active process".into()))?;

    let pid = {
        let mut rt = handle.runtime.lock();
        if rt.status != ServerStatus::Running
            && rt.status != ServerStatus::Starting
            && rt.status != ServerStatus::Stopping
        {
            return Err(crate::error::AppError::Conflict(
                "Server is not running".into(),
            ));
        }
        // Mark as stopping so the monitor doesn't auto-restart.
        rt.status = ServerStatus::Stopping;
        rt.pid
    };

    handle.broadcast_status();

    // SIGKILL the entire process group — this kills the direct child AND
    // all of its descendants (game server, child workers, etc.).
    if let Some(pid) = pid {
        tracing::warn!("Force-killing server {} (process group {})", server_id, pid,);
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }

    // Let the monitor task detect the exit naturally and transition the
    // status to Stopped.  SIGKILL is immediate so this should be fast.
    // Wait on exit_notify instead of polling.
    let exited = tokio::time::timeout(
        std::time::Duration::from_millis(2000),
        handle.exit_notify.notified(),
    )
    .await
    .is_ok();

    let current = handle.runtime.lock().status;
    if current != ServerStatus::Stopped && current != ServerStatus::Crashed {
        if !exited {
            // The monitor didn't pick up the exit — force the status and
            // abort the monitor as a last resort.
            tracing::warn!(
                "Monitor task did not detect exit for server {} after SIGKILL — forcing Stopped",
                server_id,
            );
        }
        {
            let mut rt = handle.runtime.lock();
            rt.status = ServerStatus::Stopped;
            rt.pid = None;
        }
        handle.broadcast_status();
        if let Some(task) = handle.monitor_handle.lock().take() {
            task.abort();
        }
    }

    Ok(())
}

/// Send a command string to a running server's stdin.
pub async fn send_command(
    state: &Arc<AppState>,
    server_id: Uuid,
    command: &str,
) -> Result<(), crate::error::AppError> {
    // Clone the handle out of DashMap synchronously so the Ref is dropped immediately.
    let handle = get_handle(state, &server_id)
        .ok_or_else(|| crate::error::AppError::Conflict("Server has no active process".into()))?;

    // Check status — scope the parking_lot lock
    {
        let rt = handle.runtime.lock();
        if rt.status != ServerStatus::Running {
            return Err(crate::error::AppError::Conflict(
                "Server is not running".into(),
            ));
        }
    }

    // Use tokio::sync::Mutex for stdin so we can safely .await while holding it
    let mut stdin_guard = handle.stdin.lock().await;
    if let Some(ref mut stdin) = *stdin_guard {
        let line = format!("{}\n", command);
        stdin.write_all(line.as_bytes()).await.map_err(|e| {
            crate::error::AppError::Internal(format!("Failed to write to stdin: {}", e))
        })?;
        stdin.flush().await.map_err(|e| {
            crate::error::AppError::Internal(format!("Failed to flush stdin: {}", e))
        })?;
        Ok(())
    } else {
        Err(crate::error::AppError::Conflict(
            "Server stdin is not available".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{LogLine, LogStream, WsMessage};
    use chrono::Utc;
    use tokio::sync::broadcast;

    fn make_log_line(text: &str) -> LogLine {
        LogLine {
            seq: 0,
            timestamp: Utc::now(),
            line: text.to_string(),
            stream: LogStream::Stdout,
        }
    }

    #[tokio::test]
    async fn test_wait_for_output_pattern_found_in_buffer() {
        let (tx, _) = broadcast::channel::<WsMessage>(16);
        let buffer = vec![
            make_log_line("Starting server..."),
            make_log_line("Server is ready!"),
            make_log_line("Listening on port 25565"),
        ];

        // Pattern exists in buffer — should return immediately without waiting.
        let rx = tx.subscribe();
        let found = wait_for_output_pattern(rx, &buffer, "server is ready", 5).await;
        assert!(found, "should find pattern in existing buffer");
    }

    #[tokio::test]
    async fn test_wait_for_output_pattern_case_insensitive() {
        let (tx, _) = broadcast::channel::<WsMessage>(16);
        let buffer = vec![make_log_line("SERVER READY")];

        let rx = tx.subscribe();
        let found = wait_for_output_pattern(rx, &buffer, "server ready", 5).await;
        assert!(found, "case-insensitive match should succeed");

        let rx = tx.subscribe();
        let found = wait_for_output_pattern(rx, &buffer, "Server Ready", 5).await;
        assert!(found, "mixed-case pattern should also match");
    }

    #[tokio::test]
    async fn test_wait_for_output_pattern_not_in_buffer_timeout() {
        let (tx, _) = broadcast::channel::<WsMessage>(16);
        let buffer = vec![make_log_line("Starting server...")];

        // Pattern does NOT exist — should time out after 1 second.
        let rx = tx.subscribe();
        let start = tokio::time::Instant::now();
        let found = wait_for_output_pattern(rx, &buffer, "never appears", 1).await;
        let elapsed = start.elapsed();

        assert!(!found, "should not find pattern that doesn't exist");
        assert!(
            elapsed >= std::time::Duration::from_millis(900),
            "should wait close to the timeout duration, elapsed: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_wait_for_output_pattern_arrives_via_broadcast() {
        let (tx, _) = broadcast::channel::<WsMessage>(16);
        let buffer: Vec<LogLine> = vec![];

        let rx = tx.subscribe();

        // Spawn a task that sends the matching log line after a short delay.
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let _ = tx_clone.send(WsMessage::Log(make_log_line("Server is ready!")));
        });

        let found = wait_for_output_pattern(rx, &buffer, "server is ready", 5).await;
        assert!(found, "should find pattern arriving via broadcast");
    }

    #[tokio::test]
    async fn test_wait_for_output_pattern_ignores_non_log_messages() {
        let (tx, _) = broadcast::channel::<WsMessage>(16);
        let buffer: Vec<LogLine> = vec![];

        let rx = tx.subscribe();

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            // Send a non-Log message first.
            let _ = tx_clone.send(WsMessage::StatusChange(ServerRuntime {
                server_id: Uuid::new_v4(),
                status: ServerStatus::Running,
                pid: Some(1234),
                started_at: None,
                restart_count: 0,
                next_restart_at: None,
            }));
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            // Then send the matching log line.
            let _ = tx_clone.send(WsMessage::Log(make_log_line("Done!")));
        });

        let found = wait_for_output_pattern(rx, &buffer, "Done!", 5).await;
        assert!(found, "should skip non-Log messages and find pattern");
    }

    #[tokio::test]
    async fn test_wait_for_output_pattern_channel_closed() {
        let (tx, _) = broadcast::channel::<WsMessage>(16);
        let buffer: Vec<LogLine> = vec![];

        let rx = tx.subscribe();

        // Drop the sender so the channel closes.
        drop(tx);

        let found = wait_for_output_pattern(rx, &buffer, "something", 5).await;
        assert!(!found, "should return false when channel is closed");
    }

    #[tokio::test]
    async fn test_wait_for_output_pattern_empty_buffer_and_no_match() {
        let (tx, _) = broadcast::channel::<WsMessage>(16);
        let buffer: Vec<LogLine> = vec![];

        let rx = tx.subscribe();

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let _ = tx_clone.send(WsMessage::Log(make_log_line("unrelated output")));
            let _ = tx_clone.send(WsMessage::Log(make_log_line("more unrelated")));
        });

        let found = wait_for_output_pattern(rx, &buffer, "server ready", 1).await;
        assert!(!found, "should not match unrelated log lines");
    }

    #[tokio::test]
    async fn test_wait_for_output_pattern_substring_match() {
        let (tx, _) = broadcast::channel::<WsMessage>(16);
        let buffer = vec![make_log_line(
            "[14:32:05 INFO]: Done (2.5s)! For help, type \"help\"",
        )];

        let rx = tx.subscribe();
        let found = wait_for_output_pattern(rx, &buffer, "Done", 5).await;
        assert!(found, "substring match should succeed");
    }

    #[tokio::test]
    async fn test_wait_for_output_pattern_zero_timeout_checks_buffer_only() {
        let (tx, _) = broadcast::channel::<WsMessage>(16);
        let buffer = vec![make_log_line("ready")];

        // With zero timeout and pattern in buffer, should find it.
        let rx = tx.subscribe();
        let found = wait_for_output_pattern(rx, &buffer, "ready", 0).await;
        assert!(
            found,
            "should find pattern in buffer even with zero timeout"
        );

        // With zero timeout and pattern NOT in buffer, should fail immediately.
        let rx = tx.subscribe();
        let start = tokio::time::Instant::now();
        let found = wait_for_output_pattern(rx, &buffer, "not here", 0).await;
        let elapsed = start.elapsed();
        assert!(!found, "should not find missing pattern with zero timeout");
        assert!(
            elapsed < std::time::Duration::from_millis(100),
            "zero timeout should return nearly instantly, elapsed: {:?}",
            elapsed
        );
    }

    // ─── Tests for read_file_header & resolve_execution ─────────────────

    /// Helper: create a ProcessHandle suitable for testing resolve_execution.
    fn make_test_handle() -> (Arc<ProcessHandle>, Arc<tokio::sync::Notify>) {
        let (tx, _) = broadcast::channel::<WsMessage>(128);
        let (global_tx, _) = broadcast::channel::<WsMessage>(128);
        let exit_notify = Arc::new(tokio::sync::Notify::new());
        let handle = Arc::new(ProcessHandle {
            runtime: Mutex::new(ServerRuntime {
                server_id: uuid::Uuid::new_v4(),
                status: ServerStatus::Stopped,
                pid: None,
                started_at: None,
                restart_count: 0,
                next_restart_at: None,
            }),
            stdin: tokio::sync::Mutex::new(None),
            log_tx: tx,
            log_buffer: Mutex::new(VecDeque::with_capacity(100)),
            log_seq: AtomicU32::new(0),
            monitor_handle: Mutex::new(None),
            global_tx,
            stop_command: Mutex::new(None),
            stop_signal: Mutex::new(None),
            stop_cancel: Mutex::new(CancellationToken::new()),
            restart_cancel: Mutex::new(CancellationToken::new()),
            log_file_sender: None,
            exit_notify: exit_notify.clone(),
        });
        (handle, exit_notify)
    }

    #[test]
    fn test_read_file_header_reads_at_most_max_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bigfile.bin");

        // Write 4096 bytes but request only 64.
        let data = vec![0xABu8; 4096];
        std::fs::write(&path, &data).unwrap();

        let header = read_file_header(&path, 64).unwrap();
        assert_eq!(header.len(), 64, "should read exactly max_bytes");
        assert!(header.iter().all(|&b| b == 0xAB));
    }

    #[test]
    fn test_read_file_header_returns_less_for_small_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tiny.txt");

        std::fs::write(&path, b"hello").unwrap();

        let header = read_file_header(&path, 512).unwrap();
        assert_eq!(header.len(), 5);
        assert_eq!(&header, b"hello");
    }

    #[test]
    fn test_read_file_header_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty");

        std::fs::write(&path, b"").unwrap();

        let header = read_file_header(&path, 512).unwrap();
        assert!(header.is_empty());
    }

    #[test]
    fn test_resolve_execution_identifies_elf_from_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server.bin");

        // Write a fake ELF file: magic header + 2 MB of junk.
        let mut data = b"\x7fELF".to_vec();
        data.extend(vec![0u8; 2 * 1024 * 1024]);
        std::fs::write(&path, &data).unwrap();

        let (handle, _notify) = make_test_handle();
        let path_str = path.to_string_lossy().to_string();
        let resolved = resolve_execution(&path_str, &["--port".into(), "25565".into()], &handle);

        assert_eq!(resolved.command, path_str, "ELF should execute directly");
        assert_eq!(resolved.args, vec!["--port", "25565"]);
    }

    #[test]
    fn test_resolve_execution_identifies_shebang_script() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("start.sh");

        // Write a small shell script with a valid shebang.
        std::fs::write(&path, "#!/bin/sh\necho hello\n").unwrap();

        // Make it executable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let (handle, _notify) = make_test_handle();
        let path_str = path.to_string_lossy().to_string();
        let resolved = resolve_execution(&path_str, &[], &handle);

        // With a valid shebang and executable bit, it should run the script directly.
        assert_eq!(
            resolved.command, path_str,
            "Script with valid shebang + exec bit should execute directly"
        );
    }

    #[test]
    fn test_resolve_execution_fixes_crlf_in_small_script() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crlf.sh");

        // Write a script with CRLF line endings.
        std::fs::write(&path, "#!/bin/sh\r\necho hello\r\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let (handle, _notify) = make_test_handle();
        let path_str = path.to_string_lossy().to_string();
        let _resolved = resolve_execution(&path_str, &[], &handle);

        // The file on disk should now have LF-only line endings.
        let content = std::fs::read(&path).unwrap();
        assert!(
            !content.contains(&b'\r'),
            "CRLF should have been stripped from the script"
        );
        assert_eq!(content, b"#!/bin/sh\necho hello\n");
    }

    #[test]
    fn test_resolve_execution_does_not_fix_crlf_for_large_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big_crlf.sh");

        // Write a script > MAX_SCRIPT_SIZE with CRLF.
        let mut data = b"#!/bin/sh\r\necho hello\r\n".to_vec();
        // Pad to just over 1 MB.
        while data.len() <= MAX_SCRIPT_SIZE as usize {
            data.extend(b"# padding line\r\n");
        }
        let original_len = data.len();
        std::fs::write(&path, &data).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let (handle, _notify) = make_test_handle();
        let path_str = path.to_string_lossy().to_string();
        let _resolved = resolve_execution(&path_str, &[], &handle);

        // The file should NOT have been modified — still has CRLF.
        let content = std::fs::read(&path).unwrap();
        assert_eq!(
            content.len(),
            original_len,
            "Large file should not be modified"
        );
        assert!(
            content.contains(&b'\r'),
            "CRLF should still be present in the large file"
        );

        // A warning should have been logged.
        let logs: Vec<_> = handle
            .log_buffer
            .lock()
            .iter()
            .map(|l| l.line.clone())
            .collect();
        assert!(
            logs.iter()
                .any(|l| l.contains("too large") && l.contains("CRLF")),
            "Should log a warning about the file being too large for CRLF fix. Logs: {:?}",
            logs
        );
    }

    #[test]
    fn test_resolve_execution_nonexistent_file_passes_through() {
        let (handle, _notify) = make_test_handle();
        let resolved = resolve_execution("/nonexistent/path/binary", &["arg1".into()], &handle);

        assert_eq!(resolved.command, "/nonexistent/path/binary");
        assert_eq!(resolved.args, vec!["arg1"]);
    }

    #[test]
    fn test_resolve_execution_no_shebang_runs_via_sh() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("noshebang.sh");

        // A plain text script with no shebang.
        std::fs::write(&path, "echo hello world\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let (handle, _notify) = make_test_handle();
        let path_str = path.to_string_lossy().to_string();
        let resolved = resolve_execution(&path_str, &["extra".into()], &handle);

        // Should fall back to sh.
        let sh_path = find_on_system("sh").unwrap_or_else(|| "/bin/sh".to_string());
        assert_eq!(resolved.command, sh_path, "should run via sh");
        assert_eq!(
            resolved.args[0], path_str,
            "script path should be first arg to sh"
        );
        assert_eq!(resolved.args[1], "extra", "user args should follow");
    }

    // ─── Tests for log sequence numbers ─────────────────────────────────

    #[test]
    fn test_log_seq_monotonically_increasing() {
        let (handle, _notify) = make_test_handle();

        for i in 0..10 {
            handle.push_log(LogLine {
                seq: 0,
                timestamp: Utc::now(),
                line: format!("line {}", i),
                stream: LogStream::Stdout,
            });
        }

        let buf = handle.log_buffer.lock();
        let seqs: Vec<u32> = buf.iter().map(|l| l.seq).collect();
        assert_eq!(seqs, (0..10).collect::<Vec<u32>>());
    }

    #[test]
    fn test_log_seq_resets_on_new_handle() {
        // First handle gets seq 0, 1, 2.
        let (handle1, _notify1) = make_test_handle();
        for _ in 0..3 {
            handle1.push_log(LogLine {
                seq: 0,
                timestamp: Utc::now(),
                line: "a".to_string(),
                stream: LogStream::Stdout,
            });
        }
        assert_eq!(handle1.log_seq.load(Ordering::Relaxed), 3);

        // Second handle starts at seq 0 again — independent counter.
        let (handle2, _notify2) = make_test_handle();
        handle2.push_log(LogLine {
            seq: 0,
            timestamp: Utc::now(),
            line: "b".to_string(),
            stream: LogStream::Stdout,
        });

        let buf = handle2.log_buffer.lock();
        assert_eq!(buf[0].seq, 0, "new handle should start seq at 0");
    }

    #[test]
    fn test_log_seq_overwrites_placeholder() {
        let (handle, _notify) = make_test_handle();

        // Push a log with a non-zero placeholder seq — push_log should overwrite it.
        handle.push_log(LogLine {
            seq: 9999,
            timestamp: Utc::now(),
            line: "overwrite me".to_string(),
            stream: LogStream::Stdout,
        });

        let buf = handle.log_buffer.lock();
        assert_eq!(
            buf[0].seq, 0,
            "push_log should overwrite the placeholder seq with the counter value"
        );
    }

    #[test]
    fn test_log_seq_preserved_in_broadcast() {
        let (handle, _notify) = make_test_handle();
        let mut rx = handle.log_tx.subscribe();

        handle.push_log(LogLine {
            seq: 0,
            timestamp: Utc::now(),
            line: "broadcast test".to_string(),
            stream: LogStream::Stdout,
        });

        // The broadcast message should carry the assigned seq.
        match rx.try_recv().unwrap() {
            WsMessage::Log(log) => {
                assert_eq!(log.seq, 0);
                assert_eq!(log.line, "broadcast test");
            }
            other => panic!("expected Log message, got {:?}", other),
        }

        handle.push_log(LogLine {
            seq: 0,
            timestamp: Utc::now(),
            line: "second".to_string(),
            stream: LogStream::Stdout,
        });

        match rx.try_recv().unwrap() {
            WsMessage::Log(log) => {
                assert_eq!(log.seq, 1);
            }
            other => panic!("expected Log message, got {:?}", other),
        }
    }
}
