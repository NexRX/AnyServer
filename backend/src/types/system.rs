use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ─── Java Runtime Detection ───

/// A detected Java runtime installation on the host system.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct JavaRuntime {
    /// Absolute path to the `java` binary (e.g. `/usr/lib/jvm/java-21/bin/java`).
    pub path: String,
    /// The JAVA_HOME directory for this installation (e.g. `/usr/lib/jvm/java-21`).
    /// Derived by stripping the trailing `/bin/java` from the binary path.
    pub java_home: String,
    /// The full version string (e.g. `"21.0.2"`).
    pub version: String,
    /// The major version number (e.g. `21` for Java 21).
    #[ts(type = "number")]
    pub major_version: u32,
    /// Runtime/vendor name (e.g. `"OpenJDK Runtime Environment"`, `"GraalVM"`).
    pub runtime_name: String,
    /// Whether this is the default `java` on the system PATH.
    pub is_default: bool,
}

/// Response from `GET /api/system/java-runtimes`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct JavaRuntimesResponse {
    /// All detected Java installations, sorted by major version descending.
    pub runtimes: Vec<JavaRuntime>,
}

// ─── .NET Runtime Detection ───

/// A detected .NET runtime installation on the host system.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct DotnetRuntime {
    /// Runtime name (e.g. `"Microsoft.NETCore.App"`, `"Microsoft.AspNetCore.App"`).
    pub runtime_name: String,
    /// The full version string (e.g. `"6.0.36"`, `"8.0.23"`).
    pub version: String,
    /// The major version number (e.g. `6` for .NET 6, `8` for .NET 8).
    #[ts(type = "number")]
    pub major_version: u32,
    /// Absolute path to the runtime shared library directory.
    pub runtime_path: String,
    /// The root directory of the .NET installation (contains `dotnet` binary).
    pub installation_root: String,
    /// Whether this runtime comes from the default `dotnet` on the system PATH.
    pub is_default: bool,
}

/// Response from `GET /api/system/dotnet-runtimes`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct DotnetRuntimesResponse {
    /// All detected .NET runtimes, sorted by runtime name and version.
    pub runtimes: Vec<DotnetRuntime>,
}

// ─── CPU Metrics ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CpuMetrics {
    /// Overall CPU utilization as a percentage (0.0–100.0).
    pub overall_percent: f32,
    /// Per-core CPU utilization percentages.
    pub per_core_percent: Vec<f32>,
    /// 1-minute load average.
    pub load_avg_1: f64,
    /// 5-minute load average.
    pub load_avg_5: f64,
    /// 15-minute load average.
    pub load_avg_15: f64,
    /// Number of logical CPU cores.
    pub core_count: u32,
}

// ─── Memory Metrics ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct MemoryMetrics {
    /// Total physical memory in bytes.
    #[ts(type = "number")]
    pub total_bytes: u64,
    /// Used physical memory in bytes.
    #[ts(type = "number")]
    pub used_bytes: u64,
    /// Available physical memory in bytes.
    #[ts(type = "number")]
    pub available_bytes: u64,
    /// Total swap space in bytes.
    #[ts(type = "number")]
    pub swap_total_bytes: u64,
    /// Used swap space in bytes.
    #[ts(type = "number")]
    pub swap_used_bytes: u64,
}

// ─── Disk Metrics ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct DiskMetrics {
    /// Device or partition name (e.g. "/dev/sda1").
    pub name: String,
    /// Mount point (e.g. "/", "/home").
    pub mount_point: String,
    /// Total disk capacity in bytes.
    #[ts(type = "number")]
    pub total_bytes: u64,
    /// Used disk space in bytes.
    #[ts(type = "number")]
    pub used_bytes: u64,
    /// Free disk space in bytes.
    #[ts(type = "number")]
    pub free_bytes: u64,
    /// Filesystem type (e.g. "ext4", "btrfs").
    pub filesystem: String,
}

// ─── Network Metrics ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct NetworkMetrics {
    /// Interface name (e.g. "eth0", "wlan0").
    pub interface: String,
    /// Total bytes received since boot.
    #[ts(type = "number")]
    pub rx_bytes: u64,
    /// Total bytes transmitted since boot.
    #[ts(type = "number")]
    pub tx_bytes: u64,
}

// ─── Top-Level System Health Snapshot ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SystemHealth {
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub disks: Vec<DiskMetrics>,
    pub networks: Vec<NetworkMetrics>,
    /// Host uptime in seconds.
    #[ts(type = "number")]
    pub uptime_secs: u64,
    /// Hostname of the machine.
    pub hostname: String,
    /// Timestamp when this snapshot was taken.
    pub timestamp: DateTime<Utc>,
}

