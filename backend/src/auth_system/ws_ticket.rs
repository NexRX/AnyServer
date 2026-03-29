use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use dashmap::DashMap;
use rand::RngCore;
use uuid::Uuid;

use crate::types::Role;

const TICKET_LIFETIME: Duration = Duration::from_secs(30);
const MAX_TICKETS_PER_USER: usize = 10;

#[derive(Debug, Clone)]
pub struct WsTicket {
    pub user_id: Uuid,
    pub role: Role,
    pub created_at: Instant,
    pub scope: Option<String>,
}

impl WsTicket {
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > TICKET_LIFETIME
    }
}

#[derive(Clone)]
pub struct WsTicketStore {
    tickets: Arc<DashMap<String, WsTicket>>,
    /// Secondary per-user counter for O(1) limit checks in `mint()`.
    user_counts: Arc<DashMap<Uuid, usize>>,
}

impl WsTicketStore {
    pub fn new() -> Self {
        Self {
            tickets: Arc::new(DashMap::new()),
            user_counts: Arc::new(DashMap::new()),
        }
    }

    pub fn mint(&self, user_id: Uuid, role: Role, scope: Option<String>) -> Option<String> {
        // O(1) per-user count check via the secondary counter map.
        let current_count = self
            .user_counts
            .get(&user_id)
            .map(|r| *r.value())
            .unwrap_or(0);

        if current_count >= MAX_TICKETS_PER_USER {
            tracing::warn!(
                "User {} has {} outstanding tickets, rejecting new ticket request",
                user_id,
                current_count
            );
            return None;
        }

        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);

