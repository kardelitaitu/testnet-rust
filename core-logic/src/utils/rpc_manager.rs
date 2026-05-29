//! # Core Logic - RPC Manager
//!
//! Generic RPC endpoint management utilities that can be used across different
//! blockchain implementations.

use crate::error::{CoreError, NetworkError};
use chrono::Utc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use tracing::warn;

#[cfg(test)]
use std::time::Duration;

/// RPC endpoint information
#[derive(Debug)]
pub struct RpcEndpoint {
    pub url: String,
    pub chain_id: u64,
    pub last_latency_ms: AtomicU64,
    pub failure_count: AtomicU64,
    pub healthy: AtomicBool,
    pub paused_until: AtomicI64, // UNIX timestamp in seconds
    pub pause_tier: AtomicU32,
}

impl RpcEndpoint {
    /// Create a new RPC endpoint
    pub fn new(url: String, chain_id: u64) -> Self {
        Self {
            url,
            chain_id,
            last_latency_ms: AtomicU64::new(0),
            failure_count: AtomicU64::new(0),
            healthy: AtomicBool::new(true),
            paused_until: AtomicI64::new(0),
            pause_tier: AtomicU32::new(0),
        }
    }

    /// Get current latency in milliseconds
    pub fn latency_ms(&self) -> u64 {
        self.last_latency_ms.load(Ordering::SeqCst)
    }

    /// Check if endpoint is healthy and not currently paused
    pub fn is_healthy(&self) -> bool {
        // If explicitly marked unhealthy, return false
        if !self.healthy.load(Ordering::SeqCst) {
            // Check if it's paused. If pause expired, we can consider it "retryable" (healthy)
            let paused_until = self.paused_until.load(Ordering::SeqCst);
            if paused_until > 0 {
                let now = Utc::now().timestamp();
                if now >= paused_until {
                    return true;
                }
            }
            return false;
        }

        // Check for active pause
        let paused_until = self.paused_until.load(Ordering::SeqCst);
        if paused_until > 0 {
            let now = Utc::now().timestamp();
            if now < paused_until {
                return false;
            }
        }

        true
    }

    /// Get failure count
    pub fn failures(&self) -> u64 {
        self.failure_count.load(Ordering::SeqCst)
    }

    /// Get current pause tier
    pub fn pause_tier(&self) -> u32 {
        self.pause_tier.load(Ordering::SeqCst)
    }
}

/// Health status of an RPC endpoint
#[derive(Debug, Clone)]
pub struct RpcHealthStatus {
    pub url: String,
    pub latency_ms: u64,
    pub healthy: bool,
    pub failure_count: u64,
    pub paused_until: Option<i64>,
    pub pause_tier: u32,
}

/// Manager for multiple RPC endpoints with health checking and failover.
/// This is a generic manager that doesn't depend on specific chain types.
#[derive(Debug)]
pub struct RpcManager {
    chain_id: u64,
    endpoints: Vec<RpcEndpoint>,
    current_index: AtomicUsize,
}

impl RpcManager {
    /// Create a new RPC manager with the given chain ID and URLs
    pub fn new(chain_id: u64, urls: &[String]) -> Self {
        let endpoints: Vec<RpcEndpoint> = urls.iter().map(|url| RpcEndpoint::new(url.clone(), chain_id)).collect();

        Self {
            chain_id,
            endpoints,
            current_index: AtomicUsize::new(0),
        }
    }

    /// Get the chain ID
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Get the next endpoint using round-robin selection.
    /// Skips endpoints that are currently paused or unhealthy.
    ///
    /// # Errors
    ///
    /// Returns `Err(CoreError::Network(NetworkError::NoEndpoints(chain_id)))` if no endpoints are configured
    /// or all configured endpoints are currently unavailable.
    pub fn get_endpoint(&self) -> Result<&RpcEndpoint, CoreError> {
        if self.endpoints.is_empty() {
            return Err(CoreError::Network(NetworkError::NoEndpoints(self.chain_id)));
        }

        let len = self.endpoints.len();
        let start_idx = self.current_index.fetch_add(1, Ordering::SeqCst) % len;

        // Try to find a healthy endpoint starting from the next index
        for i in 0..len {
            let idx = (start_idx + i) % len;
            let endpoint = &self.endpoints[idx];
            if endpoint.is_healthy() {
                return Ok(endpoint);
            }
        }

        // Fallback: If all are "unhealthy" but some pauses might have expired or we just need ANY endpoint,
        // we return the one at the start_idx. But per user request, we should probably fail if all are bad.
        Err(CoreError::Network(NetworkError::NoEndpoints(self.chain_id)))
    }

