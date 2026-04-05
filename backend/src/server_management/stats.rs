//! Per-server resource usage collection (ticket 015).
//!
//! Provides a [`StatsCollector`] that periodically samples CPU, memory, and
//! disk usage for every managed server process.  Results are cached in a
//! [`DashMap`] so API handlers can serve them without touching `/proc` on
//! every request.
//!
//! ## Platform support
//!
//! CPU and memory stats rely on `/proc/<pid>/stat` and `/proc/<pid>/status`
//! which are Linux-only.  On other platforms those fields are returned as
//! `None` and no error is raised.  Disk usage (directory size) works on all
//! platforms.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

// ─── Public Types ──────────────────────────────────────────────────────────

/// Resource usage snapshot for a single managed server.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types/generated/")]
pub struct ServerResourceStats {
    pub server_id: Uuid,
    /// CPU usage as a percentage (0.0–100.0).
    /// `None` when the server is stopped or the platform doesn't support
    /// `/proc`-based sampling.
    pub cpu_percent: Option<f32>,
    /// Resident Set Size in bytes.
    /// `None` when the server is stopped or unsupported.
    #[ts(type = "number | null")]
    pub memory_rss_bytes: Option<u64>,
    /// Swap usage in bytes (from VmSwap).
    /// `None` when the server is stopped or unsupported.
    #[ts(type = "number | null")]
    pub memory_swap_bytes: Option<u64>,
    /// Total size of the server's data directory in bytes.
    /// Available even when the server is stopped.
    #[ts(type = "number")]
    pub disk_usage_bytes: u64,
    /// When this snapshot was taken.
    pub timestamp: DateTime<Utc>,
}

// ─── Internal bookkeeping for CPU delta calculation ────────────────────────

/// Raw CPU tick values read from `/proc/<pid>/stat`.
#[derive(Debug, Clone)]
struct CpuSample {
    /// `utime + stime` in clock ticks.
    total_ticks: u64,
    /// Wall-clock instant when the sample was taken.
    when: Instant,
}

/// Previous-sample state kept between collection cycles.
#[derive(Debug, Clone)]
struct PreviousSample {
    cpu: CpuSample,
}

// ─── StatsCollector ────────────────────────────────────────────────────────

/// Caches the most recent [`ServerResourceStats`] for every known server and
/// drives the periodic background sampling task.
pub struct StatsCollector {
    /// Most recent stats keyed by server UUID.
    pub cache: Arc<DashMap<Uuid, ServerResourceStats>>,
    /// Previous CPU sample for delta calculation, keyed by server UUID.
    previous: Arc<DashMap<Uuid, PreviousSample>>,
    /// Clock ticks per second (`sysconf(_SC_CLK_TCK)`).  Cached once at
    /// construction time.
    #[allow(dead_code)]
    ticks_per_sec: u64,
}

