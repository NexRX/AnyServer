//! Per-IP sliding-window rate limiting middleware.
//!
//! Set `ANYSERVER_TRUSTED_PROXIES` (comma-separated IPs/CIDRs) to trust
//! `X-Forwarded-For`/`X-Real-Ip` headers from reverse proxies. If unset,
//! proxy headers are never trusted and the TCP peer address is always used.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use tower::{Layer, Service};

use crate::types::ApiError;

#[derive(Debug, Clone)]
enum TrustedSource {
    Exact(IpAddr),
    V4Cidr { network: Ipv4Addr, prefix_len: u8 },
    V6Cidr { network: Ipv6Addr, prefix_len: u8 },
}

impl TrustedSource {
    fn contains(&self, ip: &IpAddr) -> bool {
        match self {
            TrustedSource::Exact(trusted) => trusted == ip,
            TrustedSource::V4Cidr {
                network,
                prefix_len,
            } => {
                if let IpAddr::V4(v4) = ip {
                    let mask = if *prefix_len == 0 {
                        0u32
                    } else {
                        u32::MAX << (32 - prefix_len)
                    };
                    u32::from_be_bytes(v4.octets()) & mask
                        == u32::from_be_bytes(network.octets()) & mask
                } else {
                    false
                }
            }
            TrustedSource::V6Cidr {
                network,
                prefix_len,
            } => {
                if let IpAddr::V6(v6) = ip {
                    let mask = if *prefix_len == 0 {
                        0u128
                    } else {
                        u128::MAX << (128 - prefix_len)
                    };
                    u128::from_be_bytes(v6.octets()) & mask
                        == u128::from_be_bytes(network.octets()) & mask
                } else {
                    false
                }
            }
        }
    }
}

fn parse_trusted_source(s: &str) -> Option<TrustedSource> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    if let Some((addr_str, prefix_str)) = s.split_once('/') {
        let prefix_len: u8 = prefix_str.parse().ok()?;
        if let Ok(v4) = addr_str.parse::<Ipv4Addr>() {
            if prefix_len > 32 {
                return None;
            }
            return Some(TrustedSource::V4Cidr {
                network: v4,
                prefix_len,
            });
        }
        if let Ok(v6) = addr_str.parse::<Ipv6Addr>() {
            if prefix_len > 128 {
                return None;
            }
            return Some(TrustedSource::V6Cidr {
                network: v6,
                prefix_len,
            });
        }
        None
    } else {
        s.parse::<IpAddr>().ok().map(TrustedSource::Exact)
    }
}

static TRUSTED_PROXIES: OnceLock<Vec<TrustedSource>> = OnceLock::new();

fn trusted_proxies() -> &'static [TrustedSource] {
    TRUSTED_PROXIES.get_or_init(|| match std::env::var("ANYSERVER_TRUSTED_PROXIES") {
        Ok(raw) => {
            let sources: Vec<TrustedSource> =
                raw.split(',').filter_map(parse_trusted_source).collect();
            if sources.is_empty() {
                tracing::warn!(
                    "ANYSERVER_TRUSTED_PROXIES is set but contains no valid entries — \
                         proxy headers will NOT be trusted"
                );
            } else {
                tracing::info!(
                    "Rate limiter: trusting {} proxy source(s) for X-Forwarded-For",
                    sources.len()
                );
            }
            sources
        }
        Err(_) => Vec::new(),
    })
}

fn is_trusted_proxy(ip: &IpAddr) -> bool {
    let proxies = trusted_proxies();
    proxies.iter().any(|src| src.contains(ip))
}

struct WindowState {
    count: u32,
    window_start: Instant,
}

#[derive(Clone)]
pub struct RateLimiterState {
    max_requests: u32,
    window: Duration,
    counters: Arc<DashMap<IpAddr, WindowState>>,
}

impl RateLimiterState {
    fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            counters: Arc::new(DashMap::new()),
        }
    }

    fn check(&self, ip: IpAddr) -> Result<(), u64> {
        let now = Instant::now();

        let mut entry = self.counters.entry(ip).or_insert_with(|| WindowState {
            count: 0,
            window_start: now,
        });

        let state = entry.value_mut();

        if now.duration_since(state.window_start) >= self.window {
            state.count = 0;
            state.window_start = now;
        }

        if state.count >= self.max_requests {
            let elapsed = now.duration_since(state.window_start);
            let remaining = self.window.saturating_sub(elapsed);
            return Err(remaining.as_secs().max(1));
        }

        state.count += 1;
        Ok(())
    }

    pub fn evict_stale(&self) {
        let now = Instant::now();
        let threshold = self.window * 2;
        self.counters
            .retain(|_ip, state| now.duration_since(state.window_start) < threshold);
    }
}

#[derive(Clone)]
pub struct RateLimitLayer {
    state: RateLimiterState,
}

impl RateLimitLayer {
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            state: RateLimiterState::new(max_requests, window),
        }
    }

    pub fn state(&self) -> RateLimiterState {
        self.state.clone()
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            state: self.state.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    state: RateLimiterState,
}

impl<S, B> Service<Request<B>> for RateLimitService<S>
where
    S: Service<Request<B>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let ip = extract_client_ip(&req);

        let result = self.state.check(ip);

        match result {
            Ok(()) => {
                let future = self.inner.call(req);
                Box::pin(future)
            }
            Err(retry_after) => {
                let response = build_rate_limit_response(retry_after);
                Box::pin(async move { Ok(response) })
            }
        }
    }
}

