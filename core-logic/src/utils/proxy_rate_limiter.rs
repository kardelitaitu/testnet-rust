use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::debug;

/// Internal state for a single proxy's rate limit
#[derive(Debug, Clone, Copy)]
struct ProxyLimit {
    window_start: Instant,
    request_count: u32,
}

pub struct ProxyRateLimiter {
    inner: Arc<RwLock<HashMap<String, ProxyLimit>>>,
    max_requests_per_second: u32,
}

impl ProxyRateLimiter {
    pub fn new(max_requests_per_second: u32) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            max_requests_per_second,
        }
    }

    /// Internal method to acquire a token using a specific instant
    /// This allows for easier testing without mocking the global clock
    async fn acquire_internal(&self, proxy_url: &str, now: Instant) -> bool {
        let mut limits = self.inner.write().await;
        let limit = limits.entry(proxy_url.to_string()).or_insert(ProxyLimit {
            window_start: now,
            request_count: 0,
        });

        // Check if we need to reset the window
        if now.duration_since(limit.window_start) >= Duration::from_secs(1) {
            limit.request_count = 0;
            limit.window_start = now;
        }

        // Check rate limit
        if limit.request_count >= self.max_requests_per_second {
            let wait_time = Duration::from_secs(1).saturating_sub(now.duration_since(limit.window_start));
            debug!("Proxy {} rate limited, wait {}ms", proxy_url, wait_time.as_millis());
            return false;
        }

        // Rate limit respected
        limit.request_count += 1;

        // Note: The previous min_interval sleep used thread::sleep,
        // which was potentially blocking tokio threads.
        // We'll rely on the caller to manage inter-request pacing if needed,
        // or add async sleep here if we truly want to force pacing.
        // For now, we focus on the window-based rate limiting logic.

        true
    }

    pub async fn acquire(&self, proxy_url: &str) -> bool {
        self.acquire_internal(proxy_url, Instant::now()).await
    }

    /// Wait until rate limit allows
    pub async fn wait_until_available(&self, proxy_url: &str) {
        while !self.acquire(proxy_url).await {
            tokio::time::sleep(Duration::from_millis(10)).await;
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
            let now = Instant::now();
            format!(
                "{}/{} req/s (window reset in {}ms)",
                l.request_count,
                self.max_requests_per_second,
                Duration::from_secs(1)
                    .saturating_sub(now.duration_since(l.window_start))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_with_custom_tps() {
        let limiter = ProxyRateLimiter::new(5);
        assert_eq!(limiter.max_requests_per_second, 5);
    }

    #[tokio::test]
    async fn test_default_tps() {
        let limiter = ProxyRateLimiter::default();
        assert_eq!(limiter.max_requests_per_second, 10);
    }

    #[tokio::test]
    async fn test_acquire_first_request_succeeds() {
        let limiter = ProxyRateLimiter::new(100);
        assert!(limiter.acquire("http://proxy1:8080").await);
    }

    #[tokio::test]
    async fn test_acquire_multiple_proxies_independent() {
        let limiter = ProxyRateLimiter::new(100);
        assert!(limiter.acquire("http://proxy-a:8080").await);
        assert!(limiter.acquire("http://proxy-b:8080").await);
        let stats_a = limiter.get_stats("http://proxy-a:8080").await;
        let stats_b = limiter.get_stats("http://proxy-b:8080").await;
        assert!(stats_a.is_some());
        assert!(stats_b.is_some());
    }

    #[tokio::test]
    async fn test_get_stats_none_for_unseen_proxy() {
        let limiter = ProxyRateLimiter::new(10);
        let stats = limiter.get_stats("http://unknown:8080").await;
        assert!(stats.is_none());
    }

    #[tokio::test]
    async fn test_get_stats_after_acquire() {
        let limiter = ProxyRateLimiter::new(10);
        limiter.acquire("http://proxy1:8080").await;
        let stats = limiter.get_stats("http://proxy1:8080").await.unwrap();
        assert!(stats.contains("1/10"));
    }

    #[tokio::test]
    async fn test_reset_known_proxy() {
        let limiter = ProxyRateLimiter::new(10);
        limiter.acquire("http://proxy1:8080").await;
        limiter.reset("http://proxy1:8080").await;
        let stats = limiter.get_stats("http://proxy1:8080").await.unwrap();
        assert!(stats.contains("0/10"));
    }

    #[tokio::test]
    async fn test_reset_unseen_proxy_no_panic() {
        let limiter = ProxyRateLimiter::new(10);
        limiter.reset("http://unknown:8080").await;
    }

    #[tokio::test]
    async fn test_try_acquire_returns_false_when_exhausted() {
        let limiter = ProxyRateLimiter::new(1);
        assert!(limiter.acquire("p").await);
        assert!(!limiter.acquire("p").await);
    }

    #[tokio::test]
    async fn test_wait_until_available_blocks_then_proceeds() {
        let limiter = Arc::new(ProxyRateLimiter::new(1));
        limiter.acquire("p").await;

        let l = limiter.clone();
        let handle = tokio::spawn(async move {
            l.wait_until_available("p").await;
            true
        });

        // Wait 1.1s for window reset
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(handle.await.unwrap());
    }

    #[tokio::test]
    async fn test_rate_limiter_concurrent_contention() {
        let limiter = Arc::new(ProxyRateLimiter::new(100));
        let proxy = "http://heavy-contention";

        let mut handles = Vec::new();
        for _ in 0..50 {
            let l = limiter.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    l.acquire(proxy).await;
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let stats = limiter.get_stats(proxy).await.unwrap();
        assert!(stats.contains("/100"));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn proptest_proxy_limiter_window_reset(
            tps in 1u32..1000u32,
            time_offset_ms in 1000u64..5000u64
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let limiter = ProxyRateLimiter::new(tps);
                let now = Instant::now();

                // 1. Exhaust the limit
                for _ in 0..tps {
                    assert!(limiter.acquire_internal("p", now).await);
                }
                assert!(!limiter.acquire_internal("p", now).await);

                // 2. Advance time past 1s
                let future_now = now + Duration::from_millis(time_offset_ms);

                // 3. Should be able to acquire again
                assert!(limiter.acquire_internal("p", future_now).await);

                let stats = limiter.get_stats("p").await.unwrap();
                assert!(stats.contains(&format!("1/{}", tps)));
            });
        }

        #[test]
        fn proptest_proxy_limiter_strict_accounting(
            tps in 10u32..100u32,
            requests in 1u32..500u32
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let limiter = ProxyRateLimiter::new(tps);
                let now = Instant::now();

                let mut granted = 0;
                for _ in 0..requests {
                    if limiter.acquire_internal("p", now).await {
                        granted += 1;
                    }
                }

                // Granted should be capped by tps
                let expected_max = std::cmp::min(tps, requests);
                assert_eq!(granted, expected_max);
            });
        }
    }
}
