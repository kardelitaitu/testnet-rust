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

        let endpoints: Vec<RpcEndpoint> = urls
            .iter()
            .map(|url| RpcEndpoint::new(url.clone(), chain_id))
            .collect();

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
                debug!(
                    "RPC {} is healthy (latency: {}ms)",
                    endpoint.url, latency_ms
                );
            } else {
                warn!(
                    "RPC {} is unhealthy (latency: {}ms)",
                    endpoint.url, latency_ms
                );
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
            }
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
