//! # Core Logic - RPC Manager
//!
//! Generic RPC endpoint management utilities that can be used across different
//! blockchain implementations.

#![allow(dead_code)]

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
    pub fn get_endpoint(&self) -> &RpcEndpoint {
        let idx = self.current_index.fetch_add(1, Ordering::SeqCst);
        &self.endpoints[idx % self.endpoints.len()]
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