    /// Get the fastest (lowest latency) healthy endpoint
    pub fn get_fastest(&self) -> Option<&RpcEndpoint> {
        self.endpoints
            .iter()
            .filter(|e| e.is_healthy())
            .min_by_key(|e| e.latency_ms())
    }

    /// Get the most reliable endpoint (lowest failure count)
    pub fn get_most_reliable(&self) -> Option<&RpcEndpoint> {
        self.endpoints
            .iter()
            .filter(|e| e.is_healthy())
            .min_by_key(|e| e.failures())
    }

    /// Get all endpoint URLs
    pub fn urls(&self) -> Vec<&str> {
        self.endpoints.iter().map(|e| e.url.as_str()).collect()
    }

    /// Get count of endpoints
    pub fn endpoints_count(&self) -> usize {
        self.endpoints.len()
    }

    /// Get count of healthy endpoints
    pub fn healthy_count(&self) -> usize {
        self.endpoints.iter().filter(|e| e.is_healthy()).count()
    }

    /// Record a successful request for an endpoint
    pub fn record_success(&self, url: &str) {
        for endpoint in &self.endpoints {
            if endpoint.url == url {
                endpoint.failure_count.store(0, Ordering::SeqCst);
                endpoint.healthy.store(true, Ordering::SeqCst);
                endpoint.paused_until.store(0, Ordering::SeqCst);
                endpoint.pause_tier.store(0, Ordering::SeqCst);
                break;
            }
        }
    }

    /// Record a failed request for an endpoint.
    /// Implements linear backoff: 10s * tier after 2 consecutive failures.
    pub fn record_failure(&self, url: &str) {
        for endpoint in &self.endpoints {
            if endpoint.url == url {
                let failures = endpoint.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
                if failures >= 2 {
                    let tier = endpoint.pause_tier.fetch_add(1, Ordering::SeqCst) + 1;
                    let pause_secs = 10 * tier as i64;
                    let now = Utc::now().timestamp();

                    endpoint.paused_until.store(now + pause_secs, Ordering::SeqCst);
                    endpoint.healthy.store(false, Ordering::SeqCst);

                    warn!(
                        "Marking RPC {} as unhealthy/paused for {}s (tier {}, failures {})",
                        url, pause_secs, tier, failures
                    );
                }
                break;
            }
        }
    }

    /// Record latency for an endpoint
    pub fn record_latency(&self, url: &str, latency_ms: u64) {
        for endpoint in &self.endpoints {
            if endpoint.url == url {
                endpoint.last_latency_ms.store(latency_ms, Ordering::SeqCst);
                break;
            }
        }
    }

    /// Update health status for an endpoint
    pub fn update_health(&self, url: &str, healthy: bool, latency_ms: u64) {
        for endpoint in &self.endpoints {
            if endpoint.url == url {
                endpoint.last_latency_ms.store(latency_ms, Ordering::SeqCst);
                endpoint.healthy.store(healthy, Ordering::SeqCst);
                if !healthy {
                    self.record_failure(url);
                } else {
                    self.record_success(url);
                }
                break;
            }
        }
    }

    /// Get all health statuses
    pub fn health_status(&self) -> Vec<RpcHealthStatus> {
        self.endpoints
            .iter()
            .map(|e| {
                let paused_until = e.paused_until.load(Ordering::SeqCst);
                RpcHealthStatus {
                    url: e.url.clone(),
                    latency_ms: e.latency_ms(),
                    healthy: e.is_healthy(),
                    failure_count: e.failures(),
                    paused_until: if paused_until > 0 { Some(paused_until) } else { None },
                    pause_tier: e.pause_tier(),
                }
            })
            .collect()
    }
}

