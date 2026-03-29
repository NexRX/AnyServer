use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

// ─── Global Roles ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    User,
}

// ─── Global Capabilities ───

/// Feature-level permissions for non-admin users.
/// Admin users implicitly have ALL capabilities regardless of this field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum GlobalCapability {
    /// Can create new servers
    CreateServers,
    /// Can create, edit, and delete templates
    ManageTemplates,
    /// Can view the system health page
    ViewSystemHealth,
}

// ─── Per-server permission levels ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    Viewer = 0,
    Operator = 1,
    Manager = 2,
    Admin = 3,
    Owner = 4,
}

impl PermissionLevel {
    pub fn can_view(self) -> bool {
        self >= PermissionLevel::Viewer
    }
    pub fn can_operate(self) -> bool {
        self >= PermissionLevel::Operator
    }
    pub fn can_manage_files(self) -> bool {
        self >= PermissionLevel::Manager
    }
    pub fn can_edit_config(self) -> bool {
        self >= PermissionLevel::Manager
    }
    pub fn can_delete(self) -> bool {
        self >= PermissionLevel::Admin
    }
    pub fn can_manage_permissions(self) -> bool {
        self >= PermissionLevel::Admin
    }
}

// ─── User ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct User {
    pub id: Uuid,
    pub username: String,
    #[ts(skip)]
    pub password_hash: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    #[ts(skip)]
    pub token_generation: i64,
    /// Feature-level capabilities for this user.
    /// Ignored for Admin users (they implicitly have all capabilities).
    #[serde(default)]
    pub global_capabilities: Vec<GlobalCapability>,
}

impl User {
    /// Returns `true` if the user has the given capability, either explicitly
    /// or implicitly (admins have all capabilities).
    pub fn has_capability(&self, cap: GlobalCapability) -> bool {
        self.role == Role::Admin || self.global_capabilities.contains(&cap)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UserPublic {
    pub id: Uuid,
    pub username: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    /// Feature-level capabilities. Empty for admins (they have all implicitly).
    #[serde(default)]
    pub global_capabilities: Vec<GlobalCapability>,
}

impl From<User> for UserPublic {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            username: u.username,
            role: u.role,
            created_at: u.created_at,
            global_capabilities: u.global_capabilities,
        }
    }
}

// ─── Capability management request/response types ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UpdateUserCapabilitiesRequest {
    pub global_capabilities: Vec<GlobalCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ServerPermission {
    pub user_id: Uuid,
    pub server_id: Uuid,
    pub level: PermissionLevel,
}

// ─── App-wide settings ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct AppSettings {
    pub setup_complete: bool,
    pub registration_enabled: bool,
    /// Whether to allow RunCommand pipeline steps to execute shell commands.
    /// Defaults to true for backward compatibility with existing installations,
    /// but new installations should default to false for security.
    #[serde(default = "default_allow_run_commands")]
    pub allow_run_commands: bool,
    /// Sandboxing mode for RunCommand steps: "auto", "off", or "strict".
    /// - "auto": Apply all available isolation layers, fall back gracefully if unavailable
    /// - "off": No sandboxing (not recommended)
    /// - "strict": Require Landlock or fail
    #[serde(default = "default_run_command_sandbox")]
    pub run_command_sandbox: String,
    /// Default timeout in seconds for RunCommand steps that don't specify their own.
    #[serde(default = "default_run_command_timeout")]
    pub run_command_default_timeout_secs: u32,
    /// Whether to use PID and mount namespaces for RunCommand steps.
    /// Provides additional process isolation when available (Linux only).
    #[serde(default = "default_run_command_use_namespaces")]
    pub run_command_use_namespaces: bool,
}

fn default_allow_run_commands() -> bool {
    true
}

fn default_run_command_sandbox() -> String {
    "auto".to_string()
}

fn default_run_command_timeout() -> u32 {
    300
}

fn default_run_command_use_namespaces() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            setup_complete: false,
            registration_enabled: false,
            // Default to false for new installations (security-first)
            allow_run_commands: false,
            run_command_sandbox: "auto".to_string(),
            run_command_default_timeout_secs: 300,
            run_command_use_namespaces: true,
        }
    }
}

// ─── Auth request / response types ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct AuthResponse {
    pub token: String,
    pub user: UserPublic,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct MeResponse {
    pub user: UserPublic,
    pub settings: AppSettings,
}

// ─── User management (admin) ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UserListResponse {
    pub users: Vec<UserPublic>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UpdateUserRoleRequest {
    pub role: Role,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UpdateSettingsRequest {
    pub registration_enabled: bool,
    pub allow_run_commands: bool,
    pub run_command_sandbox: String,
    pub run_command_default_timeout_secs: u32,
    pub run_command_use_namespaces: bool,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct RefreshResponse {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct LogoutEverywhereResponse {
    pub revoked_count: i64,
    pub token: String,
}

// ─── Session management ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SessionInfo {
    pub id: String,
    pub family_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub is_current: bool,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SessionListResponse {
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct RevokeSessionRequest {
    pub family_id: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct RevokeSessionResponse {
    pub revoked_count: i64,
}

// ─── WebSocket ticket authentication ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct WsTicketRequest {
    pub scope: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct WsTicketResponse {
    pub ticket: String,
}

// ─── Permission management ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ServerPermissionEntry {
    pub user: UserPublic,
    pub level: PermissionLevel,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ServerPermissionsResponse {
    pub permissions: Vec<ServerPermissionEntry>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SetPermissionRequest {
    pub user_id: Uuid,
    pub level: PermissionLevel,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct RemovePermissionRequest {
    pub user_id: Uuid,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct EffectivePermission {
    pub level: PermissionLevel,
    pub is_global_admin: bool,
}

// ─── Import types ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ImportUrlRequest {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ImportUrlResponse {
    pub config: super::server::ServerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct RemoteConfigEntry {
    pub name: String,
    pub download_url: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ImportFolderRequest {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ImportFolderResponse {
    pub configs: Vec<RemoteConfigEntry>,
}
