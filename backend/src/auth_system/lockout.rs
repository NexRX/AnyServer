//! Per-username exponential backoff for brute-force protection.
//!
//! Backoff: 1–3 failures = no cooldown, then 5s → 15s → 30s → 1m → 2m → 5m → 15m cap.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;

#[derive(Clone)]
pub struct LoginAttemptTracker {
    attempts: Arc<DashMap<String, FailureState>>,
}

#[derive(Debug, Clone)]
struct FailureState {
    consecutive_failures: u32,
    last_failure_at: Instant,
    cooldown_until: Option<Instant>,
}

impl LoginAttemptTracker {
    pub fn new() -> Self {
        Self {
            attempts: Arc::new(DashMap::new()),
        }
    }

    pub fn check_allowed(&self, username: &str) -> Result<(), u64> {
        let normalized = normalize_username(username);

        if let Some(entry) = self.attempts.get(&normalized) {
            if let Some(cooldown_until) = entry.cooldown_until {
                let now = Instant::now();
                if now < cooldown_until {
                    let remaining = cooldown_until.duration_since(now);
                    return Err(remaining.as_secs().saturating_add(1)); // Round up
                }
            }
        }

        Ok(())
    }

    pub fn record_success(&self, username: &str) {
        let normalized = normalize_username(username);
        self.attempts.remove(&normalized);
    }

    pub fn record_failure(&self, username: &str) {
        let normalized = normalize_username(username);
        let now = Instant::now();

        self.attempts
            .entry(normalized)
            .and_modify(|state| {
                state.consecutive_failures += 1;
                state.last_failure_at = now;
                state.cooldown_until = compute_cooldown_until(state.consecutive_failures, now);
            })
            .or_insert_with(|| FailureState {
                consecutive_failures: 1,
                last_failure_at: now,
                cooldown_until: None, // First failure: no cooldown
            });
    }

    pub fn evict_stale(&self, max_age: Duration) {
        let now = Instant::now();
        self.attempts
            .retain(|_, state| now.duration_since(state.last_failure_at) < max_age);
    }

    pub fn enforce_max_entries(&self, max_entries: usize) {
        if self.attempts.len() <= max_entries {
            return;
        }

        // Collect all entries with their timestamps
        let mut entries: Vec<(String, Instant)> = self
            .attempts
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().last_failure_at))
            .collect();

        entries.sort_by_key(|(_, timestamp)| *timestamp);

        let to_remove = entries.len().saturating_sub(max_entries);
        for (username, _) in entries.iter().take(to_remove) {
            self.attempts.remove(username);
        }
    }

    pub fn len(&self) -> usize {
        self.attempts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.attempts.is_empty()
    }

    #[cfg(test)]
    pub fn get_state(&self, username: &str) -> Option<(u32, Option<Instant>)> {
        let normalized = normalize_username(username);
        self.attempts
            .get(&normalized)
            .map(|entry| (entry.consecutive_failures, entry.cooldown_until))
    }
}

impl Default for LoginAttemptTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_username(username: &str) -> String {
    username.trim().to_lowercase()
}

fn compute_cooldown_until(consecutive_failures: u32, now: Instant) -> Option<Instant> {
    let cooldown_secs = match consecutive_failures {
        0..=3 => return None, // First 3 failures: no cooldown
        4 => 5,
        5 => 15,
        6 => 30,
        7 => 60,
        8 => 120,
        9 => 300,
        _ => 900, // 10+ failures: 15-minute cap
    };

    Some(now + Duration::from_secs(cooldown_secs))
}

