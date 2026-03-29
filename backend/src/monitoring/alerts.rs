use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use parking_lot::Mutex;
use uuid::Uuid;

use crate::types::{AlertConfig, AlertEvent, AlertEventKind, ServerAlertConfig};
use crate::AppState;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CooldownKey {
    server_id: Uuid,
    kind: AlertEventKind,
}

pub struct AlertDispatcher {
    cooldowns: Mutex<HashMap<CooldownKey, Instant>>,
}

impl Default for AlertDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertDispatcher {
    pub fn new() -> Self {
        Self {
            cooldowns: Mutex::new(HashMap::new()),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_in_cooldown(
        &self,
        server_id: Uuid,
        kind: AlertEventKind,
        cooldown_secs: u64,
    ) -> bool {
        let key = CooldownKey { server_id, kind };
        let mut map = self.cooldowns.lock();
        let now = Instant::now();
        let cooldown = Duration::from_secs(cooldown_secs);

        if let Some(last) = map.get(&key) {
            if now.duration_since(*last) < cooldown {
                return true; // still in cooldown
            }
        }

        map.insert(key, now);
        false
    }

    pub fn clear_cooldowns_for_server(&self, server_id: &Uuid) {
        let mut map = self.cooldowns.lock();
        map.retain(|k, _| k.server_id != *server_id);
    }

    pub fn fire(&self, state: &Arc<AppState>, event: AlertEvent) {
        let state = Arc::clone(state);

        tokio::spawn(async move {
            let alert_config = match state.db.get_alert_config().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Alert dispatcher: failed to load alert config: {}", e);
                    return;
                }
            };

            if !alert_config.enabled {
                return;
            }

            if alert_config.recipients.is_empty() {
                return;
            }

            if !is_trigger_enabled(&alert_config, event.kind) {
                return;
            }

            match state.db.get_server_alert_config(&event.server_id).await {
                Ok(Some(ServerAlertConfig { muted: true, .. })) => {
                    tracing::debug!(
                        "Alert suppressed (server muted): {} for server {}",
                        event.kind.display_name(),
                        event.server_id
                    );
                    return;
                }
                Err(e) => {
                    tracing::warn!(
                        "Alert dispatcher: failed to load server alert config for {}: {}",
                        event.server_id,
                        e
                    );
                }
                _ => {}
            }

            let smtp_config = match state.db.get_smtp_config().await {
                Ok(Some(c)) => c,
                Ok(None) => {
                    tracing::debug!(
                        "Alert not sent (no SMTP config): {} for server {}",
                        event.kind.display_name(),
                        event.server_id
                    );
                    return;
                }
                Err(e) => {
                    tracing::warn!("Alert dispatcher: failed to load SMTP config: {}", e);
                    return;
                }
            };

            let recipients = alert_config.recipients.clone();
            let base_url = alert_config.base_url.clone();

            tracing::info!(
                "Dispatching alert: {} for server '{}' ({})",
                event.kind.display_name(),
                event.server_name,
                event.server_id,
            );

            if let Err(e) = crate::monitoring::email::send_alert_email(
                &smtp_config,
                &recipients,
                &event,
                base_url.as_deref(),
            )
            .await
            {
                tracing::error!(
                    "Failed to send alert email ({} for server {}): {}",
                    event.kind.display_name(),
                    event.server_id,
                    e
                );
            }
        });
    }

    /// Emit an alert with a pre-built message for a given event kind.
    fn notify(
        &self,
        state: &Arc<AppState>,
        server_id: Uuid,
        server_name: &str,
        kind: AlertEventKind,
        message: String,
    ) {
        self.fire(
            state,
            AlertEvent {
                kind,
                server_id,
                server_name: server_name.to_string(),
                timestamp: Utc::now(),
                message,
            },
        );
    }

