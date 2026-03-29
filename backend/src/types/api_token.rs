//! Types for long-lived API token management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

// ─── Scope Model ─────────────────────────────────────────────

/// Controls what an API token is allowed to do.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ApiTokenScope {
    /// `"full"` — equivalent to the user's normal permissions.
    /// `"read_only"` — can only perform GET requests and view WebSocket streams.
    #[serde(default = "default_access")]
    pub access: String,

    /// `None` — all servers the user has access to.
    /// `Some([...])` — only the listed servers (intersected with user permissions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_ids: Option<Vec<Uuid>>,
}

fn default_access() -> String {
    "full".to_string()
}

impl Default for ApiTokenScope {
    fn default() -> Self {
        Self {
            access: default_access(),
            server_ids: None,
        }
    }
}

impl ApiTokenScope {
    pub fn is_read_only(&self) -> bool {
        self.access == "read_only"
    }
}

// ─── Stored Row ──────────────────────────────────────────────

/// Database representation of an API token (never contains the raw secret).
#[derive(Debug, Clone)]
pub struct ApiToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub scope: ApiTokenScope,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked: bool,
}

impl ApiToken {
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| exp < Utc::now())
    }

    pub fn is_usable(&self) -> bool {
        !self.revoked && !self.is_expired()
    }
}

// ─── API Request / Response Types ────────────────────────────

/// POST /api/auth/api-tokens — create a new token.
#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CreateApiTokenRequest {
    /// Human-readable label (e.g. "GitHub Actions deploy").
    pub name: String,

    /// Optional expiry.  `None` means never-expires.
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,

    /// Permission scope.  Defaults to full access if omitted.
    #[serde(default)]
    pub scope: ApiTokenScope,
}

/// Response returned exactly once when a token is created.
/// The `token` field contains the raw secret — it is never stored or
/// retrievable again after this response.
#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CreateApiTokenResponse {
    pub id: Uuid,
    pub name: String,
    /// The raw API token.  Copy this now — it will not be shown again.
    pub token: String,
    pub scope: ApiTokenScope,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// A single token as returned by the list endpoint — never includes the
/// raw secret.
#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ApiTokenInfo {
    pub id: Uuid,
    pub name: String,
    pub scope: ApiTokenScope,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked: bool,
}

impl From<&ApiToken> for ApiTokenInfo {
    fn from(t: &ApiToken) -> Self {
        Self {
            id: t.id,
            name: t.name.clone(),
            scope: t.scope.clone(),
            created_at: t.created_at,
            expires_at: t.expires_at,
            last_used_at: t.last_used_at,
            revoked: t.revoked,
        }
    }
}

/// GET /api/auth/api-tokens — list the user's tokens.
#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ListApiTokensResponse {
    pub tokens: Vec<ApiTokenInfo>,
}

/// DELETE /api/auth/api-tokens/:id — revoke response.
#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct RevokeApiTokenResponse {
    pub revoked: bool,
}