pub fn spawn_reaper_task(
    tracker: LoginAttemptTracker,
    interval: Duration,
    max_age: Duration,
    max_entries: usize,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;

            tracker.evict_stale(max_age);
            tracker.enforce_max_entries(max_entries);

            let count = tracker.len();
            if count > 0 {
                tracing::debug!("Lockout tracker: {} active entries after eviction", count);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_first_three_failures_no_cooldown() {
        let tracker = LoginAttemptTracker::new();
        let username = "testuser";

        // First 3 failures should not trigger cooldown
        for i in 1..=3 {
            tracker.record_failure(username);
            assert!(
                tracker.check_allowed(username).is_ok(),
                "Failure {} should not trigger cooldown",
                i
            );
        }

        // Check state
        let (failures, cooldown) = tracker.get_state(username).unwrap();
        assert_eq!(failures, 3);
        assert!(cooldown.is_none());
    }

    #[test]
    fn test_fourth_failure_triggers_cooldown() {
        let tracker = LoginAttemptTracker::new();
        let username = "testuser";

        // 3 failures: OK
        for _ in 0..3 {
            tracker.record_failure(username);
        }

        // 4th failure: cooldown kicks in
        tracker.record_failure(username);
        let result = tracker.check_allowed(username);
        assert!(result.is_err(), "4th failure should trigger cooldown");

        let retry_after = result.unwrap_err();
        // Should be approximately 5 seconds (allow some slack)
        assert!((4..=6).contains(&retry_after));
    }

    #[test]
    fn test_exponential_backoff_schedule() {
        let tracker = LoginAttemptTracker::new();
        let username = "testuser";

        // Expected cooldowns (in seconds) by failure count
        let expected = vec![
            (1, None),
            (2, None),
            (3, None),
            (4, Some(5)),
            (5, Some(15)),
            (6, Some(30)),
            (7, Some(60)),
            (8, Some(120)),
            (9, Some(300)),
            (10, Some(900)),
            (11, Some(900)), // Cap at 15 minutes
        ];

        for (failures, expected_cooldown) in expected {
            // Reset tracker for clean test
            tracker.attempts.clear();

            // Record N failures
            for _ in 0..failures {
                tracker.record_failure(username);
            }

            let (recorded_failures, cooldown_until) = tracker.get_state(username).unwrap();
            assert_eq!(recorded_failures, failures);

            match expected_cooldown {
                None => {
                    assert!(
                        cooldown_until.is_none(),
                        "Failure {} should have no cooldown",
                        failures
                    );
                    assert!(tracker.check_allowed(username).is_ok());
                }
                Some(expected_secs) => {
                    assert!(
                        cooldown_until.is_some(),
                        "Failure {} should have cooldown",
                        failures
                    );
                    let result = tracker.check_allowed(username);
                    assert!(result.is_err());
                    let retry_after = result.unwrap_err();
                    // Allow 1 second slack for rounding and test timing
                    let expected_secs_u64: u64 = expected_secs;
                    assert!(
                        retry_after >= expected_secs_u64.saturating_sub(1)
                            && retry_after <= expected_secs_u64 + 1,
                        "Failure {}: expected ~{}s cooldown, got {}s",
                        failures,
                        expected_secs,
                        retry_after
                    );
                }
            }
        }
    }

    #[test]
    fn test_success_resets_counter() {
        let tracker = LoginAttemptTracker::new();
        let username = "testuser";

        // Record 5 failures (should trigger 15-second cooldown)
        for _ in 0..5 {
            tracker.record_failure(username);
        }

        assert!(tracker.check_allowed(username).is_err());

        // Record success
        tracker.record_success(username);

        // Counter should be reset
        assert!(tracker.check_allowed(username).is_ok());
        assert!(tracker.get_state(username).is_none());
    }

    #[test]
    fn test_cooldown_expires() {
        let tracker = LoginAttemptTracker::new();
        let username = "testuser";

        // Record 4 failures (5-second cooldown)
        for _ in 0..4 {
            tracker.record_failure(username);
        }

        // Should be in cooldown
        assert!(tracker.check_allowed(username).is_err());

        // Wait for cooldown to expire (6 seconds to be safe)
        thread::sleep(Duration::from_secs(6));

        // Should be allowed now
        assert!(tracker.check_allowed(username).is_ok());
    }

    #[test]
    fn test_username_normalization() {
        let tracker = LoginAttemptTracker::new();

        // Record failures with different case variations
        tracker.record_failure("TestUser");
        tracker.record_failure("TESTUSER");
        tracker.record_failure(" testuser ");

        // All variations should map to the same entry
        let (failures, _) = tracker.get_state("testuser").unwrap();
        assert_eq!(failures, 3);

        // Check with different case
        assert!(tracker.check_allowed("TeStUsEr").is_ok());
    }

    #[test]
    fn test_evict_stale() {
        let tracker = LoginAttemptTracker::new();

        // Record a failure for user1
        tracker.record_failure("user1");

        // Manually set last_failure_at to 31 minutes ago
        if let Some(mut entry) = tracker.attempts.get_mut("user1") {
            entry.last_failure_at = Instant::now() - Duration::from_secs(31 * 60);
        }

        // Record a recent failure for user2
        tracker.record_failure("user2");

        assert_eq!(tracker.len(), 2);

        // Evict entries older than 30 minutes
        tracker.evict_stale(Duration::from_secs(30 * 60));

        // user1 should be evicted, user2 should remain
        assert_eq!(tracker.len(), 1);
        assert!(tracker.get_state("user1").is_none());
        assert!(tracker.get_state("user2").is_some());
    }

    #[test]
    fn test_enforce_max_entries() {
        let tracker = LoginAttemptTracker::new();
        let max_entries = 5;

        // Add 10 entries
        for i in 0..10 {
            tracker.record_failure(&format!("user{}", i));
            // Small delay to ensure different timestamps
            thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(tracker.len(), 10);

        // Enforce cap
        tracker.enforce_max_entries(max_entries);

        // Should only have 5 entries left
        assert_eq!(tracker.len(), max_entries);

        // The oldest entries (user0-user4) should be removed
        assert!(tracker.get_state("user0").is_none());
        assert!(tracker.get_state("user4").is_none());

        // The newest entries (user5-user9) should remain
        assert!(tracker.get_state("user5").is_some());
        assert!(tracker.get_state("user9").is_some());
    }

    #[test]
    fn test_different_usernames_are_independent() {
        let tracker = LoginAttemptTracker::new();

        // User1: 5 failures (15-second cooldown)
        for _ in 0..5 {
            tracker.record_failure("user1");
        }

        // User2: 3 failures (no cooldown)
        for _ in 0..3 {
            tracker.record_failure("user2");
        }

        // User1 should be locked out
        assert!(tracker.check_allowed("user1").is_err());

        // User2 should be OK
        assert!(tracker.check_allowed("user2").is_ok());
    }

    #[test]
    fn test_attempt_during_cooldown_does_not_check_password() {
        let tracker = LoginAttemptTracker::new();
        let username = "testuser";

        // Trigger cooldown (4 failures)
        for _ in 0..4 {
            tracker.record_failure(username);
        }

        // Get initial failure count
        let (initial_failures, _) = tracker.get_state(username).unwrap();
        assert_eq!(initial_failures, 4);

        // Attempt during cooldown
        let result = tracker.check_allowed(username);
        assert!(result.is_err());

        // Failure count should NOT increase (we never got to check the password)
        let (failures_after, _) = tracker.get_state(username).unwrap();
        assert_eq!(failures_after, 4);
    }

    #[test]
    fn test_nonexistent_username_is_tracked() {
        let tracker = LoginAttemptTracker::new();
        let username = "nonexistent_user_12345";

        // Should be allowed initially
        assert!(tracker.check_allowed(username).is_ok());

        // Record failures for nonexistent user
        for _ in 0..5 {
            tracker.record_failure(username);
        }

        // Should now be rate-limited
        assert!(tracker.check_allowed(username).is_err());
    }
}
