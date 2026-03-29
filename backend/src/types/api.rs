use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::pipeline::{PhaseKind, PhaseProgress, PhaseStatus, PipelineStep};
use super::server::ServerWithStatus;

// ─── Pagination Types ───

#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}

// Concrete type for TypeScript export
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct PaginatedServerListResponse {
    pub servers: Vec<ServerWithStatus>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}

#[derive(Debug, Deserialize)]
pub struct ListServersParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
    pub search: Option<String>,
    pub status: Option<String>,
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default = "default_order")]
    pub order: String,
}

fn default_page() -> u32 {
    1
}

fn default_per_page() -> u32 {
    25
}

fn default_sort() -> String {
    "name".to_string()
}

fn default_order() -> String {
    "asc".to_string()
}

// ─── Generic API Error Envelope ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ApiError {
    pub error: String,
    pub details: Option<String>,
}

// ─── Pipeline API Request / Response Types ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct RunPhaseRequest {
    #[serde(default)]
    pub steps_override: Option<Vec<PipelineStep>>,
    #[serde(default)]
    pub parameter_overrides: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct RunPhaseResponse {
    pub server_id: Uuid,
    pub phase: PhaseKind,
    pub status: PhaseStatus,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct PhaseStatusResponse {
    pub server_id: Uuid,
    pub progress: Option<PhaseProgress>,
    pub installed: bool,
    pub installed_at: Option<DateTime<Utc>>,
    pub updated_via_pipeline_at: Option<DateTime<Utc>>,
}

// ─── Typed Action Responses ───
//
// These replace the ad-hoc `Json<serde_json::Value>` responses that were
// previously constructed via `serde_json::json!({...})`.  Having proper
// structs means the Rust→TypeScript type bridge (ts-rs) can generate
// matching frontend types, and any field rename is caught at compile time.

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct DeleteServerResponse {
    pub deleted: bool,
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ResetServerResponse {
    pub reset: bool,
    pub id: String,
    pub killed_processes: usize,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct KillProcessResult {
    pub pid: u32,
    pub command: String,
    pub success: bool,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct KillDirectoryProcessesResponse {
    pub killed: usize,
    pub failed: usize,
    pub processes: Vec<KillProcessResult>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SendCommandResponse {
    pub sent: bool,
    pub command: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SendSignalResponse {
    pub sent: bool,
    pub signal: String,
    pub pid: u32,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CancelStopResponse {
    pub cancelled: bool,
    pub server_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ChangePasswordResponse {
    pub changed: bool,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CancelPhaseResponse {
    pub cancelled: bool,
    pub server_id: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct WriteFileResponse {
    pub written: bool,
    pub path: String,
    pub size: usize,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CreateDirResponse {
    pub created: bool,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct DeletePathResponse {
    pub deleted: bool,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ChmodResponse {
    pub path: String,
    pub mode: String,
    pub mode_display: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct RemovePermissionResponse {
    pub removed: bool,
    pub user_id: String,
    pub server_id: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct DeleteTemplateResponse {
    pub deleted: bool,
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct DeleteUserResponse {
    pub deleted: bool,
    pub id: String,
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct MarkInstalledResponse {
    pub server_id: Uuid,
    pub installed: bool,
    pub installed_at: Option<DateTime<Utc>>,
}
