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
}

impl ServerTemplate {
    /// Recompute `requires_steamcmd` from the config contents.
    pub fn with_steamcmd_flag(mut self) -> Self {
        self.requires_steamcmd = crate::utils::steamcmd::config_requires_steamcmd(&self.config);
        self
    }
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
}
