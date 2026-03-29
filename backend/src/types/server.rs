use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::auth::EffectivePermission;
use super::pipeline::{ConfigParameter, PhaseProgress, PipelineStep};

// ─── Update Check Types ───

/// How often and by what strategy AnyServer checks for available updates.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UpdateCheck {
    /// Which strategy to use for determining the latest version.
    #[serde(flatten)]
    pub provider: UpdateCheckProvider,

    /// How often (in seconds) to automatically check in the background.
    /// `0` or `None` means manual checks only.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub interval_secs: Option<u64>,

    /// How long (in seconds) to cache a successful check result before
    /// re-fetching on the next check. Default: 300 (5 minutes).
    #[serde(default = "default_cache_secs")]
    #[ts(type = "number")]
    pub cache_secs: u64,
}

fn default_cache_secs() -> u64 {
    300
}

/// The strategy used to determine the latest available version.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum UpdateCheckProvider {
    /// Fetch a JSON API endpoint and extract the latest version.
    Api {
        url: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(default = "default_pick")]
        pick: VersionPick,
        #[serde(default)]
        value_key: Option<String>,
    },
    /// Compare against the template's current default value for the
    /// version parameter.
    TemplateDefault,
    /// Run a shell command that outputs the latest version to stdout.
    Command {
        command: String,
        #[serde(default = "default_command_timeout")]
        timeout_secs: u32,
    },
}

fn default_pick() -> VersionPick {
    VersionPick::Last
}
fn default_command_timeout() -> u32 {
    15
}

/// When a JSON path resolves to an array, which element to pick as the
/// latest version.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum VersionPick {
    First,
    Last,
}

/// The result of a single update check for a server.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UpdateCheckResult {
    pub server_id: Uuid,
    pub update_available: bool,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub checked_at: DateTime<Utc>,
    /// Human-readable error if the check failed.
    #[serde(default)]
    pub error: Option<String>,
}

/// Response for the bulk update-status endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UpdateCheckStatusResponse {
    pub results: Vec<UpdateCheckResult>,
}

// ─── Isolation Config ───

/// Per-server process isolation settings.
///
/// When `enabled` is `true` (the default), AnyServer applies as many
/// isolation layers as the host OS supports:
///
/// - **Landlock** (Linux 5.13+) — restricts filesystem access to the
///   server's data directory plus a set of read-only system paths.
/// - **`PR_SET_NO_NEW_PRIVS`** — prevents privilege escalation via
///   suid/sgid binaries.
/// - **FD cleanup** — closes inherited file descriptors beyond
///   stdin/stdout/stderr so the child can't touch AnyServer's DB,
///   sockets, etc.
/// - **`RLIMIT_NPROC`** — caps the number of child processes (fork-bomb
///   protection).
///
/// Each layer is independently runtime-detected.  If a layer isn't
/// available on the host, it is silently skipped.
///
/// On non-Linux platforms the entire config is accepted but treated as a
/// no-op.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct IsolationConfig {
    /// Master switch.  When `false`, all isolation is disabled for this
    /// server.  Defaults to `true`.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Additional host paths the server process may **read** (but not
    /// write).  Useful for custom JDK/Python/runtime installations that
    /// live outside the default system paths.
    #[serde(default)]
    pub extra_read_paths: Vec<String>,

    /// Additional host paths the server process may **read and write**.
    /// Use sparingly — every entry widens the blast radius.
    #[serde(default)]
    pub extra_rw_paths: Vec<String>,

    /// Maximum number of child processes (threads + forks) the server may
    /// create.  `None` means no limit (the default).
    ///
    /// **Caution:** this sets `RLIMIT_NPROC` which is a **per-UID** limit,
    /// not per-process.  Setting it too low can prevent the server (and
    /// other processes belonging to the same OS user) from forking at all.
    /// Only set this if you understand the implications.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub pids_max: Option<u64>,
}

impl Default for IsolationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extra_read_paths: Vec::new(),
            extra_rw_paths: Vec::new(),
            pids_max: None,
        }
    }
}

fn default_true() -> bool {
    true
}

// ─── Stop Signal ───

/// Which signal to send to the process group during graceful stop when no
/// stop command is configured.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "lowercase")]
pub enum StopSignal {
    /// SIGTERM (default) — standard graceful termination.
    #[default]
    Sigterm,
    /// SIGINT — equivalent to Ctrl+C.
    Sigint,
}

