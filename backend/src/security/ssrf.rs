//! SSRF protection — shared by import proxy and pipeline executors.
//!
//! In addition to the early-reject `check_url_not_private()` helper, this
//! module exposes [`SsrfSafeResolver`] — a custom [`reqwest::dns::Resolve`]
//! implementation that filters out private/internal IPs at the transport
//! layer.  Plugging it into the shared `reqwest::Client` via
//! `ClientBuilder::dns_resolver()` closes the DNS-rebinding window: even if
//! a hostname resolves to a public IP during the pre-flight check and then
//! switches to `127.0.0.1` by the time reqwest connects, the resolver will
//! reject the private address before a TCP connection is ever made.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

pub fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64 // CGNAT
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // ULA
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local
                || v6.to_ipv4_mapped().is_some_and(|v4| {
                    v4.is_loopback()
                        || v4.is_private()
                        || v4.is_link_local()
                        || v4.is_unspecified()
                })
        }
    }
}

pub fn check_url_not_private(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

    let host = parsed.host_str().ok_or("URL has no host")?;
    let port = parsed.port_or_known_default().unwrap_or(443);

    let addr_str = format!("{}:{}", host, port);
    let addrs: Vec<_> = addr_str
        .to_socket_addrs()
        .map_err(|e| format!("Failed to resolve '{}': {}", host, e))?
        .collect();

    if addrs.is_empty() {
        return Err(format!("Could not resolve hostname '{}'", host));
    }

    for addr in &addrs {
        if is_private_ip(&addr.ip()) {
            return Err(format!(
                "URL resolves to private/internal IP address ({}) — request blocked for security. \
                 Only public URLs are allowed.",
                addr.ip()
            ));
        }
    }

    Ok(())
}

// ── Transport-layer SSRF-safe DNS resolver ──────────────────────────────

/// A [`reqwest::dns::Resolve`] implementation that performs standard DNS
/// resolution and then strips any addresses that point to private or
/// internal networks.  If *all* resolved addresses are private the
/// resolution fails with an error, preventing the HTTP client from ever
/// opening a connection to an internal host.
///
/// # Usage
///
/// ```rust,ignore
/// let client = reqwest::Client::builder()
///     .dns_resolver(Arc::new(SsrfSafeResolver::default()))
///     .build()
///     .expect("failed to build HTTP client");
/// ```
#[derive(Debug, Default, Clone)]
pub struct SsrfSafeResolver;

impl Resolve for SsrfSafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            // Perform standard blocking DNS resolution on the Tokio blocking
            // thread-pool so we don't block the async runtime.
            let host = name.as_str().to_owned();
            let addrs: Vec<SocketAddr> =
                tokio::task::spawn_blocking(move || -> std::io::Result<Vec<SocketAddr>> {
                    // Resolve with port 0 — reqwest replaces the port later.
                    let iter = (host.as_str(), 0u16).to_socket_addrs()?;
                    Ok(iter.collect())
                })
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

            // Filter out private / internal IPs.
            let safe: Vec<SocketAddr> = addrs
                .into_iter()
                .filter(|a| !is_private_ip(&a.ip()))
                .collect();

            if safe.is_empty() {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "DNS resolution returned only private/internal IP addresses — \
                     request blocked (SSRF protection)",
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }

            let addrs: Addrs = Box::new(safe.into_iter());
            Ok(addrs)
        })
    }
}

