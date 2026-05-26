//! # Core Logic - RPC Manager
//!
//! Generic RPC endpoint management utilities that can be used across different
//! blockchain implementations.

#![allow(dead_code)]

use crate::error::{CoreError, NetworkError};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tracing::warn;

/// RPC endpoint information
#[derive(Debug)]
pub struct RpcEndpoint {
    pub url: String,
    pub chain_id: u64,
    pub last_latency_ms: AtomicU64,
    pub failure_count: AtomicU64,
    pub healthy: AtomicBool,
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
        }
    }

    /// Get current latency in milliseconds
    pub fn latency_ms(&self) -> u64 {
        self.last_latency_ms.load(Ordering::SeqCst)
    }

    /// Check if endpoint is healthy
    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }

    /// Get failure count
    pub fn failures(&self) -> u64 {
        self.failure_count.load(Ordering::SeqCst)
    }
}

/// Health status of an RPC endpoint
#[derive(Debug, Clone)]
pub struct RpcHealthStatus {
    pub url: String,
    pub latency_ms: u64,
    pub healthy: bool,
    pub failure_count: u64,
}

/// Manager for multiple RPC endpoints with health checking and failover.
/// This is a generic manager that doesn't depend on specific chain types.
#[derive(Debug)]
pub struct RpcManager {
    chain_id: u64,
    endpoints: Vec<RpcEndpoint>,
    current_index: AtomicUsize,
    _latency_history: Mutex<Vec<(String, u64)>>,
}

impl RpcManager {
    /// Create a new RPC manager with the given chain ID and URLs
    pub fn new(chain_id: u64, urls: &[String]) -> Self {
        let endpoints: Vec<RpcEndpoint> = urls
            .iter()
            .map(|url| RpcEndpoint::new(url.clone(), chain_id))
            .collect();

        Self {
            chain_id,
            endpoints,
            current_index: AtomicUsize::new(0),
            _latency_history: Mutex::new(Vec::new()),
        }
    }

    /// Get the chain ID
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Get the next endpoint using round-robin selection
    ///
    /// # Errors
    ///
    /// Returns `Err(CoreError::Network(NetworkError::NoEndpoints(chain_id)))` if no endpoints are configured.
    pub fn get_endpoint(&self) -> Result<&RpcEndpoint, CoreError> {
        if self.endpoints.is_empty() {
            return Err(CoreError::Network(NetworkError::NoEndpoints(self.chain_id)));
        }
        let idx = self.current_index.fetch_add(1, Ordering::SeqCst);
        Ok(&self.endpoints[idx % self.endpoints.len()])
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
                break;
            }
        }
    }

    /// Record a failed request for an endpoint
    pub fn record_failure(&self, url: &str) {
        for endpoint in &self.endpoints {
            if endpoint.url == url {
                let failures = endpoint.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
                if failures >= 3 {
                    endpoint.healthy.store(false, Ordering::SeqCst);
                    warn!(
                        "Marking RPC {} as unhealthy after {} failures",
                        url, failures
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
                    endpoint.failure_count.fetch_add(1, Ordering::SeqCst);
                } else {
                    endpoint.failure_count.store(0, Ordering::SeqCst);
                }
                break;
            }
        }
    }

    /// Get all health statuses
    pub fn health_status(&self) -> Vec<RpcHealthStatus> {
        self.endpoints
            .iter()
            .map(|e| RpcHealthStatus {
                url: e.url.clone(),
                latency_ms: e.latency_ms(),
                healthy: e.is_healthy(),
                failure_count: e.failures(),
            })
            .collect()
    }
}