// ─── GitHub Integration ───

/// A GitHub release tag.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct GithubReleaseTag {
    /// The tag name (e.g. "v5.2.0", "1.20.4").
    pub name: String,
    /// The release name/title (may be the same as tag_name).
    pub title: String,
    /// When the release was published.
    pub published_at: DateTime<Utc>,
    /// The release notes/body (markdown).
    #[serde(default)]
    pub body: String,
}

/// Response from `GET /api/github/releases`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct GithubReleasesResponse {
    /// List of release tags, sorted by published_at ascending (oldest first).
    pub releases: Vec<GithubReleaseTag>,
    /// Whether this response used the cached result.
    pub cached: bool,
}

/// Query parameters for `GET /api/github/releases`.
#[derive(Debug, Clone, Deserialize)]
pub struct GithubReleasesQuery {
    /// GitHub repository in "owner/repo" format.
    pub repo: String,
}

/// A GitHub release asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubAsset {
    /// Asset ID.
    pub id: u64,
    /// Asset filename.
    pub name: String,
    /// Direct download URL.
    pub browser_download_url: String,
    /// Asset size in bytes.
    pub size: u64,
}

/// GitHub release details with assets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubRelease {
    /// The tag name.
    pub tag_name: String,
    /// The release name/title.
    pub name: String,
    /// When the release was published.
    pub published_at: DateTime<Utc>,
    /// List of assets attached to this release.
    pub assets: Vec<GithubAsset>,
}

// ─── System Settings ───

/// GitHub API configuration stored in system settings.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct GithubSettings {
    /// Optional GitHub Personal Access Token for private repos and higher rate limits.
    /// Stored encrypted in the database.
    #[serde(default)]
    pub api_token: Option<String>,
}

// ─── CurseForge Integration ───

/// CurseForge API configuration stored in system settings.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CurseForgeSettings {
    /// CurseForge API key (from https://console.curseforge.com/).
    /// Stored encrypted in the database.
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Request body for `PUT /api/admin/settings/curseforge`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SaveCurseForgeSettingsRequest {
    /// CurseForge API key. If empty or null, clears the existing key.
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Response from `GET /api/admin/settings/curseforge`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CurseForgeSettingsResponse {
    /// Whether a CurseForge API key is currently configured (doesn't reveal the actual key).
    pub has_key: bool,
}

/// A CurseForge file version option returned by the proxy endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CurseForgeFileOption {
    /// The file ID (used as the parameter value).
    pub value: String,
    /// The display name shown to the user.
    pub label: String,
}

/// Response from `GET /api/curseforge/files`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CurseForgeFilesResponse {
    /// List of available file versions.
    pub options: Vec<CurseForgeFileOption>,
}

/// Query parameters for `GET /api/curseforge/files`.
#[derive(Debug, Clone, Deserialize)]
pub struct CurseForgeFilesQuery {
    /// CurseForge project (mod) ID.
    pub project_id: u32,
}

/// Request body for `POST /api/admin/settings/github`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SaveGithubSettingsRequest {
    /// GitHub API token. If empty or null, clears the existing token.
    #[serde(default)]
    pub api_token: Option<String>,
}

/// Response from `GET /api/admin/settings/github`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct GithubSettingsResponse {
    /// Whether a GitHub token is currently configured (doesn't reveal the actual token).
    pub has_token: bool,
}

// ─── Integration Status (unified feature flags) ───

/// Unified integration/feature availability status.
///
/// Returned by `GET /api/integrations/status` to any authenticated user so
/// the frontend can proactively hide, disable, or annotate features whose
/// backing integrations haven't been configured by an admin.
///
/// **Security note:** This intentionally reveals *whether* integrations are
/// configured but never exposes secrets (API keys, tokens, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct IntegrationStatus {
    /// Whether a CurseForge API key has been configured by an admin.
    /// When `false`, CurseForge file-version dropdowns and
    /// `DownloadCurseForgeFile` pipeline steps will fail at runtime.
    pub curseforge_configured: bool,

    /// Whether a GitHub Personal Access Token has been configured.
    /// GitHub integration works without a token for public repositories
    /// (with lower rate limits), but private repos require one.
    /// When `false`, users may hit rate limits or fail on private repos.
    pub github_configured: bool,

    /// Whether the `steamcmd` binary is available on the host's PATH.
    pub steamcmd_available: bool,

    /// Whether SMTP email has been configured (for alert delivery).
    pub smtp_configured: bool,
}