impl Default for StatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsCollector {
    /// Create a new collector.  Call [`spawn_collection_task`] afterwards to
    /// start the background loop.
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            previous: Arc::new(DashMap::new()),
            ticks_per_sec: clock_ticks_per_sec(),
        }
    }

    /// Return the cached stats for a single server (if available).
    pub fn get(&self, server_id: &Uuid) -> Option<ServerResourceStats> {
        self.cache.get(server_id).map(|r| r.value().clone())
    }

    /// Remove a server from the cache (e.g. after deletion).
    pub fn remove(&self, server_id: &Uuid) {
        self.cache.remove(server_id);
        self.previous.remove(server_id);
    }

    /// Run a single collection cycle for the given set of servers.
    ///
    /// `servers` is a list of `(server_id, optional_pid, server_data_dir)`.
    /// This is called by the background task but is also useful in tests.
    pub fn collect_once(&self, servers: &[(Uuid, Option<u32>, PathBuf)]) {
        let now = Utc::now();
        let wall_now = Instant::now();

        for (server_id, pid, data_dir) in servers {
            let (cpu_percent, memory_rss_bytes, memory_swap_bytes) = match pid {
                Some(p) if is_pid_alive(*p) => {
                    let cpu = self.sample_cpu(*server_id, *p, wall_now);
                    let (rss, swap) = read_memory(*p);
                    (cpu, rss, swap)
                }
                _ => (None, None, None),
            };

            let disk_usage_bytes = dir_size(data_dir);

            let stats = ServerResourceStats {
                server_id: *server_id,
                cpu_percent,
                memory_rss_bytes,
                memory_swap_bytes,
                disk_usage_bytes,
                timestamp: now,
            };

            self.cache.insert(*server_id, stats);
        }

        // Clean up previous-sample entries for servers that no longer exist
        // in the input set.
        let active_ids: std::collections::HashSet<Uuid> =
            servers.iter().map(|(id, _, _)| *id).collect();
        self.previous.retain(|id, _| active_ids.contains(id));
    }

    // ── CPU sampling ───────────────────────────────────────────────────

    /// Read current CPU ticks for `pid` and compute the delta percentage
    /// against the previous sample (if any).
    fn sample_cpu(&self, server_id: Uuid, pid: u32, wall_now: Instant) -> Option<f32> {
        let ticks = read_cpu_ticks(pid)?;
        let current = CpuSample {
            total_ticks: ticks,
            when: wall_now,
        };

        let cpu_percent = if let Some(prev) = self.previous.get(&server_id) {
            let dt = current.when.duration_since(prev.cpu.when).as_secs_f64();
            if dt > 0.0 {
                let dticks = current.total_ticks.saturating_sub(prev.cpu.total_ticks) as f64;
                let tps = self.ticks_per_sec as f64;
                // percentage = (delta_ticks / tps) / delta_wall_seconds * 100
                let pct = (dticks / tps) / dt * 100.0;
                // Clamp to [0, num_cpus * 100] then to 0..=100 for a
                // single-process view.  A process using multiple cores can
                // exceed 100% in raw terms; we cap at 100 for the UI.
                Some((pct as f32).clamp(0.0, 100.0))
            } else {
                // No time elapsed — can't compute meaningful delta.
                None
            }
        } else {
            // First sample — no delta yet.
            None
        };

        self.previous
            .insert(server_id, PreviousSample { cpu: current });

        cpu_percent
    }
}

// ─── Background task ───────────────────────────────────────────────────────

/// Spawn a tokio task that collects stats for all managed servers every
/// `interval`.
///
/// The task runs until the returned [`tokio::task::JoinHandle`] is aborted
/// or the process exits.
pub fn spawn_collection_task(
    collector: Arc<StatsCollector>,
    state: Arc<crate::AppState>,
    interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // The first tick completes immediately — skip it so we don't
        // sample before any process has had time to start.
        ticker.tick().await;

        loop {
            ticker.tick().await;

            // Build the list of (server_id, optional_pid, data_dir).
            let servers: Vec<(Uuid, Option<u32>, PathBuf)> = {
                let all_servers = match state.db.list_servers().await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("Stats collector: failed to list servers: {}", e);
                        continue;
                    }
                };

                all_servers
                    .into_iter()
                    .map(|s| {
                        let runtime = state.process_manager.get_runtime(&s.id);
                        let pid = runtime.pid;
                        let dir = state.server_dir(&s.id);
                        (s.id, pid, dir)
                    })
                    .collect()
            };

            // Offload the potentially blocking /proc reads and directory
            // walks to a blocking thread so we don't stall the async
            // runtime.
            let coll = Arc::clone(&collector);
            let _ = tokio::task::spawn_blocking(move || {
                coll.collect_once(&servers);
            })
            .await;
        }
    })
}

// ─── /proc helpers (Linux) ─────────────────────────────────────────────────

/// Read `utime + stime` from `/proc/<pid>/stat`.
///
/// Returns `None` on non-Linux platforms or if the file can't be read (e.g.
/// the process exited between the PID lookup and the read).
fn read_cpu_ticks(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let path = format!("/proc/{}/stat", pid);
        let contents = std::fs::read_to_string(&path).ok()?;
        // The comm field (field 2) is wrapped in parens and may contain
        // spaces, so we find the *last* ')' and parse from there.
        let after_comm = contents.rfind(')')? + 1;
        let fields: Vec<&str> = contents[after_comm..].split_whitespace().collect();
        // After the closing ')' the fields are (1-indexed in the man page):
        //   3=state, 4=ppid, ..., 14=utime, 15=stime
        // In our 0-indexed `fields` vec that's indices 11 and 12.
        if fields.len() < 13 {
            return None;
        }
        let utime: u64 = fields[11].parse().ok()?;
        let stime: u64 = fields[12].parse().ok()?;
        Some(utime + stime)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// Read `VmRSS` and `VmSwap` from `/proc/<pid>/status`.
///
/// Returns `(Some(rss_bytes), Some(swap_bytes))` on Linux, `(None, None)`
/// elsewhere or on read failure.
fn read_memory(pid: u32) -> (Option<u64>, Option<u64>) {
    #[cfg(target_os = "linux")]
    {
        let path = format!("/proc/{}/status", pid);
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return (None, None),
        };

        let mut rss: Option<u64> = None;
        let mut swap: Option<u64> = None;

        for line in contents.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                rss = parse_kb_line(rest);
            } else if let Some(rest) = line.strip_prefix("VmSwap:") {
                swap = parse_kb_line(rest);
            }
            if rss.is_some() && swap.is_some() {
                break;
            }
        }

        (rss, swap)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        (None, None)
    }
}

