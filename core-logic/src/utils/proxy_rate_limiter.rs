use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::debug;

pub struct ProxyRateLimiter {
    inner: Arc<RwLock<HashMap<String, ProxyLimit>>>,
    max_requests_per_second: u32,
    min_interval: Duration,
}

#[derive(Debug)]
struct ProxyLimit {
    last_request: Instant,
    request_count: u32,
    window_start: Instant,
}

impl ProxyRateLimiter {
    pub fn new(max_requests_per_second: u32) -> Self {
        let min_interval = Duration::from_millis(1000 / max_requests_per_second as u64);
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            max_requests_per_second,
            min_interval,
        }
    }

    pub async fn acquire(&self, proxy_url: &str) -> bool {
        let now = Instant::now();
        let mut limits = self.inner.write().await;
        let limit = limits.entry(proxy_url.to_string()).or_insert(ProxyLimit {
            last_request: now,
            request_count: 0,
            window_start: now,
        });

        // Check if we need to reset the window
        if now.duration_since(limit.window_start) >= Duration::from_secs(1) {
            limit.request_count = 0;
            limit.window_start = now;
        }

        // Check rate limit
        if limit.request_count >= self.max_requests_per_second {
            // Calculate how long to wait
            let wait_time = Duration::from_secs(1).saturating_sub(now.duration_since(limit.window_start));
            debug!(
                "Proxy {} rate limited, wait {}ms",
                proxy_url,
                wait_time.as_millis()
            );
            return false;
        }

        // Rate limit respected
        limit.request_count += 1;
        limit.last_request = now;

        // If too fast, wait a bit
        let elapsed = now.duration_since(limit.last_request);
        if elapsed < self.min_interval {
            let sleep_time = self.min_interval - elapsed;
            std::thread::sleep(sleep_time);
        }

        true
    }

    /// Wait until rate limit allows (blocking-style for simplicity)
    pub async fn wait_until_available(&self, proxy_url: &str) {
        while !self.acquire(proxy_url).await {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub async fn reset(&self, proxy_url: &str) {
        let mut limits = self.inner.write().await;
        if let Some(limit) = limits.get_mut(proxy_url) {
            limit.request_count = 0;
            limit.window_start = Instant::now();
        }
    }

    pub async fn get_stats(&self, proxy_url: &str) -> Option<String> {
        let limits = self.inner.read().await;
        limits.get(proxy_url).map(|l| {
            format!(
                "{}/{} req/s (window reset in {}ms)",
                l.request_count,
                self.max_requests_per_second,
                Duration::from_secs(1)
                    .saturating_sub(Instant::now().duration_since(l.window_start))
                    .as_millis()
            )
        })
    }
}

impl Default for ProxyRateLimiter {
    fn default() -> Self {
        Self::new(10) // Default 10 TPS per proxy
    }
}