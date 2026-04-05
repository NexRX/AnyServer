use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

// ─── SMTP Configuration ───

/// SMTP server configuration for sending email alerts.
/// Stored in the sled `settings` tree under the key `"smtp"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    /// SMTP server hostname (e.g. "smtp.gmail.com").
    pub host: String,
    /// SMTP server port (e.g. 587 for STARTTLS, 465 for implicit TLS).
    pub port: u16,
    /// Use TLS (STARTTLS on port 587, implicit TLS on port 465).
    pub tls: bool,
    /// SMTP username (often the email address itself).
    pub username: String,
    /// SMTP password — stored in sled, never returned via API.
    pub password: String,
    /// "From" address used in outgoing emails.
    pub from_address: String,
}

/// The SMTP config as returned by the API — password is always redacted.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SmtpConfigPublic {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub username: String,
    /// Always `"********"` — the real password is never sent to the frontend.
    pub password_set: bool,
    pub from_address: String,
}

impl From<&SmtpConfig> for SmtpConfigPublic {
    fn from(c: &SmtpConfig) -> Self {
        Self {
            host: c.host.clone(),
            port: c.port,
            tls: c.tls,
            username: c.username.clone(),
            password_set: !c.password.is_empty(),
            from_address: c.from_address.clone(),
        }
    }
}

/// Request to save SMTP configuration.
/// If `password` is `None`, the existing password is kept (allows updating
/// other fields without re-entering the password).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SaveSmtpConfigRequest {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub username: String,
    /// `None` means "keep existing password". Empty string means "clear password".
    #[serde(default)]
    pub password: Option<String>,
    pub from_address: String,
}

/// Request body for sending a test email.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct TestEmailRequest {
    pub recipient: String,
}

/// Response from sending a test email.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct TestEmailResponse {
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
}

/// Response from deleting SMTP configuration.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct DeleteSmtpConfigResponse {
    pub deleted: bool,
}

// ─── Alert Configuration ───

/// Global alert configuration — thresholds and recipients.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct AlertConfig {
    /// Master switch — when false, no alerts are sent regardless of other settings.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Email addresses to send alerts to.
    #[serde(default)]
    pub recipients: Vec<String>,

    /// Base URL for the AnyServer instance, used to generate direct links
    /// in email bodies (e.g. `https://my.server.com:3001`).
    #[serde(default)]
    pub base_url: Option<String>,

    /// Cooldown in seconds between repeated alerts of the same type for the
    /// same server.  Prevents alert storms from crash loops.
    #[serde(default = "default_cooldown_secs")]
    #[ts(type = "number")]
    pub cooldown_secs: u64,

    /// Individual trigger configurations.
    #[serde(default)]
    pub triggers: AlertTriggers,
}

fn default_true() -> bool {
    true
}

fn default_cooldown_secs() -> u64 {
    300 // 5 minutes
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            recipients: Vec::new(),
            base_url: None,
            cooldown_secs: default_cooldown_secs(),
            triggers: AlertTriggers::default(),
        }
    }
}

/// Individual alert trigger toggles and thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct AlertTriggers {
    /// Alert when a server process exits unexpectedly (crashes).
    #[serde(default = "default_true")]
    pub server_crashed: bool,

    /// Alert when auto-restart attempts are exhausted.
    #[serde(default = "default_true")]
    pub restart_exhausted: bool,

    /// Alert when a server has been down for longer than `down_threshold_mins`.
    #[serde(default)]
    pub server_down: bool,

    /// Minutes a server must be continuously down before an alert fires.
    #[serde(default = "default_down_threshold")]
    #[ts(type = "number")]
    pub down_threshold_mins: u64,

    /// Alert when server memory usage exceeds threshold (percentage of system total).
    #[serde(default)]
    pub high_memory: bool,

    /// Memory usage percentage threshold (0–100).
    #[serde(default = "default_memory_threshold")]
    pub memory_threshold_percent: f64,

    /// Alert when server CPU usage exceeds threshold.
    #[serde(default)]
    pub high_cpu: bool,

    /// CPU usage percentage threshold (0–100+, can exceed 100 on multi-core).
    #[serde(default = "default_cpu_threshold")]
    pub cpu_threshold_percent: f64,

    /// Alert when disk space is low on the data partition.
    #[serde(default)]
    pub low_disk: bool,

    /// Disk free-space threshold in megabytes.
    #[serde(default = "default_disk_threshold")]
    #[ts(type = "number")]
    pub disk_threshold_mb: u64,
}

fn default_down_threshold() -> u64 {
    10
}

fn default_memory_threshold() -> f64 {
    90.0
}

fn default_cpu_threshold() -> f64 {
    95.0
}

fn default_disk_threshold() -> u64 {
    1024 // 1 GB
}

impl Default for AlertTriggers {
    fn default() -> Self {
        Self {
            server_crashed: true,
            restart_exhausted: true,
            server_down: false,
            down_threshold_mins: default_down_threshold(),
            high_memory: false,
            memory_threshold_percent: default_memory_threshold(),
            high_cpu: false,
            cpu_threshold_percent: default_cpu_threshold(),
            low_disk: false,
            disk_threshold_mb: default_disk_threshold(),
        }
    }
}

/// Request to update alert configuration.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct SaveAlertConfigRequest {
    pub enabled: bool,
    pub recipients: Vec<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[ts(type = "number")]
    pub cooldown_secs: u64,
    pub triggers: AlertTriggers,
}

// ─── Per-Server Alert Opt-Out ───

/// Per-server alert preferences.  By default all servers inherit the
/// global alert configuration.  Setting `muted` to `true` suppresses
/// all alerts for that server.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ServerAlertConfig {
    pub server_id: Uuid,
    /// When `true`, no alerts are sent for this server.
    pub muted: bool,
}

/// Request to update per-server alert settings.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct UpdateServerAlertRequest {
    pub muted: bool,
}

// ─── Alert Event Types ───

/// The kind of event that triggered an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertEventKind {
    ServerCrashed,
    RestartExhausted,
    ServerDown,
    HighMemory,
    HighCpu,
    LowDisk,
}

impl AlertEventKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            AlertEventKind::ServerCrashed => "Server Crashed",
            AlertEventKind::RestartExhausted => "Restart Attempts Exhausted",
            AlertEventKind::ServerDown => "Server Down",
            AlertEventKind::HighMemory => "High Memory Usage",
            AlertEventKind::HighCpu => "High CPU Usage",
            AlertEventKind::LowDisk => "Low Disk Space",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            AlertEventKind::ServerCrashed => "💥",
            AlertEventKind::RestartExhausted => "🔄",
            AlertEventKind::ServerDown => "⬇️",
            AlertEventKind::HighMemory => "🧠",
            AlertEventKind::HighCpu => "🔥",
            AlertEventKind::LowDisk => "💾",
        }
    }
}

/// An alert event ready to be dispatched via email.
#[derive(Debug, Clone)]
pub struct AlertEvent {
    pub kind: AlertEventKind,
    pub server_id: Uuid,
    pub server_name: String,
    pub timestamp: DateTime<Utc>,
    /// Human-readable description of what happened.
    pub message: String,
}