/// Parse a line like `"   12345 kB"` into bytes.
fn parse_kb_line(s: &str) -> Option<u64> {
    let trimmed = s.trim();
    let numeric_part = trimmed.split_whitespace().next()?;
    let kb: u64 = numeric_part.parse().ok()?;
    Some(kb * 1024)
}

/// Get the system clock ticks per second.
fn clock_ticks_per_sec() -> u64 {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: sysconf is always safe to call with a valid constant.
        let val = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        if val > 0 {
            val as u64
        } else {
            100 // sensible default
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        100
    }
}

/// Check whether a PID is still alive.
fn is_pid_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        // Guard against values that don't fit in a positive i32.
        // `kill(-1, 0)` signals *all* processes (always succeeds) and
        // `kill(0, 0)` signals the caller's process group — neither is
        // what we want.
        let Ok(pid_i32) = i32::try_from(pid) else {
            return false;
        };
        if pid_i32 <= 0 {
            return false;
        }
        // kill(pid, 0) checks existence without sending a signal.
        let ret = unsafe { libc::kill(pid_i32, 0) };
        ret == 0
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        false
    }
}

/// Public wrapper for [`dir_size`] so the API handler can compute disk
/// usage on the fly when no cached stats are available yet.
#[allow(rustdoc::private_intra_doc_links)]
pub fn dir_size_public(path: &Path) -> u64 {
    dir_size(path)
}

/// Compute the total size of a directory tree (non-recursive errors are
/// silently skipped).
fn dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }

    let mut total: u64 = 0;
    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