    pub fn notify_server_crashed(&self, state: &Arc<AppState>, server_id: Uuid, server_name: &str) {
        self.notify(
            state,
            server_id,
            server_name,
            AlertEventKind::ServerCrashed,
            format!("Server '{}' process exited unexpectedly.", server_name),
        );
    }

    pub fn notify_restart_exhausted(
        &self,
        state: &Arc<AppState>,
        server_id: Uuid,
        server_name: &str,
        attempts: u32,
        max_attempts: u32,
    ) {
        self.notify(state, server_id, server_name, AlertEventKind::RestartExhausted,
            format!("Server '{}' exhausted all {} of {} restart attempts and will not be restarted automatically.",
                server_name, attempts, max_attempts));
    }

    pub fn notify_server_down(
        &self,
        state: &Arc<AppState>,
        server_id: Uuid,
        server_name: &str,
        down_mins: u64,
    ) {
        self.notify(
            state,
            server_id,
            server_name,
            AlertEventKind::ServerDown,
            format!(
                "Server '{}' has been down for {} minutes.",
                server_name, down_mins
            ),
        );
    }

    pub fn notify_high_memory(
        &self,
        state: &Arc<AppState>,
        server_id: Uuid,
        server_name: &str,
        memory_bytes: u64,
        threshold_percent: f64,
        system_total_bytes: u64,
    ) {
        let percent = if system_total_bytes > 0 {
            (memory_bytes as f64 / system_total_bytes as f64) * 100.0
        } else {
            0.0
        };
        self.notify(state, server_id, server_name, AlertEventKind::HighMemory,
            format!("Server '{}' is using {:.1}% of system memory ({} MB), exceeding the {:.0}% threshold.",
                server_name, percent, memory_bytes / (1024 * 1024), threshold_percent));
    }

    pub fn notify_high_cpu(
        &self,
        state: &Arc<AppState>,
        server_id: Uuid,
        server_name: &str,
        cpu_percent: f64,
        threshold_percent: f64,
    ) {
        self.notify(
            state,
            server_id,
            server_name,
            AlertEventKind::HighCpu,
            format!(
                "Server '{}' CPU usage is at {:.1}%, exceeding the {:.0}% threshold.",
                server_name, cpu_percent, threshold_percent
            ),
        );
    }

    pub fn notify_low_disk(
        &self,
        state: &Arc<AppState>,
        server_id: Uuid,
        server_name: &str,
        free_mb: u64,
        threshold_mb: u64,
    ) {
        self.notify(
            state,
            server_id,
            server_name,
            AlertEventKind::LowDisk,
            format!(
                "Server '{}' data partition has only {} MB free, below the {} MB threshold.",
                server_name, free_mb, threshold_mb
            ),
        );
    }
}

fn is_trigger_enabled(config: &AlertConfig, kind: AlertEventKind) -> bool {
    match kind {
        AlertEventKind::ServerCrashed => config.triggers.server_crashed,
        AlertEventKind::RestartExhausted => config.triggers.restart_exhausted,
        AlertEventKind::ServerDown => config.triggers.server_down,
        AlertEventKind::HighMemory => config.triggers.high_memory,
        AlertEventKind::HighCpu => config.triggers.high_cpu,
        AlertEventKind::LowDisk => config.triggers.low_disk,
    }
}

