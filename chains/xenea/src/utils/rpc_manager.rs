use anyhow::{Context, Result};
use ethers::providers::{Http, Middleware, Provider};
use reqwest::Client;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{debug, warn};
use url::Url;

#[derive(Debug)]
pub struct RpcEndpoint {
    pub url: String,
    pub chain_id: u64,
    pub last_latency_ms: AtomicU64,
    pub failure_count: AtomicU64,
    pub healthy: AtomicBool,
}

impl RpcEndpoint {
    pub fn new(url: String, chain_id: u64) -> Self {
        Self {
            url,
            chain_id,
            last_latency_ms: AtomicU64::new(0),
            failure_count: AtomicU64::new(0),
            healthy: AtomicBool::new(true),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RpcHealthStatus {
    pub url: String,
    pub latency_ms: u64,
    pub healthy: bool,
    pub failure_count: u64,
}

pub struct RpcManager {
    chain_id: u64,
    endpoints: Vec<RpcEndpoint>,
    current_index: AtomicUsize,
    client: Client,
    _latency_history: Mutex<HashMap<String, Vec<Duration>>>,
}

impl RpcManager {
    pub fn new(chain_id: u64, urls: &[String]) -> Result<Self> {
        if urls.is_empty() {
            anyhow::bail!("No RPC URLs provided");
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client")?;

        let endpoints: Vec<RpcEndpoint> = urls.iter().map(|url| RpcEndpoint::new(url.clone(), chain_id)).collect();

        Ok(Self {
            chain_id,
            endpoints,
            current_index: AtomicUsize::new(0),
            client,
            _latency_history: Mutex::new(HashMap::new()),
        })
    }

    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    pub fn get_endpoint(&self) -> &RpcEndpoint {
        let idx = self.current_index.fetch_add(1, Ordering::SeqCst);
        &self.endpoints[idx % self.endpoints.len()]
    }

    pub fn get_best_endpoint(&self) -> Result<&RpcEndpoint> {
        let mut best_idx = 0;
        let mut best_latency = u64::MAX;

        for (idx, endpoint) in self.endpoints.iter().enumerate() {
            let latency = endpoint.last_latency_ms.load(Ordering::SeqCst);
            if latency < best_latency && endpoint.healthy.load(Ordering::SeqCst) {
                best_latency = latency;
                best_idx = idx;
            }
        }

        if best_latency == u64::MAX {
            anyhow::bail!("No healthy RPC endpoints available");
        }

        Ok(&self.endpoints[best_idx])
    }

    pub fn get_provider(&self) -> Result<Provider<Http>> {
        let endpoint = self.get_endpoint();
        let url: Url = endpoint.url.parse().context("Invalid RPC URL")?;
        let provider = Provider::new(Http::new_with_client(url, self.client.clone()));
        Ok(provider)
    }

    pub fn get_provider_for(&self, url: &str) -> Result<Provider<Http>> {
        let url_parsed: Url = url.parse().context("Invalid RPC URL")?;
        let provider = Provider::new(Http::new_with_client(url_parsed, self.client.clone()));
        Ok(provider)
    }

    pub async fn health_check_all(&self) -> Vec<RpcHealthStatus> {
        let mut results = Vec::new();

        for endpoint in &self.endpoints {
            let start = Instant::now();
            let healthy = self.check_endpoint(&endpoint.url).await;
            let latency_ms = start.elapsed().as_millis() as u64;

            endpoint.last_latency_ms.store(latency_ms, Ordering::SeqCst);
            endpoint.healthy.store(healthy, Ordering::SeqCst);

            let failure_count = endpoint.failure_count.load(Ordering::SeqCst);

            results.push(RpcHealthStatus {
                url: endpoint.url.clone(),
                latency_ms,
                healthy,
                failure_count,
            });

            if healthy {
                debug!("RPC {} is healthy (latency: {}ms)", endpoint.url, latency_ms);
            } else {
                warn!("RPC {} is unhealthy (latency: {}ms)", endpoint.url, latency_ms);
            }
        }

        results
    }

    async fn check_endpoint(&self, url: &str) -> bool {
        let url_parsed: Url = match url.parse() {
            Ok(u) => u,
            Err(_) => return false,
        };

        let provider = Provider::new(Http::new_with_client(url_parsed, self.client.clone()));

        match provider.get_block_number().await {
            Ok(_) => true,
            Err(e) => {
                debug!("Health check failed for {}: {}", url, e);
                false
            },
        }
    }

    pub fn record_failure(&self, url: &str) {
        for endpoint in &self.endpoints {
            if endpoint.url == url {
                endpoint.failure_count.fetch_add(1, Ordering::SeqCst);
                if endpoint.failure_count.load(Ordering::SeqCst) >= 3 {
                    endpoint.healthy.store(false, Ordering::SeqCst);
                    warn!("Marking RPC {} as unhealthy after 3 failures", url);
                }
                break;
            }
        }
    }

    pub fn record_success(&self, url: &str) {
        for endpoint in &self.endpoints {
            if endpoint.url == url {
                endpoint.failure_count.store(0, Ordering::SeqCst);
                endpoint.healthy.store(true, Ordering::SeqCst);
                break;
            }
        }
    }

    pub fn endpoints_count(&self) -> usize {
        self.endpoints.len()
    }

    pub fn healthy_count(&self) -> usize {
        self.endpoints
            .iter()
            .filter(|e| e.healthy.load(Ordering::SeqCst))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_with_multiple_urls() {
        let urls = vec!["http://rpc1.com".into(), "http://rpc2.com".into()];
        let mgr = RpcManager::new(1, &urls).unwrap();
        assert_eq!(mgr.chain_id(), 1);
        assert_eq!(mgr.endpoints_count(), 2);
    }

    #[test]
    fn test_new_empty_urls_fails() {
        let result = RpcManager::new(1, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_chain_id() {
        let mgr = RpcManager::new(137, &["http://rpc.com".into()]).unwrap();
        assert_eq!(mgr.chain_id(), 137);
    }

    #[test]
    fn test_get_endpoint_round_robin() {
        let urls = vec!["http://rpc1.com".into(), "http://rpc2.com".into()];
        let mgr = RpcManager::new(1, &urls).unwrap();
        assert_eq!(mgr.get_endpoint().url, "http://rpc1.com");
        assert_eq!(mgr.get_endpoint().url, "http://rpc2.com");
        assert_eq!(mgr.get_endpoint().url, "http://rpc1.com"); // wraps around
    }

    #[test]
    fn test_record_failure_marks_unhealthy() {
        let mgr = RpcManager::new(1, &["http://rpc.com".into()]).unwrap();
        assert_eq!(mgr.healthy_count(), 1);
        mgr.record_failure("http://rpc.com");
        assert_eq!(mgr.healthy_count(), 1); // 1 failure, not enough
        mgr.record_failure("http://rpc.com");
        assert_eq!(mgr.healthy_count(), 1); // 2 failures, still not enough
        mgr.record_failure("http://rpc.com");
        assert_eq!(mgr.healthy_count(), 0); // 3rd failure → unhealthy
    }

    #[test]
    fn test_record_success_resets_unhealthy() {
        let mgr = RpcManager::new(1, &["http://rpc.com".into()]).unwrap();
        mgr.record_failure("http://rpc.com");
        mgr.record_failure("http://rpc.com");
        mgr.record_failure("http://rpc.com"); // now unhealthy
        assert_eq!(mgr.healthy_count(), 0);
        mgr.record_success("http://rpc.com");
        assert_eq!(mgr.healthy_count(), 1); // restored
    }

    #[test]
    fn test_endpoints_count() {
        let mgr = RpcManager::new(1, &["a.com".into(), "b.com".into(), "c.com".into()]).unwrap();
        assert_eq!(mgr.endpoints_count(), 3);
    }

    #[test]
    fn test_healthy_count_mixed() {
        let urls = vec![
            "http://good.com".into(),
            "http://bad.com".into(),
            "http://ok.com".into(),
        ];
        let mgr = RpcManager::new(1, &urls).unwrap();
        mgr.record_failure("http://bad.com");
        mgr.record_failure("http://bad.com");
        mgr.record_failure("http://bad.com"); // unhealthy
        assert_eq!(mgr.healthy_count(), 2);
    }

    #[test]
    fn test_get_best_endpoint_no_healthy_errors() {
        let mgr = RpcManager::new(1, &["http://dead.com".into()]).unwrap();
        mgr.record_failure("http://dead.com");
        mgr.record_failure("http://dead.com");
        mgr.record_failure("http://dead.com");
        let result = mgr.get_best_endpoint();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No healthy"));
    }

    #[test]
    fn test_rpc_endpoint_new_defaults() {
        let ep = RpcEndpoint::new("http://test:8545".into(), 137);
        assert_eq!(ep.url, "http://test:8545");
        assert_eq!(ep.chain_id, 137);
        assert_eq!(ep.last_latency_ms.load(Ordering::SeqCst), 0);
        assert_eq!(ep.failure_count.load(Ordering::SeqCst), 0);
        assert!(ep.healthy.load(Ordering::SeqCst));
    }

    #[test]
    fn test_get_best_endpoint_picks_lowest_latency_healthy() {
        let urls = vec![
            "http://slow.com".into(),
            "http://fast.com".into(),
            "http://medium.com".into(),
        ];
        let mgr = RpcManager::new(1, &urls).unwrap();
        mgr.endpoints[0].last_latency_ms.store(500, Ordering::SeqCst);
        mgr.endpoints[1].last_latency_ms.store(50, Ordering::SeqCst);
        mgr.endpoints[2].last_latency_ms.store(200, Ordering::SeqCst);
        let best = mgr.get_best_endpoint().unwrap();
        assert_eq!(best.url, "http://fast.com");
    }

    #[test]
    fn test_get_best_endpoint_skips_unhealthy_even_if_low_latency() {
        let urls = vec!["http://fast-but-dead.com".into(), "http://slow.com".into()];
        let mgr = RpcManager::new(1, &urls).unwrap();
        mgr.endpoints[0].last_latency_ms.store(10, Ordering::SeqCst);
        mgr.endpoints[0].healthy.store(false, Ordering::SeqCst);
        mgr.endpoints[1].last_latency_ms.store(300, Ordering::SeqCst);
        let best = mgr.get_best_endpoint().unwrap();
        assert_eq!(best.url, "http://slow.com");
    }

    #[test]
    fn test_get_best_endpoint_with_single_healthy() {
        let mgr = RpcManager::new(1, &["http://only.com".into()]).unwrap();
        mgr.endpoints[0].last_latency_ms.store(100, Ordering::SeqCst);
        let best = mgr.get_best_endpoint().unwrap();
        assert_eq!(best.url, "http://only.com");
    }

    #[test]
    fn test_record_failure_unknown_url_no_panic() {
        let mgr = RpcManager::new(1, &["http://rpc.com".into()]).unwrap();
        mgr.record_failure("http://unknown.com");
        assert_eq!(mgr.healthy_count(), 1);
    }

    #[test]
    fn test_record_success_unknown_url_no_panic() {
        let mgr = RpcManager::new(1, &["http://rpc.com".into()]).unwrap();
        mgr.record_success("http://unknown.com");
        assert_eq!(mgr.healthy_count(), 1);
    }

    #[test]
    fn test_healthy_count_all_healthy_returns_count() {
        let urls = vec!["http://a.com".into(), "http://b.com".into(), "http://c.com".into()];
        let mgr = RpcManager::new(1, &urls).unwrap();
        assert_eq!(mgr.healthy_count(), 3);
    }

    #[test]
    fn test_get_best_endpoint_all_healthy_zero_latency() {
        let urls = vec!["http://a.com".into(), "http://b.com".into()];
        let mgr = RpcManager::new(1, &urls).unwrap();
        let best = mgr.get_best_endpoint().unwrap();
        assert_eq!(best.url, "http://a.com");
    }
}
