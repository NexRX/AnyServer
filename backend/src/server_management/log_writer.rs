//! Async batched log file writer with size-based rotation.
//!
//! Console output lines are sent over a bounded channel and flushed to
//! disk in batches.  When the log file exceeds the configured size limit
//! it is rotated (`console.log` → `console.log.1` → `console.log.2`).
//! At most 3 files are kept (current + 2 rotated).

use std::io::Write;
use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use crate::types::LogLine;

/// Capacity of the bounded channel between `push_log` and the writer task.
/// If the channel is full the caller drops the line rather than blocking.
const CHANNEL_CAPACITY: usize = 4096;

/// Maximum number of lines to collect in a single batch before flushing.
const MAX_BATCH_SIZE: usize = 200;

/// How long to wait for additional lines before flushing a partial batch.
const BATCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Number of rotated files to keep (in addition to the current one).
const MAX_ROTATED_FILES: u32 = 2;

/// Handle returned to callers so they can send log lines to the writer.
#[derive(Clone)]
pub struct LogFileSender {
    tx: mpsc::Sender<LogLine>,
}

impl LogFileSender {
    /// Non-blocking send — drops the line silently if the channel is full
    /// (back-pressure should never stall log ingestion from the child
    /// process).
    pub fn send(&self, line: &LogLine) {
        let _ = self.tx.try_send(line.clone());
    }
}

/// Spawn an async log writer task for the given server directory.
///
/// Returns a [`LogFileSender`] that can be cloned into `ProcessHandle`.
/// The task runs until the sender half is dropped (all clones).
///
/// `max_log_size_bytes` is the per-file size limit; when exceeded the
/// current file is rotated.
pub fn spawn_log_writer(server_dir: &Path, max_log_size_bytes: u64) -> LogFileSender {
    let (tx, rx) = mpsc::channel::<LogLine>(CHANNEL_CAPACITY);

    let logs_dir = server_dir.join("logs");
    let log_path = logs_dir.join("console.log");

    tokio::spawn(writer_task(rx, log_path, logs_dir, max_log_size_bytes));

    LogFileSender { tx }
}

/// The long-running writer task.  Receives lines from the channel,
/// batches them, and flushes to disk periodically.
async fn writer_task(
    mut rx: mpsc::Receiver<LogLine>,
    log_path: PathBuf,
    logs_dir: PathBuf,
    max_log_size_bytes: u64,
) {
    // Ensure the logs directory exists.
    if let Err(e) = std::fs::create_dir_all(&logs_dir) {
        tracing::warn!(
            "Could not create log directory {:?}: {} — disk logging disabled for this server",
            logs_dir,
            e
        );
        // Drain the channel so senders don't get stuck.
        drain_channel(&mut rx).await;
        return;
    }

    let mut file = match open_log_file(&log_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(
                "Could not open log file {:?}: {} — disk logging disabled for this server",
                log_path,
                e
            );
            drain_channel(&mut rx).await;
            return;
        }
    };

    let mut current_size: u64 = std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0);

    let mut batch: Vec<LogLine> = Vec::with_capacity(MAX_BATCH_SIZE);

    loop {
        // Wait for the first line (or channel close).
        let first = rx.recv().await;
        let Some(first_line) = first else {
            // Channel closed — flush and exit.
            break;
        };

        batch.push(first_line);

        // Try to collect more lines up to MAX_BATCH_SIZE within the timeout.
        let deadline = tokio::time::Instant::now() + BATCH_TIMEOUT;
        loop {
            if batch.len() >= MAX_BATCH_SIZE {
                break;
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Some(line)) => batch.push(line),
                Ok(None) => {
                    // Channel closed — flush remaining, rotate if needed, and exit.
                    flush_batch(&mut file, &batch, &mut current_size);
                    batch.clear();
                    if current_size >= max_log_size_bytes && max_log_size_bytes > 0 {
                        let _ = rotate_logs(&log_path);
                    }
                    return;
                }
                Err(_) => break, // timeout — flush what we have
            }
        }

        // Flush the batch to disk.
        flush_batch(&mut file, &batch, &mut current_size);
        batch.clear();

        // Rotate if necessary.
        if current_size >= max_log_size_bytes && max_log_size_bytes > 0 {
            if let Err(e) = rotate_logs(&log_path) {
                tracing::warn!("Log rotation failed for {:?}: {}", log_path, e);
            }
            // Re-open the (now-empty) log file.
            match open_log_file(&log_path) {
                Ok(f) => {
                    file = f;
                    current_size = 0;
                }
                Err(e) => {
                    tracing::warn!(
                        "Could not re-open log file {:?} after rotation: {} — stopping disk logging",
                        log_path,
                        e
                    );
                    drain_channel(&mut rx).await;
                    return;
                }
            }
        }
    }
}

/// Format a log line for disk output.
fn format_log_line(line: &LogLine) -> String {
    let stream_tag = match line.stream {
        crate::types::LogStream::Stdout => "OUT",
        crate::types::LogStream::Stderr => "ERR",
    };
    format!(
        "[{}] [{}] {}\n",
        line.timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
        stream_tag,
        line.line
    )
}

/// Write a batch of lines to the file and update the running size.
fn flush_batch(file: &mut std::fs::File, batch: &[LogLine], current_size: &mut u64) {
    for line in batch {
        let formatted = format_log_line(line);
        match file.write_all(formatted.as_bytes()) {
            Ok(()) => *current_size += formatted.len() as u64,
            Err(e) => {
                tracing::debug!("Failed to write log line: {}", e);
                // Continue — don't crash the writer over a single bad write.
            }
        }
    }
    if let Err(e) = file.flush() {
        tracing::debug!("Failed to flush log file: {}", e);
    }
}