pub fn spawn_alert_monitor_task(
    state: Arc<AppState>,
    interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await;

        let mut down_since: HashMap<Uuid, Instant> = HashMap::new();

        loop {
            ticker.tick().await;

            let alert_config = match state.db.get_alert_config().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Alert monitor: failed to load config: {}", e);
                    continue;
                }
            };

            if !alert_config.enabled {
                continue;
            }

            let servers = match state.db.list_servers().await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("Alert monitor: failed to list servers: {}", e);
                    continue;
                }
            };

            let system_total_memory = {
                let sys = state.system_monitor.lock();
                sys.total_memory()
            };

            let disk_free_mb = get_data_dir_free_mb(&state.data_dir);

            for server in &servers {
                let runtime = state.process_manager.get_runtime(&server.id);

                if alert_config.triggers.server_down {
                    let is_down = matches!(
                        runtime.status,
                        crate::types::ServerStatus::Stopped | crate::types::ServerStatus::Crashed
                    );

                    let should_be_running = server.config.auto_start || server.installed;

                    if is_down && should_be_running {
                        let entry = down_since.entry(server.id).or_insert_with(Instant::now);
                        let down_mins = entry.elapsed().as_secs() / 60;
                        if down_mins >= alert_config.triggers.down_threshold_mins
                            && alert_config.triggers.down_threshold_mins > 0
                        {
                            state.alert_dispatcher.notify_server_down(
                                &state,
                                server.id,
                                &server.config.name,
                                down_mins,
                            );
                        }
                    } else {
                        down_since.remove(&server.id);
                    }
                }

                if runtime.pid.is_some() {
                    if let Some(stats) = state.stats_collector.get(&server.id) {
                        if alert_config.triggers.high_memory && system_total_memory > 0 {
                            if let Some(rss_bytes) = stats.memory_rss_bytes {
                                let mem_percent =
                                    (rss_bytes as f64 / system_total_memory as f64) * 100.0;
                                if mem_percent >= alert_config.triggers.memory_threshold_percent {
                                    state.alert_dispatcher.notify_high_memory(
                                        &state,
                                        server.id,
                                        &server.config.name,
                                        rss_bytes,
                                        alert_config.triggers.memory_threshold_percent,
                                        system_total_memory,
                                    );
                                }
                            }
                        }

                        if alert_config.triggers.high_cpu {
                            if let Some(cpu) = stats.cpu_percent {
                                let cpu_f64 = cpu as f64;
                                if cpu_f64 >= alert_config.triggers.cpu_threshold_percent {
                                    state.alert_dispatcher.notify_high_cpu(
                                        &state,
                                        server.id,
                                        &server.config.name,
                                        cpu_f64,
                                        alert_config.triggers.cpu_threshold_percent,
                                    );
                                }
                            }
                        }
                    }
                }

                if alert_config.triggers.low_disk {
                    if let Some(free_mb) = disk_free_mb {
                        if free_mb < alert_config.triggers.disk_threshold_mb {
                            state.alert_dispatcher.notify_low_disk(
                                &state,
                                server.id,
                                &server.config.name,
                                free_mb,
                                alert_config.triggers.disk_threshold_mb,
                            );
                        }
                    }
                }
            }

            let server_ids: std::collections::HashSet<Uuid> =
                servers.iter().map(|s| s.id).collect();
            down_since.retain(|id, _| server_ids.contains(id));
        }
    })
}

