use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::auth::{GlobalCapability, PermissionLevel};

// ─── Invite Code ───

/// A one-time redeemable invite code created by an admin.
/// Contains the role and per-server permissions to grant on redemption.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct InviteCode {
    pub id: Uuid,
    /// The 6-digit numeric code
    pub code: String,
    /// Admin user ID who created this invite
    pub created_by: Uuid,
    /// Role to assign when redeemed
    pub assigned_role: super::auth::Role,
    /// Per-server permissions to grant on redemption
    pub assigned_permissions: Vec<InvitePermissionGrant>,
    /// Global capabilities to grant on redemption
    #[serde(default)]
    pub assigned_capabilities: Vec<GlobalCapability>,
    /// When this code expires
    pub expires_at: DateTime<Utc>,
    /// User who redeemed this code (None if not yet redeemed)
    pub redeemed_by: Option<Uuid>,
    /// When this code was redeemed
    pub redeemed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Optional admin-facing label/note
    pub label: Option<String>,
}

/// A single server permission grant included in an invite code.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct InvitePermissionGrant {
    pub server_id: Uuid,
    pub level: PermissionLevel,
}

/// Public view of an invite code (hides sensitive internal fields for list views).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct InviteCodePublic {
    pub id: Uuid,
    pub code: String,
    pub created_by: Uuid,
    pub created_by_username: Option<String>,
    pub assigned_role: super::auth::Role,
    pub assigned_permissions: Vec<InvitePermissionGrant>,
    #[serde(default)]
    pub assigned_capabilities: Vec<GlobalCapability>,
    pub expires_at: DateTime<Utc>,
    pub redeemed_by: Option<Uuid>,
    pub redeemed_by_username: Option<String>,
    pub redeemed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub label: Option<String>,
    /// Whether the code is still valid (not expired, not redeemed)
    pub is_active: bool,
}

// ─── Expiry duration options ───

/// Allowed expiry durations for invite codes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum InviteExpiry {
    /// 30 minutes
    ThirtyMinutes,
    /// 1 hour
    OneHour,
    /// 1 day
    OneDay,
    /// 3 days
    ThreeDays,
    /// 7 days (maximum)
    SevenDays,
}

impl InviteExpiry {
    /// Convert to a chrono Duration.
    pub fn to_duration(&self) -> chrono::Duration {
        match self {
            InviteExpiry::ThirtyMinutes => chrono::Duration::minutes(30),
            InviteExpiry::OneHour => chrono::Duration::hours(1),
            InviteExpiry::OneDay => chrono::Duration::days(1),
            InviteExpiry::ThreeDays => chrono::Duration::days(3),
            InviteExpiry::SevenDays => chrono::Duration::days(7),
        }
    }
}

// ─── Request / Response types ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CreateInviteCodeRequest {
    /// How long before the code expires
    pub expiry: InviteExpiry,
    /// Role to assign when the code is redeemed
    pub assigned_role: super::auth::Role,
    /// Per-server permissions to grant on redemption
    #[serde(default)]
    pub assigned_permissions: Vec<InvitePermissionGrant>,
    /// Global capabilities to grant on redemption
    #[serde(default)]
    pub assigned_capabilities: Vec<GlobalCapability>,
    /// Optional admin-facing label/note
    pub label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CreateInviteCodeResponse {
    pub invite: InviteCodePublic,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct InviteCodeListResponse {
    pub invites: Vec<InviteCodePublic>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UpdateInvitePermissionsRequest {
    /// Updated role assignment
    pub assigned_role: super::auth::Role,
    /// Updated per-server permissions
    pub assigned_permissions: Vec<InvitePermissionGrant>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct RedeemInviteCodeRequest {
    /// The 6-digit code to redeem
    pub code: String,
    /// Username for the new account
    pub username: String,
    /// Password for the new account
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct RedeemInviteCodeResponse {
    /// The newly created user
    pub token: String,
    pub user: super::auth::UserPublic,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct DeleteInviteCodeResponse {
    pub deleted: bool,
    pub id: String,
}

// ─── User permission management (admin) ───

/// A user's permissions across all servers, for the admin management view.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UserPermissionSummary {
    pub user_id: Uuid,
    pub username: String,
    pub role: super::auth::Role,
    pub server_permissions: Vec<UserServerPermission>,
}

/// A single server permission entry in the user permission summary.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UserServerPermission {
    pub server_id: Uuid,
    pub server_name: String,
    pub level: PermissionLevel,
    /// Whether this is an implicit permission (owner/global admin) or explicit
    pub is_implicit: bool,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UserPermissionListResponse {
    pub users: Vec<UserPermissionSummary>,
}