/// Simple health checker that can be extended for different chain types
#[cfg(test)]
#[allow(dead_code)]
pub struct RpcHealthChecker {
    request_timeout: Duration,
}

#[cfg(test)]
#[allow(dead_code)]
impl RpcHealthChecker {
    /// Create a new health checker with timeout
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            request_timeout: Duration::from_millis(timeout_ms),
        }
    }

    /// Get the request timeout
    pub fn timeout(&self) -> Duration {
        self.request_timeout
    }
}

#[cfg(test)]
impl Default for RpcHealthChecker {
    fn default() -> Self {
        Self::new(30000) // 30 seconds default timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_endpoint_new() {
        let ep = RpcEndpoint::new("https://eth.rpc".into(), 1);
        assert_eq!(ep.url, "https://eth.rpc");
        assert_eq!(ep.chain_id, 1);
        assert!(ep.is_healthy());
        assert_eq!(ep.latency_ms(), 0);
        assert_eq!(ep.failures(), 0);
    }

    #[test]
    fn test_rpc_endpoint_pause_logic() {
        let ep = RpcEndpoint::new("https://eth.rpc".into(), 1);
        let now = Utc::now().timestamp();

        // Pause for 10 seconds
        ep.paused_until.store(now + 10, Ordering::SeqCst);
        assert!(!ep.is_healthy(), "Should be unhealthy while paused");

        // Set pause to the past
        ep.paused_until.store(now - 10, Ordering::SeqCst);
        assert!(ep.is_healthy(), "Should be healthy if pause expired");
    }

    #[test]
    fn test_rpc_manager_linear_backoff() {
        let urls = vec!["https://rpc1.com".into()];
        let mgr = RpcManager::new(1, &urls);

        // 1st failure - still healthy
        mgr.record_failure("https://rpc1.com");
        assert!(mgr.get_endpoint().is_ok());

        // 2nd failure - marks unhealthy and pauses for 10s
        mgr.record_failure("https://rpc1.com");
        assert!(mgr.get_endpoint().is_err());
        let status = &mgr.health_status()[0];
        assert_eq!(status.pause_tier, 1);
        assert!(status.paused_until.is_some());

        // 3rd failure (consecutive) - tier 2 (20s)
        mgr.record_failure("https://rpc1.com");
        let status = &mgr.health_status()[0];
        assert_eq!(status.pause_tier, 2);

        // Success - resets everything
        mgr.record_success("https://rpc1.com");
        assert!(mgr.get_endpoint().is_ok());
        let status = &mgr.health_status()[0];
        assert_eq!(status.pause_tier, 0);
        assert_eq!(status.failure_count, 0);
        assert!(status.paused_until.is_none());
    }

    #[test]
    fn test_rpc_manager_get_endpoint_skips_unhealthy() {
        let urls = vec!["https://bad.com".into(), "https://good.com".into()];
        let mgr = RpcManager::new(1, &urls);

        // Make bad.com unhealthy
        mgr.record_failure("https://bad.com");
        mgr.record_failure("https://bad.com");

        // Should always return good.com
        for _ in 0..10 {
            assert_eq!(mgr.get_endpoint().unwrap().url, "https://good.com");
        }
    }

    #[test]
    fn test_rpc_manager_get_endpoint_wraps_around() {
        let urls = vec!["https://rpc1.com".into(), "https://rpc2.com".into()];
        let mgr = RpcManager::new(1, &urls);
        assert_eq!(mgr.get_endpoint().unwrap().url, "https://rpc1.com");
        assert_eq!(mgr.get_endpoint().unwrap().url, "https://rpc2.com");
        assert_eq!(mgr.get_endpoint().unwrap().url, "https://rpc1.com");
    }

    #[test]
    fn test_get_fastest_skips_paused() {
        let urls = vec!["http://slow.com".into(), "http://fast.com".into()];
        let mgr = RpcManager::new(1, &urls);
        mgr.record_latency("http://slow.com", 500);
        mgr.record_latency("http://fast.com", 50);

        // Pause fast.com
        mgr.record_failure("http://fast.com");
        mgr.record_failure("http://fast.com");

        let fastest = mgr.get_fastest().unwrap();
        assert_eq!(fastest.url, "http://slow.com");
    }