fn get_data_dir_free_mb(data_dir: &std::path::Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();

    let data_dir_str = data_dir.to_string_lossy();
    let mut best: Option<(usize, u64)> = None;

    for disk in disks.iter() {
        let mount = disk.mount_point().to_string_lossy();
        if data_dir_str.starts_with(mount.as_ref()) {
            let len = mount.len();
            if best.is_none_or(|(best_len, _)| len > best_len) {
                let free_mb = disk.available_space() / (1024 * 1024);
                best = Some((len, free_mb));
            }
        }
    }

    best.map(|(_, mb)| mb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AlertConfig, AlertEventKind, AlertTriggers};

    #[test]
    fn test_is_trigger_enabled() {
        let config = AlertConfig::default();
        // Default: server_crashed and restart_exhausted are true
        assert!(is_trigger_enabled(&config, AlertEventKind::ServerCrashed));
        assert!(is_trigger_enabled(
            &config,
            AlertEventKind::RestartExhausted
        ));
        // Default: resource triggers are false
        assert!(!is_trigger_enabled(&config, AlertEventKind::HighMemory));
        assert!(!is_trigger_enabled(&config, AlertEventKind::HighCpu));
        assert!(!is_trigger_enabled(&config, AlertEventKind::LowDisk));
        assert!(!is_trigger_enabled(&config, AlertEventKind::ServerDown));
    }

    #[test]
    fn test_is_trigger_enabled_all_on() {
        let config = AlertConfig {
            triggers: AlertTriggers {
                server_crashed: true,
                restart_exhausted: true,
                server_down: true,
                high_memory: true,
                high_cpu: true,
                low_disk: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(is_trigger_enabled(&config, AlertEventKind::ServerCrashed));
        assert!(is_trigger_enabled(
            &config,
            AlertEventKind::RestartExhausted
        ));
        assert!(is_trigger_enabled(&config, AlertEventKind::ServerDown));
        assert!(is_trigger_enabled(&config, AlertEventKind::HighMemory));
        assert!(is_trigger_enabled(&config, AlertEventKind::HighCpu));
        assert!(is_trigger_enabled(&config, AlertEventKind::LowDisk));
    }

    #[test]
    fn test_cooldown_allows_first_then_blocks() {
        let dispatcher = AlertDispatcher::new();
        let server_id = Uuid::new_v4();
        let kind = AlertEventKind::ServerCrashed;

        // First call should not be in cooldown.
        assert!(!dispatcher.is_in_cooldown(server_id, kind, 300));

        // Second call within cooldown window should be blocked.
        assert!(dispatcher.is_in_cooldown(server_id, kind, 300));
    }

    #[test]
    fn test_cooldown_different_servers_independent() {
        let dispatcher = AlertDispatcher::new();
        let server_a = Uuid::new_v4();
        let server_b = Uuid::new_v4();
        let kind = AlertEventKind::ServerCrashed;

        assert!(!dispatcher.is_in_cooldown(server_a, kind, 300));
        // Different server should still be allowed.
        assert!(!dispatcher.is_in_cooldown(server_b, kind, 300));
    }

    #[test]
    fn test_cooldown_different_kinds_independent() {
        let dispatcher = AlertDispatcher::new();
        let server_id = Uuid::new_v4();

        assert!(!dispatcher.is_in_cooldown(server_id, AlertEventKind::ServerCrashed, 300));
        // Different event kind should still be allowed.
        assert!(!dispatcher.is_in_cooldown(server_id, AlertEventKind::HighMemory, 300));
    }

    #[test]
    fn test_cooldown_zero_never_blocks() {
        let dispatcher = AlertDispatcher::new();
        let server_id = Uuid::new_v4();
        let kind = AlertEventKind::ServerCrashed;

        // With 0-second cooldown, nothing should be blocked.
        assert!(!dispatcher.is_in_cooldown(server_id, kind, 0));
        assert!(!dispatcher.is_in_cooldown(server_id, kind, 0));
        assert!(!dispatcher.is_in_cooldown(server_id, kind, 0));
    }

    #[test]
    fn test_clear_cooldowns_for_server() {
        let dispatcher = AlertDispatcher::new();
        let server_a = Uuid::new_v4();
        let server_b = Uuid::new_v4();
        let kind = AlertEventKind::ServerCrashed;

        // Put both servers in cooldown.
        dispatcher.is_in_cooldown(server_a, kind, 300);
        dispatcher.is_in_cooldown(server_b, kind, 300);

        // Clear cooldowns for server_a.
        dispatcher.clear_cooldowns_for_server(&server_a);

        // server_a should be allowed again.
        assert!(!dispatcher.is_in_cooldown(server_a, kind, 300));
        // server_b should still be in cooldown.
        assert!(dispatcher.is_in_cooldown(server_b, kind, 300));
    }

    #[test]
    fn test_get_data_dir_free_mb_returns_something() {
        // The data dir "/" or "/tmp" should always resolve to a disk.
        let result = get_data_dir_free_mb(std::path::Path::new("/tmp"));
        assert!(result.is_some());
        // Should be non-negative (it's u64, so always >= 0).
        assert!(result.unwrap() > 0);
    }
}
