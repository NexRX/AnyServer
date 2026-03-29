use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::server::VersionPick;

// ─── Config Parameters (template variables) ───

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum ConfigParameterType {
    #[default]
    String,
    Number,
    Boolean,
    Select,
    /// A GitHub release tag selector. Requires `github_repo` to be set.
    /// Presents a searchable dropdown of release tags from the specified repo.
    GithubReleaseTag,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ConfigParameter {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub param_type: ConfigParameterType,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub regex: Option<String>,
    /// When `true`, this parameter is treated as the "version" for update
    /// detection purposes. At most one parameter per config should have
    /// this set.
    #[serde(default)]
    pub is_version: bool,
    /// Declarative API fetch definition for populating dropdown options
    /// dynamically from an external JSON API.
    #[serde(default)]
    pub options_from: Option<OptionsFrom>,
    /// GitHub repository (owner/repo format) for `github_release_tag` parameter type.
    /// Required when `param_type` is `GithubReleaseTag`.
    #[serde(default)]
    pub github_repo: Option<String>,
}

// ─── Dynamic Options (Tier 1: API Fetch + Mapping) ───

/// Sort order for dynamically fetched options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum OptionsSortOrder {
    Asc,
    Desc,
}

/// Declarative definition for fetching dropdown options from a JSON API.
///
/// The backend proxies the request, navigates the JSON response using
/// `path`, extracts values via `value_key`/`label_key`, and applies
/// `sort` and `limit` before returning a canonical list of options.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct OptionsFrom {
    /// The URL to GET.  Supports `{{param}}` variable substitution from
    /// other already-filled parameter values.
    pub url: String,
    /// Dot-separated path to an array in the JSON response.  If omitted,
    /// the root of the response must be an array.
    #[serde(default)]
    pub path: Option<String>,
    /// If items in the array are objects, which key to use as the option
    /// **value**.  If omitted, items must be strings.
    #[serde(default)]
    pub value_key: Option<String>,
    /// If items in the array are objects, which key to use as the display
    /// **label**.  If omitted, uses `value_key` or the raw string.
    #[serde(default)]
    pub label_key: Option<String>,
    /// Sort the resulting options alphabetically.  Default: preserve API
    /// order.
    #[serde(default)]
    pub sort: Option<OptionsSortOrder>,
    /// Maximum number of options to return.  Applied after sorting.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub limit: Option<u32>,
    /// How long (in seconds) the frontend should cache the results before
    /// re-fetching.  Default: `0` (no cache).
    #[serde(default)]
    #[ts(type = "number | null")]
    pub cache_secs: Option<u32>,
}

/// A single option in the canonical response format returned by the
/// fetch-options endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct FetchedOption {
    pub value: String,
    pub label: String,
}

/// Response from `GET /api/templates/fetch-options`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct FetchOptionsResponse {
    pub options: Vec<FetchedOption>,
    pub cached: bool,
}

/// Query parameters for `GET /api/templates/fetch-options`.
#[derive(Debug, Clone, Deserialize)]
pub struct FetchOptionsQuery {
    pub url: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub value_key: Option<String>,
    #[serde(default)]
    pub label_key: Option<String>,
    #[serde(default)]
    pub sort: Option<OptionsSortOrder>,
    #[serde(default)]
    pub limit: Option<u32>,
    /// JSON-encoded map of `{ param_name: param_value }` for `{{param}}`
    /// substitution in the URL.
    #[serde(default)]
    pub params: Option<String>,
}

// ─── File Operations (for EditFile step) ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileOperation {
    Overwrite {
        content: String,
    },
    Append {
        content: String,
    },
    Prepend {
        content: String,
    },
    FindReplace {
        find: String,
        replace: String,
        #[serde(default = "default_true")]
        all: bool,
    },
    RegexReplace {
        pattern: String,
        replace: String,
        #[serde(default = "default_true")]
        all: bool,
    },
    InsertAfter {
        pattern: String,
        content: String,
    },
    InsertBefore {
        pattern: String,
        content: String,
    },
    ReplaceLine {
        pattern: String,
        content: String,
        #[serde(default)]
        all: bool,
    },
}