/// Build a [`reqwest::Client`] that is hardened against SSRF / DNS-rebinding
/// attacks.  All DNS lookups go through [`SsrfSafeResolver`], which rejects
/// private IPs at the transport layer.
pub fn build_ssrf_safe_client() -> reqwest::ClientBuilder {
    reqwest::Client::builder().dns_resolver(Arc::new(SsrfSafeResolver))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_v4_is_private() {
        assert!(is_private_ip(&"127.0.0.1".parse().unwrap()));
        assert!(is_private_ip(&"127.255.255.255".parse().unwrap()));
    }

    #[test]
    fn loopback_v6_is_private() {
        assert!(is_private_ip(&"::1".parse().unwrap()));
    }

    #[test]
    fn rfc1918_ranges_are_private() {
        assert!(is_private_ip(&"10.0.0.1".parse().unwrap()));
        assert!(is_private_ip(&"10.255.255.255".parse().unwrap()));
        assert!(is_private_ip(&"172.16.0.1".parse().unwrap()));
        assert!(is_private_ip(&"172.31.255.255".parse().unwrap()));
        assert!(is_private_ip(&"192.168.0.1".parse().unwrap()));
        assert!(is_private_ip(&"192.168.255.255".parse().unwrap()));
    }

    #[test]
    fn link_local_v4_is_private() {
        assert!(is_private_ip(&"169.254.1.1".parse().unwrap()));
    }

    #[test]
    fn broadcast_is_private() {
        assert!(is_private_ip(&"255.255.255.255".parse().unwrap()));
    }

    #[test]
    fn unspecified_is_private() {
        assert!(is_private_ip(&"0.0.0.0".parse().unwrap()));
        assert!(is_private_ip(&"::".parse().unwrap()));
    }

    #[test]
    fn cgnat_is_private() {
        assert!(is_private_ip(&"100.64.0.1".parse().unwrap()));
        assert!(is_private_ip(&"100.127.255.255".parse().unwrap()));
        // Just outside CGNAT range
        assert!(!is_private_ip(&"100.128.0.1".parse().unwrap()));
    }

    #[test]
    fn ipv6_ula_is_private() {
        assert!(is_private_ip(&"fc00::1".parse().unwrap()));
        assert!(is_private_ip(&"fdff::1".parse().unwrap()));
    }

    #[test]
    fn ipv6_link_local_is_private() {
        assert!(is_private_ip(&"fe80::1".parse().unwrap()));
    }

    #[test]
    fn public_ips_are_not_private() {
        assert!(!is_private_ip(&"8.8.8.8".parse().unwrap()));
        assert!(!is_private_ip(&"1.1.1.1".parse().unwrap()));
        assert!(!is_private_ip(&"93.184.216.34".parse().unwrap()));
        assert!(!is_private_ip(&"2606:4700::1111".parse().unwrap()));
    }

    #[test]
    fn ipv4_mapped_v6_private_is_detected() {
        // ::ffff:127.0.0.1
        assert!(is_private_ip(&"::ffff:127.0.0.1".parse().unwrap()));
        // ::ffff:10.0.0.1
        assert!(is_private_ip(&"::ffff:10.0.0.1".parse().unwrap()));
        // ::ffff:192.168.1.1
        assert!(is_private_ip(&"::ffff:192.168.1.1".parse().unwrap()));
    }

    #[test]
    fn ipv4_mapped_v6_public_is_not_private() {
        assert!(!is_private_ip(&"::ffff:8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn blocks_localhost_url() {
        let result = check_url_not_private("http://localhost:8080/secret");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("private"));
    }

    #[test]
    fn blocks_private_ip_url() {
        let result = check_url_not_private("http://192.168.1.1/admin");
        assert!(result.is_err());
    }

    #[test]
    fn blocks_loopback_ip_url() {
        let result = check_url_not_private("http://127.0.0.1:3000/api/secret");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_url_without_scheme() {
        let result = check_url_not_private("not-a-url");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_non_http_scheme() {
        let result = check_url_not_private("file:///etc/passwd");
        assert!(result.is_err());
    }

    // ── SsrfSafeResolver tests ──

    #[tokio::test]
    async fn resolver_rejects_localhost() {
        let resolver = SsrfSafeResolver;
        let name: Name = "localhost".parse().unwrap();
        let result = resolver.resolve(name).await;
        assert!(
            result.is_err(),
            "expected resolver to reject localhost, but it succeeded"
        );
    }

    #[tokio::test]
    async fn resolver_accepts_public_hostname() {
        // dns.google resolves to 8.8.8.8 / 8.8.4.4 — all public.
        let resolver = SsrfSafeResolver;
        let name: Name = "dns.google".parse().unwrap();
        let result = resolver.resolve(name).await;
        assert!(
            result.is_ok(),
            "expected resolver to accept dns.google, but got error: {:?}",
            result.err()
        );
        let addrs: Vec<SocketAddr> = result.unwrap().collect();
        assert!(!addrs.is_empty(), "expected at least one address");
        for addr in &addrs {
            assert!(
                !is_private_ip(&addr.ip()),
                "resolver returned private IP {} for dns.google",
                addr.ip()
            );
        }
    }

    #[tokio::test]
    async fn ssrf_safe_client_blocks_localhost_request() {
        let client = build_ssrf_safe_client()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("failed to build SSRF-safe client");

        let result = client
            .get("http://127.0.0.1:1/should-not-connect")
            .send()
            .await;
        assert!(
            result.is_err(),
            "expected SSRF-safe client to reject request to 127.0.0.1"
        );
    }
}