// ─── Server Configuration ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ServerConfig {
    pub name: String,
    pub binary: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub auto_restart: bool,
    #[serde(default)]
    pub max_restart_attempts: u32,
    #[serde(default = "default_restart_delay")]
    pub restart_delay_secs: u32,
    #[serde(default)]
    pub stop_command: Option<String>,
    #[serde(default)]
    pub stop_signal: StopSignal,
    #[serde(default = "default_stop_timeout")]
    pub stop_timeout_secs: u32,
    #[serde(default)]
    pub sftp_username: Option<String>,
    #[serde(default)]
    pub sftp_password: Option<String>,
    #[serde(default)]
    pub parameters: Vec<ConfigParameter>,
    #[serde(default)]
    pub stop_steps: Vec<PipelineStep>,
    #[serde(default)]
    pub start_steps: Vec<PipelineStep>,
    #[serde(default)]
    pub install_steps: Vec<PipelineStep>,
    #[serde(default)]
    pub update_steps: Vec<PipelineStep>,
    #[serde(default)]
    pub uninstall_steps: Vec<PipelineStep>,
    /// Process isolation settings.  Defaults to enabled with sensible
    /// values — see [`IsolationConfig`].
    #[serde(default)]
    pub isolation: IsolationConfig,
    /// Optional configuration for automatic update detection.
    /// When present, AnyServer can check whether a newer version is
    /// available and surface it in the UI.
    #[serde(default)]
    pub update_check: Option<UpdateCheck>,
    /// Whether to persist console output (stdout/stderr) to a log file
    /// under `<server_dir>/logs/console.log`.  Defaults to `true`.
    #[serde(default = "default_log_to_disk")]
    pub log_to_disk: bool,
    /// Maximum size of a single console log file in megabytes before
    /// rotation occurs.  Defaults to 50 MB.  Up to 3 files are kept
    /// (current + 2 rotated), capping total log usage at ~150 MB.
    #[serde(default = "default_max_log_size_mb")]
    pub max_log_size_mb: u32,
    /// Manually enable Java runtime helper even if the binary path doesn't
    /// match auto-detection patterns. When enabled, the Java runtime selector
    /// is always shown in the wizard, allowing the user to configure a Java
    /// runtime for custom binaries that require Java underneath.
    #[serde(default)]
    pub enable_java_helper: bool,
    /// Manually enable .NET runtime helper even if the binary path doesn't
    /// match auto-detection patterns. When enabled, the .NET runtime selector
    /// is always shown in the wizard, allowing the user to configure a .NET
    /// runtime for custom binaries that require .NET underneath.
    #[serde(default)]
    pub enable_dotnet_helper: bool,
    /// Optional Steam application ID for servers installed via SteamCMD.
    /// When set, the install/update pipelines can use `SteamCmdInstall` /
    /// `SteamCmdUpdate` step actions that reference this app ID.
    /// The ID is validated against the Steam store API on save.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub steam_app_id: Option<u32>,
}

fn default_log_to_disk() -> bool {
    true
}

fn default_max_log_size_mb() -> u32 {
    50
}

fn default_restart_delay() -> u32 {
    5
}

fn default_stop_timeout() -> u32 {
    10
}

// ─── Stored Server ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct Server {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub config: ServerConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub parameter_values: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub installed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_via_pipeline_at: Option<DateTime<Utc>>,
    /// The version string that was active when the last install or update
    /// pipeline completed successfully.
    #[serde(default)]
    pub installed_version: Option<String>,
    /// The template this server was originally created from, if any.
    #[serde(default)]
    pub source_template_id: Option<Uuid>,
}

// ─── Runtime Status ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
    Installing,
    Updating,
    Uninstalling,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ServerRuntime {
    pub server_id: Uuid,
    pub status: ServerStatus,
    pub pid: Option<u32>,
    pub started_at: Option<DateTime<Utc>>,
    pub restart_count: u32,
    /// When the next auto-restart will occur (if auto-restart is pending).
    /// Used to display a countdown timer in the UI.
    #[serde(default)]
    pub next_restart_at: Option<DateTime<Utc>>,
}

// ─── API Request / Response Types ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CreateServerRequest {
    pub config: ServerConfig,
    #[serde(default)]
    pub parameter_values: std::collections::HashMap<String, String>,
    /// The template this server was created from, if any.
    #[serde(default)]
    pub source_template_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UpdateServerRequest {
    pub config: ServerConfig,
    #[serde(default)]
    pub parameter_values: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ServerWithStatus {
    pub server: Server,
    pub runtime: ServerRuntime,
    pub permission: EffectivePermission,
    #[serde(default)]
    pub phase_progress: Option<PhaseProgress>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ServerListResponse {
    pub servers: Vec<ServerWithStatus>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SendCommandRequest {
    pub command: String,
}