        let ticket_str = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);

        let ticket = WsTicket {
            user_id,
            role,
            created_at: Instant::now(),
            scope: scope.clone(),
        };

        self.tickets.insert(ticket_str.clone(), ticket);

        // Increment the per-user counter.
        self.user_counts
            .entry(user_id)
            .and_modify(|c| *c += 1)
            .or_insert(1);

        tracing::debug!(
            "Minted WebSocket ticket for user {} (scope: {:?})",
            user_id,
            scope
        );

        Some(ticket_str)
    }

    pub fn redeem(
        &self,
        ticket_str: &str,
        requested_scope: Option<&str>,
    ) -> Result<WsTicket, String> {
        let (_, ticket) = self
            .tickets
            .remove(ticket_str)
            .ok_or_else(|| "Invalid or expired ticket".to_string())?;

        // Decrement the per-user counter now that the ticket has been
        // removed from the map.  The `remove` above returns `None` if the
        // reaper already evicted it, so we only decrement when we were the
        // one to actually remove the entry — no double-decrement.
        self.decrement_user_count(ticket.user_id);

        if ticket.is_expired() {
            tracing::debug!("WebSocket ticket {} is expired", ticket_str);
            return Err("Ticket has expired".to_string());
        }

        if let Some(required_scope) = &ticket.scope {
            match requested_scope {
                Some(actual_scope) if actual_scope == required_scope => {}
                Some(actual_scope) => {
                    tracing::warn!(
                        "WebSocket ticket scope mismatch: ticket={}, requested={}",
                        required_scope,
                        actual_scope
                    );
                    return Err("Ticket scope does not match requested endpoint".to_string());
                }
                None => {
                    tracing::warn!(
                        "WebSocket ticket has scope {} but no scope was provided",
                        required_scope
                    );
                    return Err("Ticket requires a scope but none was provided".to_string());
                }
            }
        }

        tracing::debug!(
            "Redeemed WebSocket ticket for user {} (scope: {:?})",
            ticket.user_id,
            ticket.scope
        );

        Ok(ticket)
    }

    pub fn evict_expired(&self) -> usize {
        // Collect the user ids of expired tickets so we can update counters.
        let mut expired_users: Vec<Uuid> = Vec::new();

        self.tickets.retain(|_, ticket| {
            if ticket.is_expired() {
                expired_users.push(ticket.user_id);
                false
            } else {
                true
            }
        });

        // Decrement per-user counters for each evicted ticket.
        for user_id in &expired_users {
            self.decrement_user_count(*user_id);
        }

        let evicted = expired_users.len();
        if evicted > 0 {
            tracing::debug!("Evicted {} expired WebSocket tickets", evicted);
        }

        evicted
    }

    /// Decrement the per-user ticket counter, removing the entry entirely
    /// when it reaches zero to prevent unbounded growth.
    fn decrement_user_count(&self, user_id: Uuid) {
        // Use the entry API for an atomic read-modify-write.
        if let Some(mut entry) = self.user_counts.get_mut(&user_id) {
            *entry = entry.saturating_sub(1);
            if *entry == 0 {
                // Drop the mutable ref before removing to avoid deadlock
                // within the same DashMap shard.
                drop(entry);
                // Re-check under remove to handle the race where another
                // thread incremented between our drop and this remove.
                self.user_counts.remove_if(&user_id, |_, v| *v == 0);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.tickets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tickets.is_empty()
    }
}

impl Default for WsTicketStore {
    fn default() -> Self {
        Self::new()
    }
}

pub fn spawn_reaper_task(store: WsTicketStore, interval: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;
            store.evict_expired();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mint_ticket() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();
        let role = Role::Admin;

        let ticket = store.mint(user_id, role, None);
        assert!(ticket.is_some());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_mint_with_scope() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();
        let role = Role::User;
        let scope = Some("/api/ws/console/abc123".to_string());

        let ticket = store.mint(user_id, role, scope.clone()).unwrap();
        assert_eq!(store.len(), 1);

        let data = store.tickets.get(&ticket).unwrap();
        assert_eq!(data.user_id, user_id);
        assert_eq!(data.role, role);
        assert_eq!(data.scope, scope);
    }

    #[test]
    fn test_redeem_valid_ticket() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();
        let role = Role::User;

        let ticket_str = store.mint(user_id, role, None).unwrap();
        assert_eq!(store.len(), 1);

        let result = store.redeem(&ticket_str, None);
        assert!(result.is_ok());
        let ticket = result.unwrap();
        assert_eq!(ticket.user_id, user_id);
        assert_eq!(ticket.role, role);

        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_redeem_ticket_twice_fails() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();
        let ticket_str = store.mint(user_id, Role::User, None).unwrap();

        let result1 = store.redeem(&ticket_str, None);
        assert!(result1.is_ok());

        let result2 = store.redeem(&ticket_str, None);
        assert!(result2.is_err());
    }

    #[test]
    fn test_redeem_with_matching_scope() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();
        let scope = Some("/api/ws/console/server123".to_string());

        let ticket_str = store.mint(user_id, Role::User, scope.clone()).unwrap();

        let result = store.redeem(&ticket_str, Some("/api/ws/console/server123"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_redeem_with_mismatched_scope() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();
        let scope = Some("/api/ws/console/server123".to_string());

        let ticket_str = store.mint(user_id, Role::User, scope).unwrap();

        let result = store.redeem(&ticket_str, Some("/api/ws/console/other-server"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("scope"));
    }

    #[test]
    fn test_redeem_scoped_ticket_without_scope_fails() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();
        let scope = Some("/api/ws/console/server123".to_string());

        let ticket_str = store.mint(user_id, Role::User, scope).unwrap();

        let result = store.redeem(&ticket_str, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("scope"));
    }

    #[test]
    fn test_expired_ticket() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();

        let ticket_str = "expired_ticket".to_string();
        let ticket = WsTicket {
            user_id,
            role: Role::User,
            created_at: Instant::now() - Duration::from_secs(31), // Expired
            scope: None,
        };
        store.tickets.insert(ticket_str.clone(), ticket);
        // Manually insert the user count to match.
        store.user_counts.insert(user_id, 1);

        let result = store.redeem(&ticket_str, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expired"));
    }

    #[test]
    fn test_evict_expired() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();

        store.mint(user_id, Role::User, None);

        let expired_ticket = WsTicket {
            user_id,
            role: Role::User,
            created_at: Instant::now() - Duration::from_secs(31),
            scope: None,
        };
        store.tickets.insert("expired".to_string(), expired_ticket);
        // Manually bump the user count for the manually inserted ticket.
        self::bump_user_count(&store, user_id);

        assert_eq!(store.len(), 2);

        let evicted = store.evict_expired();
        assert_eq!(evicted, 1);
        assert_eq!(store.len(), 1);

        // The user should still have 1 outstanding ticket (the non-expired one).
        let count = store
            .user_counts
            .get(&user_id)
            .map(|r| *r.value())
            .unwrap_or(0);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_max_tickets_per_user() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();

        for _ in 0..MAX_TICKETS_PER_USER {
            let result = store.mint(user_id, Role::User, None);
            assert!(result.is_some());
        }

        let result = store.mint(user_id, Role::User, None);
        assert!(result.is_none());
        assert_eq!(store.len(), MAX_TICKETS_PER_USER);
    }

    #[test]
    fn test_different_users_independent_limits() {
        let store = WsTicketStore::new();
        let user1 = Uuid::new_v4();
        let user2 = Uuid::new_v4();

        for _ in 0..MAX_TICKETS_PER_USER {
            store.mint(user1, Role::User, None);
        }

        let result = store.mint(user2, Role::User, None);
        assert!(result.is_some());
        assert_eq!(store.len(), MAX_TICKETS_PER_USER + 1);
    }

    #[test]
    fn test_ticket_is_url_safe() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();
        let ticket = store.mint(user_id, Role::User, None).unwrap();

        assert!(ticket
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn test_counter_cleaned_up_after_all_redeemed() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();

        let t1 = store.mint(user_id, Role::User, None).unwrap();
        let t2 = store.mint(user_id, Role::User, None).unwrap();

        assert_eq!(store.user_counts.get(&user_id).map(|r| *r.value()), Some(2));

        store.redeem(&t1, None).unwrap();
        assert_eq!(store.user_counts.get(&user_id).map(|r| *r.value()), Some(1));

        store.redeem(&t2, None).unwrap();
        // Counter entry should be cleaned up when it reaches zero.
        assert!(store.user_counts.get(&user_id).is_none());
    }

    #[test]
    fn test_counter_cleaned_up_after_eviction() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();

        // Insert an already-expired ticket manually.
        let expired_ticket = WsTicket {
            user_id,
            role: Role::User,
            created_at: Instant::now() - Duration::from_secs(31),
            scope: None,
        };
        store
            .tickets
            .insert("expired1".to_string(), expired_ticket.clone());
        store.tickets.insert("expired2".to_string(), expired_ticket);
        store.user_counts.insert(user_id, 2);

        let evicted = store.evict_expired();
        assert_eq!(evicted, 2);
        assert_eq!(store.len(), 0);

        // Counter should be cleaned up.
        assert!(store.user_counts.get(&user_id).is_none());
    }

    #[test]
    fn test_mint_allowed_after_redeem_frees_slot() {
        let store = WsTicketStore::new();
        let user_id = Uuid::new_v4();

        let mut tickets = Vec::new();
        for _ in 0..MAX_TICKETS_PER_USER {
            tickets.push(store.mint(user_id, Role::User, None).unwrap());
        }

        // At the limit — minting should fail.
        assert!(store.mint(user_id, Role::User, None).is_none());

        // Redeem one ticket.
        store.redeem(&tickets.pop().unwrap(), None).unwrap();

        // Now minting should succeed again.
        assert!(store.mint(user_id, Role::User, None).is_some());
        assert_eq!(store.len(), MAX_TICKETS_PER_USER);
    }

    /// Test helper: bump the user count by 1 (for manually inserted tickets).
    fn bump_user_count(store: &WsTicketStore, user_id: Uuid) {
        store
            .user_counts
            .entry(user_id)
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }
}