fn extract_client_ip<B>(req: &Request<B>) -> IpAddr {
    let peer_ip = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip());

    let peer = match peer_ip {
        Some(ip) => ip,
        None => {
            tracing::warn!(
                "ConnectInfo<SocketAddr> not found in request extensions — \
                 falling back to 127.0.0.1. Ensure axum::serve uses \
                 into_make_service_with_connect_info::<SocketAddr>()"
            );
            return IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
        }
    };

    if !is_trusted_proxy(&peer) {
        return peer;
    }

    if let Some(forwarded) = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(first) = forwarded.split(',').next() {
            if let Ok(ip) = first.trim().parse::<IpAddr>() {
                return ip;
            }
        }
    }

    if let Some(real_ip) = req.headers().get("x-real-ip").and_then(|v| v.to_str().ok()) {
        if let Ok(ip) = real_ip.trim().parse::<IpAddr>() {
            return ip;
        }
    }

    peer
}

fn build_rate_limit_response(retry_after: u64) -> Response {
    let body = axum::Json(ApiError {
        error: format!(
            "Too many requests. Please try again in {} second{}.",
            retry_after,
            if retry_after == 1 { "" } else { "s" }
        ),
        details: None,
    });

    let mut response = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
    response.headers_mut().insert(
        "retry-after",
        axum::http::HeaderValue::from_str(&retry_after.to_string())
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("60")),
    );
    response
}

pub fn spawn_eviction_task(state: RateLimiterState, interval: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            state.evict_stale();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn localhost() -> IpAddr {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    }

    fn other_ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))
    }

    #[test]
    fn test_under_limit_succeeds() {
        let state = RateLimiterState::new(5, Duration::from_secs(60));
        for _ in 0..5 {
            assert!(state.check(localhost()).is_ok());
        }
    }

    #[test]
    fn test_over_limit_fails() {
        let state = RateLimiterState::new(3, Duration::from_secs(60));
        for _ in 0..3 {
            assert!(state.check(localhost()).is_ok());
        }
        let err = state.check(localhost());
        assert!(err.is_err());
        assert!(err.unwrap_err() >= 1);
    }

    #[test]
    fn test_different_ips_are_independent() {
        let state = RateLimiterState::new(2, Duration::from_secs(60));

        assert!(state.check(localhost()).is_ok());
        assert!(state.check(localhost()).is_ok());
        assert!(state.check(localhost()).is_err());

        // Different IP should still be allowed
        assert!(state.check(other_ip()).is_ok());
        assert!(state.check(other_ip()).is_ok());
        assert!(state.check(other_ip()).is_err());
    }

    #[test]
    fn test_window_reset() {
        // Use a very short window so it expires during the test
        let state = RateLimiterState::new(1, Duration::from_millis(10));

        assert!(state.check(localhost()).is_ok());
        assert!(state.check(localhost()).is_err());

        // Wait for the window to expire
        std::thread::sleep(Duration::from_millis(20));

        assert!(state.check(localhost()).is_ok());
    }

    #[test]
    fn test_evict_stale_removes_old_entries() {
        let state = RateLimiterState::new(5, Duration::from_millis(5));
        state.check(localhost()).unwrap();
        state.check(other_ip()).unwrap();

        assert_eq!(state.counters.len(), 2);

        // Wait for entries to become stale (2× the window)
        std::thread::sleep(Duration::from_millis(15));

        state.evict_stale();
        assert_eq!(state.counters.len(), 0);
    }

    #[test]
    fn test_evict_stale_keeps_recent_entries() {
        let state = RateLimiterState::new(5, Duration::from_secs(60));
        state.check(localhost()).unwrap();

        state.evict_stale();
        assert_eq!(state.counters.len(), 1);
    }

    // ── Trusted proxy tests ──

    #[test]
    fn test_parse_trusted_source_exact_ipv4() {
        let src = parse_trusted_source("127.0.0.1").unwrap();
        assert!(src.contains(&"127.0.0.1".parse().unwrap()));
        assert!(!src.contains(&"127.0.0.2".parse().unwrap()));
    }

    #[test]
    fn test_parse_trusted_source_exact_ipv6() {
        let src = parse_trusted_source("::1").unwrap();
        assert!(src.contains(&"::1".parse().unwrap()));
        assert!(!src.contains(&"::2".parse().unwrap()));
    }

    #[test]
    fn test_parse_trusted_source_cidr_v4() {
        let src = parse_trusted_source("10.0.0.0/8").unwrap();
        assert!(src.contains(&"10.0.0.1".parse().unwrap()));
        assert!(src.contains(&"10.255.255.255".parse().unwrap()));
        assert!(!src.contains(&"11.0.0.1".parse().unwrap()));
    }

    #[test]
    fn test_parse_trusted_source_cidr_v6() {
        let src = parse_trusted_source("fd00::/8").unwrap();
        assert!(src.contains(&"fd00::1".parse().unwrap()));
        assert!(src.contains(&"fdff::1".parse().unwrap()));
        assert!(!src.contains(&"fe00::1".parse().unwrap()));
    }

    #[test]
    fn test_parse_trusted_source_empty() {
        assert!(parse_trusted_source("").is_none());
        assert!(parse_trusted_source("  ").is_none());
    }

    #[test]
    fn test_parse_trusted_source_invalid() {
        assert!(parse_trusted_source("not-an-ip").is_none());
        assert!(parse_trusted_source("10.0.0.0/33").is_none());
    }
}
