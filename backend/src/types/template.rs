use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::server::ServerConfig;

// ─── Stored Template ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ServerTemplate {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub config: ServerConfig,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// When `true`, this template was shipped with AnyServer and cannot be
    /// modified or deleted by users.
    #[serde(default)]
    pub is_builtin: bool,
    /// Computed at serialization time: `true` when the template's config
    /// uses SteamCMD step actions or has a `steam_app_id` set.
    /// The frontend uses this to gray-out templates when SteamCMD is
    /// not available on the host.
    #[serde(default)]
    pub requires_steamcmd: bool,
    /// Computed at serialization time: `true` when the template's config
    /// has parameters of type `CurseForgeFileVersion` or uses
    /// `DownloadCurseForgeFile` step actions.  The frontend uses this to
    /// warn users when the CurseForge API key hasn't been configured.
    #[serde(default)]
    pub requires_curseforge: bool,
    /// Computed at serialization time: `true` when the template's config
    /// has parameters of type `GithubReleaseTag` or uses
    /// `DownloadGithubReleaseAsset` step actions.
    #[serde(default)]
    pub requires_github: bool,
}

impl ServerTemplate {
    /// Recompute all `requires_*` flags from the config contents.
    pub fn with_integration_flags(mut self) -> Self {
        self.requires_steamcmd = crate::utils::steamcmd::config_requires_steamcmd(&self.config);
        self.requires_curseforge = config_requires_curseforge(&self.config);
        self.requires_github = config_requires_github(&self.config);
        self
    }

    /// Legacy alias — kept so existing call-sites compile, but prefer
    /// [`Self::with_integration_flags`] for new code.
    pub fn with_steamcmd_flag(self) -> Self {
        self.with_integration_flags()
    }
}

/// Returns `true` when the config uses any CurseForge-dependent features:
/// - A parameter of type `CurseForgeFileVersion`
/// - A `DownloadCurseForgeFile` step action
/// - An update-check provider of type `CurseForge`
pub fn config_requires_curseforge(config: &ServerConfig) -> bool {
    // Check parameters
    for param in &config.parameters {
        if matches!(
            param.param_type,
            crate::types::pipeline::ConfigParameterType::CurseForgeFileVersion
        ) {
            return true;
        }
    }

    // Check update-check provider
    if let Some(ref uc) = config.update_check {
        if matches!(
            uc.provider,
            crate::types::server::UpdateCheckProvider::CurseForge { .. }
        ) {
            return true;
        }
    }

    // Check all pipeline steps
    let all_steps = config
        .install_steps
        .iter()
        .chain(config.update_steps.iter())
        .chain(config.uninstall_steps.iter())
        .chain(config.start_steps.iter())
        .chain(config.stop_steps.iter());

    for step in all_steps {
        if matches!(
            &step.action,
            crate::types::pipeline::StepAction::DownloadCurseForgeFile { .. }
        ) {
            return true;
        }
    }

    false
}

/// Returns `true` when the config uses any GitHub-dependent features:
/// - A parameter of type `GithubReleaseTag`
/// - A `DownloadGithubReleaseAsset` step action
pub fn config_requires_github(config: &ServerConfig) -> bool {
    // Check parameters
    for param in &config.parameters {
        if matches!(
            param.param_type,
            crate::types::pipeline::ConfigParameterType::GithubReleaseTag
        ) {
            return true;
        }
    }

    // Check all pipeline steps
    let all_steps = config
        .install_steps
        .iter()
        .chain(config.update_steps.iter())
        .chain(config.uninstall_steps.iter())
        .chain(config.start_steps.iter())
        .chain(config.stop_steps.iter());

    for step in all_steps {
        if matches!(
            &step.action,
            crate::types::pipeline::StepAction::DownloadGithubReleaseAsset { .. }
        ) {
            return true;
        }
    }

    false
}

// ─── API Request / Response Types ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CreateTemplateRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub config: ServerConfig,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UpdateTemplateRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub config: ServerConfig,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct TemplateListResponse {
    pub templates: Vec<ServerTemplate>,
    /// Whether SteamCMD is available on the host.  Sent alongside the
    /// template list so the frontend can gray-out templates that need it.
    #[serde(default)]
    pub steamcmd_available: bool,
    /// Whether a CurseForge API key has been configured by an admin.
    #[serde(default)]
    pub curseforge_available: bool,
    /// Whether a GitHub token has been configured by an admin.
    /// Note: GitHub works without a token for public repos (lower rate
    /// limits), so this being `false` doesn't fully block GitHub features.
    #[serde(default)]
    pub github_available: bool,
}