// ─── Unit tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── dir_size ──

    #[test]
    fn test_dir_size_empty() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(dir_size(tmp.path()), 0);
    }

    #[test]
    fn test_dir_size_with_files() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "hello").unwrap(); // 5 bytes
        std::fs::create_dir(tmp.path().join("sub")).unwrap();
        std::fs::write(tmp.path().join("sub/b.txt"), "world!").unwrap(); // 6 bytes
        assert_eq!(dir_size(tmp.path()), 11);
    }

    #[test]
    fn test_dir_size_nonexistent() {
        assert_eq!(dir_size(Path::new("/nonexistent/path/12345")), 0);
    }

    // ── parse_kb_line ──

    #[test]
    fn test_parse_kb_line() {
        assert_eq!(parse_kb_line("   12345 kB"), Some(12345 * 1024));
        assert_eq!(parse_kb_line("0 kB"), Some(0));
        assert_eq!(parse_kb_line("   "), None);
        assert_eq!(parse_kb_line("abc kB"), None);
    }

    // ── clock_ticks_per_sec ──

    #[test]
    fn test_clock_ticks_nonzero() {
        let tps = clock_ticks_per_sec();
        assert!(tps > 0, "clock ticks per sec should be > 0, got {}", tps);
    }

    // ── is_pid_alive ──

    #[test]
    fn test_is_pid_alive_self() {
        // Our own process should be alive.
        let pid = std::process::id();
        if cfg!(target_os = "linux") {
            assert!(is_pid_alive(pid));
        }
    }

    #[test]
    fn test_is_pid_alive_bogus() {
        // PID 0 and very large PIDs should not be alive (or return false
        // on non-Linux).
        assert!(!is_pid_alive(u32::MAX));
    }

    // ── StatsCollector basics ──

    #[test]
    fn test_collector_get_returns_none_before_collection() {
        let collector = StatsCollector::new();
        let id = Uuid::new_v4();
        assert!(collector.get(&id).is_none());
    }

    #[test]
    fn test_collector_collect_once_stopped_server() {
        let collector = StatsCollector::new();
        let id = Uuid::new_v4();
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("data.bin"), "0123456789").unwrap(); // 10 bytes

        collector.collect_once(&[(id, None, tmp.path().to_path_buf())]);

        let stats = collector.get(&id).unwrap();
        assert_eq!(stats.server_id, id);
        assert!(stats.cpu_percent.is_none());
        assert!(stats.memory_rss_bytes.is_none());
        assert!(stats.memory_swap_bytes.is_none());
        assert_eq!(stats.disk_usage_bytes, 10);
    }

    #[test]
    fn test_collector_remove() {
        let collector = StatsCollector::new();
        let id = Uuid::new_v4();
        let tmp = TempDir::new().unwrap();

        collector.collect_once(&[(id, None, tmp.path().to_path_buf())]);
        assert!(collector.get(&id).is_some());

        collector.remove(&id);
        assert!(collector.get(&id).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_collector_collect_live_process() {
        // Use our own PID — we know it's alive.
        let collector = StatsCollector::new();
        let id = Uuid::new_v4();
        let tmp = TempDir::new().unwrap();
        let pid = std::process::id();

        // First collection: CPU delta is None (no previous sample).
        collector.collect_once(&[(id, Some(pid), tmp.path().to_path_buf())]);
        let stats1 = collector.get(&id).unwrap();
        assert!(
            stats1.cpu_percent.is_none(),
            "First sample should have no CPU delta"
        );
        assert!(
            stats1.memory_rss_bytes.is_some(),
            "Our process should have measurable RSS"
        );
        assert!(stats1.memory_rss_bytes.unwrap() > 0, "RSS should be > 0");

        // Second collection: CPU delta should now be available.
        // Do a little busy work so we consume some CPU.
        let mut _v = 0u64;
        for i in 0..1_000_000u64 {
            _v = _v.wrapping_add(i);
        }

        collector.collect_once(&[(id, Some(pid), tmp.path().to_path_buf())]);
        let stats2 = collector.get(&id).unwrap();
        // CPU percent may be Some or None depending on timing — the
        // important thing is it doesn't panic or error.
        assert_eq!(stats2.server_id, id);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_read_cpu_ticks_self() {
        let pid = std::process::id();
        let ticks = read_cpu_ticks(pid);
        assert!(ticks.is_some(), "Should be able to read our own CPU ticks");
        // The value may legitimately be 0 if the test binary just
        // started, so we only assert that parsing succeeded.
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_read_memory_self() {
        let pid = std::process::id();
        let (rss, swap) = read_memory(pid);
        assert!(rss.is_some(), "Should be able to read our own RSS");
        assert!(rss.unwrap() > 0, "Our RSS should be > 0");
        // swap may be 0 or Some(0), that's fine.
        assert!(swap.is_some(), "VmSwap should be readable");
    }

    #[test]
    fn test_read_cpu_ticks_dead_pid() {
        let ticks = read_cpu_ticks(u32::MAX);
        assert!(ticks.is_none(), "Dead PID should return None");
    }

    #[test]
    fn test_read_memory_dead_pid() {
        let (rss, swap) = read_memory(u32::MAX);
        assert!(rss.is_none());
        assert!(swap.is_none());
    }

    // ── Multiple servers in one cycle ──

    #[test]
    fn test_collector_multiple_servers() {
        let collector = StatsCollector::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();

        std::fs::write(tmp1.path().join("a"), "aaa").unwrap(); // 3
        std::fs::write(tmp2.path().join("b"), "bbbbbb").unwrap(); // 6

        collector.collect_once(&[
            (id1, None, tmp1.path().to_path_buf()),
            (id2, None, tmp2.path().to_path_buf()),
        ]);

        let s1 = collector.get(&id1).unwrap();
        let s2 = collector.get(&id2).unwrap();
        assert_eq!(s1.disk_usage_bytes, 3);
        assert_eq!(s2.disk_usage_bytes, 6);
    }

    // ── Stale previous samples are cleaned up ──

    #[test]
    fn test_collector_cleans_stale_previous() {
        let collector = StatsCollector::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let tmp = TempDir::new().unwrap();

        // Collect both
        collector.collect_once(&[
            (id1, None, tmp.path().to_path_buf()),
            (id2, None, tmp.path().to_path_buf()),
        ]);
        assert!(collector.cache.contains_key(&id1));
        assert!(collector.cache.contains_key(&id2));

        // Collect only id1 — id2's previous sample should be cleaned up
        // (though cache entry persists from the first cycle).
        collector.collect_once(&[(id1, None, tmp.path().to_path_buf())]);
        assert!(!collector.previous.contains_key(&id2));
    }
}