    #[test]
    fn test_rpc_manager_recovery_after_pause() {
        let urls = vec!["https://rpc1.com".into()];
        let mgr = RpcManager::new(1, &urls);

        // Pause it
        mgr.record_failure("https://rpc1.com");
        mgr.record_failure("https://rpc1.com");
        assert!(mgr.get_endpoint().is_err());

        // Manually "fast-forward" time by setting paused_until to the past
        let now = Utc::now().timestamp();
        mgr.endpoints[0].paused_until.store(now - 1, Ordering::SeqCst);

        // Should be available again
        assert!(mgr.get_endpoint().is_ok());
        assert_eq!(mgr.get_endpoint().unwrap().url, "https://rpc1.com");
    }

    #[test]
    fn test_rpc_manager_all_paused_returns_error() {
        let urls = vec!["https://rpc1.com".into(), "https://rpc2.com".into()];
        let mgr = RpcManager::new(1, &urls);

        // Pause both
        mgr.record_failure("https://rpc1.com");
        mgr.record_failure("https://rpc1.com");
        mgr.record_failure("https://rpc2.com");
        mgr.record_failure("https://rpc2.com");

        let result = mgr.get_endpoint();
        assert!(result.is_err());
        match result.unwrap_err() {
            CoreError::Network(NetworkError::NoEndpoints(id)) => assert_eq!(id, 1),
            _ => panic!("Expected NoEndpoints error"),
        }
    }

    #[test]
    fn test_rpc_manager_backoff_progression() {
        let urls = vec!["https://rpc1.com".into()];
        let mgr = RpcManager::new(1, &urls);

        // Tier 1 (10s)
        mgr.record_failure("https://rpc1.com");
        mgr.record_failure("https://rpc1.com");
        assert_eq!(mgr.endpoints[0].pause_tier.load(Ordering::SeqCst), 1);

        // Tier 2 (20s) - another failure while paused or right after
        mgr.record_failure("https://rpc1.com");
        assert_eq!(mgr.endpoints[0].pause_tier.load(Ordering::SeqCst), 2);

        // Tier 3 (30s)
        mgr.record_failure("https://rpc1.com");
        assert_eq!(mgr.endpoints[0].pause_tier.load(Ordering::SeqCst), 3);

        // Tier 4 (40s)
        mgr.record_failure("https://rpc1.com");
        assert_eq!(mgr.endpoints[0].pause_tier.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn test_concurrent_round_robin_distribution() {
        let urls = vec!["http://a.com".into(), "http://b.com".into(), "http://c.com".into()];
        let mgr = std::sync::Arc::new(RpcManager::new(1, &urls));
        let mut handles = Vec::new();
        let calls_per_task = 500;
        let task_count = 8;

        for _ in 0..task_count {
            let mgr_clone = mgr.clone();
            handles.push(tokio::spawn(async move {
                let mut seen = Vec::new();
                for _ in 0..calls_per_task {
                    seen.push(mgr_clone.get_endpoint().unwrap().url.clone());
                }
                seen
            }));
        }

        let mut all_urls = Vec::new();
        for h in handles {
            all_urls.extend(h.await.unwrap());
        }

        let total = all_urls.len() as u64;
        assert_eq!(total, (calls_per_task * task_count) as u64);

        let count_a = all_urls.iter().filter(|u| *u == "http://a.com").count() as u64;
        let count_b = all_urls.iter().filter(|u| *u == "http://b.com").count() as u64;
        let count_c = all_urls.iter().filter(|u| *u == "http://c.com").count() as u64;

        let expected_per = total / 3;
        let tolerance = (total as f64 * 0.10) as u64; // Slightly higher tolerance for CI
        assert!(
            count_a.abs_diff(expected_per) <= tolerance,
            "a: got {}, expected {} ± {}",
            count_a,
            expected_per,
            tolerance
        );
        assert!(
            count_b.abs_diff(expected_per) <= tolerance,
            "b: got {}, expected {} ± {}",
            count_b,
            expected_per,
            tolerance
        );
        assert!(
            count_c.abs_diff(expected_per) <= tolerance,
            "c: got {}, expected {} ± {}",
            count_c,
            expected_per,
            tolerance
        );
    }
}
