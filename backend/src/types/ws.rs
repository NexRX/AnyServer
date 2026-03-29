use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::pipeline::{PhaseKind, PhaseProgress};
use super::server::ServerRuntime;

// ─── WebSocket Messages ───

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(tag = "type", content = "data")]
pub enum WsMessage {
    Log(LogLine),
    StatusChange(ServerRuntime),
    PhaseProgress(PhaseProgress),
    PhaseLog(PhaseLogLine),
    StopProgress(StopProgress),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct PhaseLogLine {
    pub timestamp: DateTime<Utc>,
    pub phase: PhaseKind,
    pub step_index: u32,
    pub step_name: String,
    pub line: String,
    pub stream: LogStream,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct LogLine {
    /// Monotonically increasing sequence number within a single
    /// `ProcessHandle`.  Clients use this to deduplicate log lines
    /// after a WebSocket reconnection and to detect gaps in the stream.
    pub seq: u32,
    pub timestamp: DateTime<Utc>,
    pub line: String,
    pub stream: LogStream,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
}

// ─── Shutdown Progress ───

/// Which phase of the graceful shutdown the server is currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum StopPhase {
    /// A configured `stop_command` is being sent via stdin.
    SendingStopCommand,
    /// The server has `stop_steps` defined and they are being executed.
    RunningStopSteps,
    /// A signal (SIGTERM/SIGINT) was sent; waiting for the process to exit.
    WaitingForExit,
    /// The grace period expired and SIGKILL has been sent.
    SendingSigkill,
    /// The shutdown was cancelled by the user; server continues running.
    Cancelled,
}

/// Info about the currently executing stop step, sent only during
/// the `RunningStopSteps` phase so the UI can show per-step progress.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct StopStepInfo {
    /// 0-based index of the currently executing step.
    pub index: u32,
    /// Total number of stop steps configured.
    pub total: u32,
    /// Human-readable name of the current step.
    pub name: String,
    /// For time-bounded steps (`Sleep`, `WaitForOutput`) this is the
    /// step's own timeout so the UI can display per-step context
    /// (e.g. "up to 30 s").  `None` for instant actions like `SendInput`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_timeout_secs: Option<u32>,
}

/// Real-time progress of a server shutdown, broadcast over WebSocket so the
/// UI can show a countdown timer and the current phase.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct StopProgress {
    pub server_id: Uuid,
    pub phase: StopPhase,
    /// Seconds elapsed since the stop was initiated.
    pub elapsed_secs: f64,
    /// Total estimated seconds from stop initiation to SIGKILL.
    ///
    /// During `RunningStopSteps` this includes the estimated duration of all
    /// stop steps **plus** the configured `stop_timeout_secs` grace period.
    /// When transitioning to `WaitingForExit` the value is recalculated as
    /// `actual_elapsed + stop_timeout_secs` so the countdown stays accurate
    /// even when steps finish earlier or later than estimated.
    pub timeout_secs: f64,
    /// The server's configured `stop_timeout_secs` grace period (seconds
    /// between the end of stop steps / stop command and the SIGKILL).
    ///
    /// Sent on every message so the frontend can display a clean countdown
    /// during `WaitingForExit` without relying on the estimated total.
    pub grace_secs: u32,
    /// Present only during `RunningStopSteps` — identifies which step is
    /// currently executing and how many steps there are in total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_info: Option<StopStepInfo>,
}
