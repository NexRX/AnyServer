use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ─── Helpers for Unix file permissions ───

/// Convert a Unix mode (e.g. 0o755) to an octal string like "755".
pub fn mode_to_octal_string(mode: u32) -> String {
    format!("{:o}", mode & 0o7777)
}

/// Convert a Unix mode to a human-readable string like "rwxr-xr-x".
pub fn mode_to_rwx_string(mode: u32) -> String {
    let flags = [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ];
    flags
        .iter()
        .map(|(bit, ch)| if mode & bit != 0 { *ch } else { '-' })
        .collect()
}

/// Parse an octal mode string (e.g. "755") into a u32.
/// Returns None if the string is not a valid octal number or is out of range.
pub fn parse_octal_mode(s: &str) -> Option<u32> {
    u32::from_str_radix(s, 8).ok().filter(|&m| m <= 0o7777)
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum FileEntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: FileEntryKind,
    #[ts(type = "number")]
    pub size: u64,
    pub modified: Option<DateTime<Utc>>,
    /// Octal permission string (e.g. "755"). Only present on Unix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct FileListResponse {
    pub path: String,
    pub entries: Vec<FileEntry>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct FileContentResponse {
    pub path: String,
    pub content: String,
    #[ts(type = "number")]
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct WriteFileRequest {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct CreateDirRequest {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct DeleteRequest {
    pub path: String,
}

// ─── Chmod ───

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ChmodRequest {
    pub path: String,
    /// Octal mode string, e.g. "755", "644".
    pub mode: String,
}

/// Detailed permission info for a single file or directory.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct FilePermissionsResponse {
    pub path: String,
    /// Octal mode string, e.g. "755".
    pub mode: String,
    /// Human-readable string, e.g. "rwxr-xr-x".
    pub mode_display: String,
    /// Whether the file is a directory.
    pub is_directory: bool,
    /// Numeric UID of the file owner.
    pub uid: u32,
    /// Numeric GID of the file group.
    pub gid: u32,
    /// Username of the file owner (if resolvable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Group name (if resolvable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}