/// Simple health checker that can be extended for different chain types
pub struct RpcHealthChecker {
    request_timeout: Duration,
}

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
    fn test_rpc_endpoint_debug() {
        let ep = RpcEndpoint::new("https://test.rpc".into(), 5);
        let debug = format!("{:?}", ep);
        assert!(debug.contains("test.rpc"));
        assert!(debug.contains("5"));
    }

    #[test]
    fn test_rpc_health_checker_new() {
        let checker = RpcHealthChecker::new(5000);
        assert_eq!(checker.timeout().as_millis(), 5000);
    }

    #[test]
    fn test_rpc_health_checker_default() {
        let checker = RpcHealthChecker::default();
        assert_eq!(checker.timeout().as_millis(), 30000);
    }

    #[test]
    fn test_rpc_endpoint_latency_tracking() {
        let ep = RpcEndpoint::new("https://eth.rpc".into(), 1);
        // Initially 0
        assert_eq!(ep.latency_ms(), 0);
        // Latency is set via RpcManager methods, not directly on endpoint
        // Just verify the accessor works
    }

    #[test]
    fn test_rpc_endpoint_health_tracking() {
        let ep = RpcEndpoint::new("https://eth.rpc".into(), 1);
        assert!(ep.is_healthy());
    }

    // ---- RpcManager ----

    #[test]
    fn test_rpc_manager_new_with_multiple_urls() {
        let urls = vec!["https://rpc1.com".into(), "https://rpc2.com".into()];
        let mgr = RpcManager::new(1, &urls);
        assert_eq!(mgr.endpoints_count(), 2);
        assert_eq!(mgr.chain_id(), 1);
        assert_eq!(mgr.urls(), vec!["https://rpc1.com", "https://rpc2.com"]);
        assert_eq!(mgr.healthy_count(), 2);
    }

    #[test]
    fn test_rpc_manager_get_endpoint_round_robin() {
        let urls = vec!["https://rpc1.com".into(), "https://rpc2.com".into()];
        let mgr = RpcManager::new(1, &urls);
        assert_eq!(mgr.get_endpoint().unwrap().url, "https://rpc1.com");
        assert_eq!(mgr.get_endpoint().unwrap().url, "https://rpc2.com");
        assert_eq!(mgr.get_endpoint().unwrap().url, "https://rpc1.com");
    }

    #[test]
    fn test_get_fastest_picks_lowest_latency_healthy() {
        let urls = vec!["http://slow.com".into(), "http://fast.com".into()];
        let mgr = RpcManager::new(1, &urls);
        mgr.record_latency("http://slow.com", 500);
        mgr.record_latency("http://fast.com", 50);
        let fastest = mgr.get_fastest().unwrap();
        assert_eq!(fastest.url, "http://fast.com");
    }

    #[test]
    fn test_get_fastest_skips_unhealthy() {
        let urls = vec!["http://fast-but-dead.com".into(), "http://slow.com".into()];
        let mgr = RpcManager::new(1, &urls);
        mgr.record_latency("http://fast-but-dead.com", 10);
        mgr.record_latency("http://slow.com", 500);
        // Mark fast one as unhealthy via 3 failures
        mgr.record_failure("http://fast-but-dead.com");
        mgr.record_failure("http://fast-but-dead.com");
        mgr.record_failure("http://fast-but-dead.com");
        let fastest = mgr.get_fastest().unwrap();
        assert_eq!(fastest.url, "http://slow.com");
    }

    #[test]
    fn test_get_most_reliable_picks_lowest_failures() {
        let urls = vec!["http://a.com".into(), "http://b.com".into()];
        let mgr = RpcManager::new(1, &urls);
        mgr.record_failure("http://a.com");
        mgr.record_failure("http://a.com");
        let reliable = mgr.get_most_reliable().unwrap();
        assert_eq!(reliable.url, "http://b.com");
    }

    #[test]
    fn test_get_fastest_returns_none_when_all_unhealthy() {
        let urls = vec!["http://dead1.com".into(), "http://dead2.com".into()];
        let mgr = RpcManager::new(1, &urls);
        for _ in 0..3 {
            mgr.record_failure("http://dead1.com");
            mgr.record_failure("http://dead2.com");
        }
        assert!(mgr.get_fastest().is_none());
        assert!(mgr.get_most_reliable().is_none());
    }

    #[test]
    fn test_record_failure_threshold_marks_unhealthy() {
        let urls = vec!["http://rpc.com".into()];
        let mgr = RpcManager::new(1, &urls);
        assert!(mgr.get_endpoint().unwrap().is_healthy());
        mgr.record_failure("http://rpc.com");
        mgr.record_failure("http://rpc.com");
        assert!(
            mgr.get_endpoint().unwrap().is_healthy(),
            "2 failures should still be healthy"
        );
        mgr.record_failure("http://rpc.com");
        assert!(
            !mgr.get_endpoint().unwrap().is_healthy(),
            "3 failures should mark unhealthy"
        );
    }

    #[test]
    fn test_record_success_resets_failure_count() {
        let urls = vec!["http://rpc.com".into()];
        let mgr = RpcManager::new(1, &urls);
        mgr.record_failure("http://rpc.com");
        mgr.record_failure("http://rpc.com");
        mgr.record_failure("http://rpc.com");
        assert!(!mgr.get_endpoint().unwrap().is_healthy());
        mgr.record_success("http://rpc.com");
        assert!(mgr.get_endpoint().unwrap().is_healthy());
        assert_eq!(mgr.get_endpoint().unwrap().failures(), 0);
    }

    #[test]
    fn test_record_success_unknown_url_no_panic() {
        let urls = vec!["http://rpc.com".into()];
        let mgr = RpcManager::new(1, &urls);
        mgr.record_success("http://unknown.com");
        assert_eq!(mgr.healthy_count(), 1);
    }

    #[test]
    fn test_record_failure_unknown_url_no_panic() {
        let urls = vec!["http://rpc.com".into()];
        let mgr = RpcManager::new(1, &urls);
        mgr.record_failure("http://unknown.com");
        assert_eq!(mgr.healthy_count(), 1);
    }

    #[test]
    fn test_healthy_count_all_healthy_returns_count() {
        let urls = vec![
            "http://a.com".into(),
            "http://b.com".into(),
            "http://c.com".into(),
        ];
        let mgr = RpcManager::new(1, &urls);
        assert_eq!(mgr.healthy_count(), 3);
    }

    #[test]
    fn test_healthy_count_mixed_health() {
        let urls = vec!["http://a.com".into(), "http://b.com".into()];
        let mgr = RpcManager::new(1, &urls);
        for _ in 0..3 {
            mgr.record_failure("http://a.com");
        }
        assert_eq!(mgr.healthy_count(), 1);
    }

    #[test]
    fn test_record_latency_updates_endpoint() {
        let urls = vec!["http://rpc.com".into()];
        let mgr = RpcManager::new(1, &urls);
        mgr.record_latency("http://rpc.com", 200);
        assert_eq!(mgr.get_endpoint().unwrap().latency_ms(), 200);
    }

    #[test]
    fn test_health_status_returns_snapshot() {
        let urls = vec!["http://a.com".into(), "http://b.com".into()];
        let mgr = RpcManager::new(1, &urls);
        mgr.record_failure("http://a.com");
        mgr.record_failure("http://a.com");
        mgr.record_failure("http://a.com");
        let statuses = mgr.health_status();
        assert_eq!(statuses.len(), 2);
        let a = statuses.iter().find(|s| s.url == "http://a.com").unwrap();
        assert!(!a.healthy);
        assert_eq!(a.failure_count, 3);
        let b = statuses.iter().find(|s| s.url == "http://b.com").unwrap();
        assert!(b.healthy);
        assert_eq!(b.failure_count, 0);
    }

    #[test]
    fn test_update_health_marks_endpoint() {
        let urls = vec!["http://rpc.com".into()];
        let mgr = RpcManager::new(1, &urls);
        mgr.update_health("http://rpc.com", false, 500);
        assert!(!mgr.get_endpoint().unwrap().is_healthy());
        assert_eq!(mgr.get_endpoint().unwrap().latency_ms(), 500);
        mgr.update_health("http://rpc.com", true, 0);
        assert!(mgr.get_endpoint().unwrap().is_healthy());
    }

    #[test]
    fn test_get_endpoint_empty_returns_error() {
        let urls: Vec<String> = vec![];
        let mgr = RpcManager::new(1, &urls);
        let result = mgr.get_endpoint();
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            CoreError::Network(NetworkError::NoEndpoints(chain_id)) => {
                assert_eq!(chain_id, 1);
            }
            _ => panic!("Expected NetworkError::NoEndpoints variant"),
        }
    }

    #[tokio::test]
    async fn test_concurrent_round_robin_distribution() {
        let urls = vec![
            "http://a.com".into(),
            "http://b.com".into(),
            "http://c.com".into(),
        ];
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

        // With 3 endpoints and 4000 total calls, each should get ~1333
        let expected_per = total / 3;
        let tolerance = (total as f64 * 0.05) as u64; // 5% tolerance for randomness
        assert!(
            count_a.abs_diff(expected_per) <= tolerance,
            "a: got {}, expected {} ± {}", count_a, expected_per, tolerance
        );
        assert!(
            count_b.abs_diff(expected_per) <= tolerance,
            "b: got {}, expected {} ± {}", count_b, expected_per, tolerance
        );
        assert!(
            count_c.abs_diff(expected_per) <= tolerance,
            "c: got {}, expected {} ± {}", count_c, expected_per, tolerance
        );
    }
}