/// Open (or create) the log file in append mode.
fn open_log_file(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
}

/// Rotate log files:
///   `console.log`   → `console.log.1`
///   `console.log.1` → `console.log.2`
///   `console.log.2` → deleted
fn rotate_logs(log_path: &Path) -> std::io::Result<()> {
    // Delete the oldest rotated file first.
    for i in (1..=MAX_ROTATED_FILES).rev() {
        let src = rotated_path(log_path, if i == 1 { 0 } else { i - 1 });
        let dst = rotated_path(log_path, i);

        if i == MAX_ROTATED_FILES {
            // Delete the oldest if it exists.
            if dst.exists() {
                std::fs::remove_file(&dst)?;
            }
        }

        let actual_src = if i == 1 { log_path.to_path_buf() } else { src };
        if actual_src.exists() {
            std::fs::rename(&actual_src, &dst)?;
        }
    }

    Ok(())
}

/// Build the path for a rotated log file (e.g. `console.log.1`).
/// Index 0 means the current (non-rotated) file.
fn rotated_path(base: &Path, index: u32) -> PathBuf {
    if index == 0 {
        base.to_path_buf()
    } else {
        let mut p = base.as_os_str().to_owned();
        p.push(format!(".{}", index));
        PathBuf::from(p)
    }
}

/// Drain the channel without processing so senders don't block forever
/// after a fatal writer error.
async fn drain_channel(rx: &mut mpsc::Receiver<LogLine>) {
    while rx.recv().await.is_some() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{LogLine, LogStream};
    use chrono::Utc;

    fn sample_line(msg: &str) -> LogLine {
        LogLine {
            seq: 0,
            timestamp: Utc::now(),
            line: msg.to_string(),
            stream: LogStream::Stdout,
        }
    }

    #[test]
    fn test_format_log_line_stdout() {
        let line = sample_line("Hello world");
        let formatted = format_log_line(&line);
        assert!(formatted.contains("[OUT]"));
        assert!(formatted.contains("Hello world"));
        assert!(formatted.ends_with('\n'));
    }

    #[test]
    fn test_format_log_line_stderr() {
        let line = LogLine {
            seq: 0,
            timestamp: Utc::now(),
            line: "error!".to_string(),
            stream: LogStream::Stderr,
        };
        let formatted = format_log_line(&line);
        assert!(formatted.contains("[ERR]"));
    }

    #[test]
    fn test_rotated_path() {
        let base = PathBuf::from("/tmp/logs/console.log");
        assert_eq!(
            rotated_path(&base, 0),
            PathBuf::from("/tmp/logs/console.log")
        );
        assert_eq!(
            rotated_path(&base, 1),
            PathBuf::from("/tmp/logs/console.log.1")
        );
        assert_eq!(
            rotated_path(&base, 2),
            PathBuf::from("/tmp/logs/console.log.2")
        );
    }

    #[test]
    fn test_rotate_logs() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("console.log");

        // Create the initial log file with some content.
        std::fs::write(&log_path, "original").unwrap();

        rotate_logs(&log_path).unwrap();

        // Original should be gone (renamed to .1).
        assert!(!log_path.exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("console.log.1")).unwrap(),
            "original"
        );

        // Create a new current file and rotate again.
        std::fs::write(&log_path, "second").unwrap();
        rotate_logs(&log_path).unwrap();

        assert!(!log_path.exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("console.log.1")).unwrap(),
            "second"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("console.log.2")).unwrap(),
            "original"
        );

        // Rotate again — console.log.2 (original) should be deleted.
        std::fs::write(&log_path, "third").unwrap();
        rotate_logs(&log_path).unwrap();

        assert!(!log_path.exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("console.log.1")).unwrap(),
            "third"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("console.log.2")).unwrap(),
            "second"
        );
        // "original" was pushed out.
    }

    #[test]
    fn test_flush_batch_updates_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.log");
        let mut file = open_log_file(&path).unwrap();
        let mut size: u64 = 0;

        let lines = vec![sample_line("hello"), sample_line("world")];
        flush_batch(&mut file, &lines, &mut size);

        assert!(size > 0);
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("hello"));
        assert!(contents.contains("world"));
        assert_eq!(size, contents.len() as u64);
    }

    #[tokio::test]
    async fn test_spawn_log_writer_writes_lines() {
        let dir = tempfile::tempdir().unwrap();
        let server_dir = dir.path();
        let sender = spawn_log_writer(server_dir, 50 * 1024 * 1024);

        sender.send(&sample_line("line one"));
        sender.send(&sample_line("line two"));

        // Drop the sender to signal the writer task to flush and exit.
        drop(sender);

        // Give the writer task a moment to flush.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let log_path = server_dir.join("logs").join("console.log");
        assert!(log_path.exists(), "log file should have been created");

        let contents = std::fs::read_to_string(&log_path).unwrap();
        assert!(contents.contains("line one"));
        assert!(contents.contains("line two"));
    }

    #[tokio::test]
    async fn test_spawn_log_writer_rotates() {
        let dir = tempfile::tempdir().unwrap();
        let server_dir = dir.path();

        // Set a very small max size to trigger rotation quickly.
        let sender = spawn_log_writer(server_dir, 100);

        // Send enough data to exceed 100 bytes.
        for i in 0..20 {
            sender.send(&sample_line(&format!(
                "This is a long log line number {} to trigger rotation",
                i
            )));
        }

        drop(sender);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let logs_dir = server_dir.join("logs");
        let rotated = logs_dir.join("console.log.1");
        // At least one rotation should have happened.
        assert!(
            rotated.exists(),
            "expected at least one rotated log file at {:?}",
            rotated
        );
    }
}
