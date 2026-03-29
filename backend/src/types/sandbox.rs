use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use chrono::{DateTime, Utc};

// ─── Per-Server Sandbox Profile ───

/// Granular security sandbox configuration for an individual server instance.
///
/// Each toggle corresponds to a specific Linux security mechanism.
/// When the master `enabled` switch is off, all isolation is bypassed.
/// Individual toggles allow fine-tuning which layers are active.
///
/// This feature is gated behind the `sandbox_management_enabled` app setting,
/// which only the site owner can toggle.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SandboxProfile {
    pub server_id: Uuid,

    /// Master switch — when false, ALL isolation is disabled for this server.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// **Landlock** (Linux 5.13+) — restricts filesystem access to an
    /// allow-list of paths. The server's data directory is always read-write;
    /// system paths like `/usr`, `/lib`, `/etc` are read-only.
    /// Requires kernel support; silently skipped if unavailable.
    #[serde(default = "default_true")]
    pub landlock_enabled: bool,

    /// **PR_SET_NO_NEW_PRIVS** — irreversibly prevents the process (and
    /// its children) from gaining new privileges through suid/sgid binaries
    /// or file capabilities. Also a prerequisite for Landlock.
    #[serde(default = "default_true")]
    pub no_new_privs: bool,

    /// **FD Cleanup** — marks all file descriptors beyond stdin/stdout/stderr
    /// as close-on-exec before the child process runs. Prevents the server
    /// from accessing AnyServer's database connections, sockets, etc.
    #[serde(default = "default_true")]
    pub fd_cleanup: bool,

    /// **PR_SET_DUMPABLE=0** — prevents other processes from attaching via
    /// ptrace or reading `/proc/<pid>/mem`. Hardens against local
    /// information-disclosure attacks.
    #[serde(default = "default_true")]
    pub non_dumpable: bool,

    /// **PID + Mount Namespace Isolation** — runs the server in its own
    /// PID namespace (cannot see/signal other processes) and mount namespace
    /// (filesystem mount changes are private). Requires unprivileged user
    /// namespaces; silently skipped if unavailable.
    #[serde(default = "default_true")]
    pub namespace_isolation: bool,

    /// **RLIMIT_NPROC** — caps the maximum number of child processes
    /// (threads + forks) the server may create. Provides fork-bomb protection.
    /// `0` means no limit. **Caution:** this is a per-UID limit, not per-process.
    /// Setting it too low may affect other processes running as the same user.
    #[serde(default)]
    pub pids_max: u64,

    /// Additional host paths the server process may **read** (but not write).
    /// Useful for custom JDK, Python, or other runtime installations that
    /// live outside the default system paths.
    #[serde(default)]
    pub extra_read_paths: Vec<String>,

    /// Additional host paths the server process may **read and write**.
    /// Use sparingly — every entry widens the blast radius of a compromised
    /// server process.
    #[serde(default)]
    pub extra_rw_paths: Vec<String>,

    /// **Network Namespace Isolation** — runs the server in its own network
    /// namespace. Currently reserved for future use; most game servers need
    /// network access, so this defaults to off.
    #[serde(default)]
    pub network_isolation: bool,

    /// **Seccomp BPF** filter mode. Reserved for future implementation.
    /// - `"off"` — no seccomp filter (default)
    /// - `"basic"` — block obviously dangerous syscalls
    /// - `"strict"` — allow only a curated set of syscalls
    #[serde(default = "default_seccomp_mode")]
    pub seccomp_mode: String,

    /// When this profile was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Default for SandboxProfile {
    fn default() -> Self {
        Self {
            server_id: Uuid::nil(),
            enabled: true,
            landlock_enabled: true,
            no_new_privs: true,
            fd_cleanup: true,
            non_dumpable: true,
            namespace_isolation: true,
            pids_max: 0,
            extra_read_paths: Vec::new(),
            extra_rw_paths: Vec::new(),
            network_isolation: false,
            seccomp_mode: "off".to_string(),
            updated_at: Utc::now(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_seccomp_mode() -> String {
    "off".to_string()
}

// ─── Sandbox Capabilities Probe ───

/// Runtime-detected sandbox capabilities of the host system.
/// Returned by the API so the frontend can show what's actually available.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SandboxCapabilities {
    /// Whether Landlock is supported by the running kernel.
    pub landlock_available: bool,
    /// Landlock ABI version (e.g. 1, 2, 3, 4). `None` if unsupported.
    #[ts(type = "number | null")]
    pub landlock_abi_version: Option<i32>,
    /// Whether unprivileged user namespaces (PID + mount) are available.
    pub namespaces_available: bool,
    /// PR_SET_NO_NEW_PRIVS is always available on Linux.
    pub no_new_privs_available: bool,
    /// FD cleanup is always available on Linux.
    pub fd_cleanup_available: bool,
    /// PR_SET_DUMPABLE is always available on Linux.
    pub non_dumpable_available: bool,
    /// RLIMIT_NPROC is always available on Linux.
    pub rlimit_nproc_available: bool,
    /// Whether the sandbox management feature is enabled site-wide.
    pub feature_enabled: bool,
}

// ─── Request / Response types ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UpdateSandboxProfileRequest {
    /// Master switch.
    pub enabled: bool,
    /// Landlock filesystem sandboxing.
    pub landlock_enabled: bool,
    /// PR_SET_NO_NEW_PRIVS.
    pub no_new_privs: bool,
    /// FD cleanup beyond stdin/stdout/stderr.
    pub fd_cleanup: bool,
    /// PR_SET_DUMPABLE=0.
    pub non_dumpable: bool,
    /// PID + mount namespace isolation.
    pub namespace_isolation: bool,
    /// RLIMIT_NPROC (0 = no limit).
    pub pids_max: u64,
    /// Extra read-only paths for Landlock.
    #[serde(default)]
    pub extra_read_paths: Vec<String>,
    /// Extra read-write paths for Landlock.
    #[serde(default)]
    pub extra_rw_paths: Vec<String>,
    /// Network namespace isolation (reserved).
    #[serde(default)]
    pub network_isolation: bool,
    /// Seccomp BPF mode (reserved).
    #[serde(default = "default_seccomp_mode")]
    pub seccomp_mode: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SandboxProfileResponse {
    pub profile: SandboxProfile,
    pub capabilities: SandboxCapabilities,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ToggleSandboxFeatureRequest {
    /// Whether the sandbox management feature should be enabled site-wide.
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ToggleSandboxFeatureResponse {
    pub sandbox_management_enabled: bool,
}