// ─── Archive Formats ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    Zip,
    TarGz,
    TarBz2,
    TarXz,
    Tar,
    Auto,
}

// ─── Step Conditions ───

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct StepCondition {
    #[serde(default)]
    pub path_exists: Option<String>,
    #[serde(default)]
    pub path_not_exists: Option<String>,
}

// ─── Process Configuration (accumulated by start pipeline steps) ───

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ProcessConfig {
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub stop_command: Option<String>,
    #[serde(default)]
    pub stop_signal: Option<super::server::StopSignal>,
}

// ─── Pipeline Step Actions ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepAction {
    Download {
        url: String,
        destination: String,
        #[serde(default)]
        filename: Option<String>,
        #[serde(default)]
        executable: bool,
    },
    Extract {
        source: String,
        #[serde(default)]
        destination: Option<String>,
        #[serde(default = "default_archive_format")]
        format: ArchiveFormat,
    },
    #[serde(rename = "move")]
    MoveAction {
        source: String,
        destination: String,
    },
    Copy {
        source: String,
        destination: String,
        #[serde(default = "default_true")]
        recursive: bool,
    },
    Delete {
        path: String,
        #[serde(default)]
        recursive: bool,
    },
    CreateDir {
        path: String,
    },
    RunCommand {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        working_dir: Option<String>,
        #[serde(default)]
        env: std::collections::HashMap<String, String>,
    },
    WriteFile {
        path: String,
        content: String,
    },
    EditFile {
        path: String,
        operation: FileOperation,
    },
    SetPermissions {
        path: String,
        mode: String,
    },
    Glob {
        pattern: String,
        destination: String,
    },
    /// Set environment variables for the server process.
    /// Variables are merged (later steps override earlier ones).
    /// Only meaningful in a `start` pipeline.
    SetEnv {
        #[serde(default)]
        variables: HashMap<String, String>,
    },
    /// Set the working directory for the server process.
    /// Only meaningful in a `start` pipeline.
    SetWorkingDir {
        path: String,
    },
    /// Set the stop command sent to the server's stdin on graceful stop.
    /// Only meaningful in a `start` pipeline.
    SetStopCommand {
        command: String,
    },
    /// Set the signal sent to the process group on graceful stop when no
    /// stop command is configured.  Accepts `"sigterm"` or `"sigint"`.
    /// Only meaningful in a `start` pipeline.
    SetStopSignal {
        signal: super::server::StopSignal,
    },
    /// Send text to the running server's stdin.
    /// Primarily useful in `stop_steps` (e.g. sending "stop" to a Minecraft server).
    SendInput {
        text: String,
    },
    /// Send a signal to the server's process group.
    /// Useful in `stop_steps` to send SIGINT/SIGTERM/SIGKILL at a specific point.
    SendSignal {
        signal: super::server::StopSignal,
    },
    /// Pause execution for the specified number of seconds.
    /// Useful in `stop_steps` to wait between sending a command and a signal.
    Sleep {
        seconds: u32,
    },
    /// Wait until a specific text pattern appears in the server's console
    /// output (stdout or stderr).  Useful in `stop_steps` to wait for
    /// confirmation messages (e.g. "Saving..." or "Closing Server") before
    /// proceeding to the next step.
    WaitForOutput {
        /// The text to search for in console output (case-insensitive substring match).
        pattern: String,
        /// Maximum seconds to wait before giving up and continuing.
        timeout_secs: u32,
    },
    /// Fetch a JSON API endpoint, navigate the response, extract a value,
    /// and store it as a pipeline variable that subsequent steps can
    /// reference via `${variable}`.
    ///
    /// This is useful when a download URL depends on dynamic data from an
    /// API (e.g. resolving the latest build number from PaperMC).
    ResolveVariable {
        /// The URL to GET.  Supports `${param}` variable substitution.
        url: String,
        /// Dot-separated path to navigate the JSON response (e.g.
        /// `"builds"` or `"data.latest"`).  If omitted the root is used.
        #[serde(default)]
        path: Option<String>,
        /// When the resolved value is an array, which element to pick.
        /// Defaults to `Last`.
        #[serde(default = "default_resolve_pick")]
        pick: VersionPick,
        /// If the picked element is an object, which key to extract as the
        /// value.  If omitted, the element must be a string or number.
        #[serde(default)]
        value_key: Option<String>,
        /// The variable name to store the result in.  Subsequent steps can
        /// reference it as `${variable}`.
        variable: String,
    },
    /// Download an asset from a GitHub release.
    ///
    /// This operation takes a reference to a `github_release_tag` parameter,
    /// resolves the release, finds an asset matching the given pattern,
    /// and downloads it to the specified destination.
    DownloadGithubReleaseAsset {
        /// The name of a `github_release_tag` parameter to use.
        /// This must reference an actual parameter of type `GithubReleaseTag`
        /// in the template's parameter list.
        tag_param: String,
        /// Asset filename matcher. Can be an exact filename or a regex pattern.
        /// Regex patterns must be wrapped in forward slashes (e.g. `/pattern/`).
        asset_matcher: String,
        /// Destination directory to save the downloaded asset.
        destination: String,
        /// Optional filename to save as. If not specified, uses the original asset name.
        #[serde(default)]
        filename: Option<String>,
        /// Whether to mark the downloaded file as executable (Unix only).
        #[serde(default)]
        executable: bool,
    },
    /// Use SteamCMD to install a Steam application into the server directory.
    ///
    /// Requires `steamcmd` to be available on PATH. The app ID is taken from
    /// the server's `steam_app_id` config field unless overridden here.
    /// SteamCMD is invoked with `+force_install_dir` pointing to the server
    /// directory so all files land in the right place.
    SteamCmdInstall {
        /// Override the Steam app ID. If `None`, uses the server config's
        /// `steam_app_id`.
        #[serde(default)]
        #[ts(type = "number | null")]
        app_id: Option<u32>,
        /// Whether to log in anonymously.  Most dedicated servers allow
        /// anonymous login.  Defaults to `true`.
        #[serde(default = "default_true")]
        anonymous: bool,
        /// Extra arguments appended to the SteamCMD command line before
        /// `+quit`.  Useful for beta branch selection, etc.
        #[serde(default)]
        extra_args: Vec<String>,
    },
    /// Use SteamCMD to update an already-installed Steam application.
    ///
    /// Functionally identical to `SteamCmdInstall` but semantically distinct
    /// so it can appear in `update_steps` while `SteamCmdInstall` appears in
    /// `install_steps`.  SteamCMD's `+app_update` handles both cases.
    SteamCmdUpdate {
        /// Override the Steam app ID. If `None`, uses the server config's
        /// `steam_app_id`.
        #[serde(default)]
        #[ts(type = "number | null")]
        app_id: Option<u32>,
        /// Whether to log in anonymously.  Defaults to `true`.
        #[serde(default = "default_true")]
        anonymous: bool,
        /// Extra arguments appended to the SteamCMD command line before
        /// `+quit`.
        #[serde(default)]
        extra_args: Vec<String>,
    },
}

fn default_resolve_pick() -> VersionPick {
    VersionPick::Last
}

fn default_archive_format() -> ArchiveFormat {
    ArchiveFormat::Auto
}

fn default_true() -> bool {
    true
}

// ─── Pipeline Step ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct PipelineStep {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub action: StepAction,
    #[serde(default)]
    pub condition: Option<StepCondition>,
    #[serde(default)]
    pub continue_on_error: bool,
}

// ─── Phase Kind ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum PhaseKind {
    Start,
    Install,
    Update,
    Uninstall,
}

// ─── Phase / Step Status ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

// ─── Step Progress ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct StepProgress {
    pub step_index: u32,
    pub step_name: String,
    pub status: PhaseStatus,
    pub message: Option<String>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ─── Phase Progress ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct PhaseProgress {
    pub server_id: uuid::Uuid,
    pub phase: PhaseKind,
    pub status: PhaseStatus,
    pub steps: Vec<StepProgress>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}
