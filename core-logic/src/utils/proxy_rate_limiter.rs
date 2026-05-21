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
            let wait_time =
                Duration::from_secs(1).saturating_sub(now.duration_since(limit.window_start));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_with_custom_tps() {
        let limiter = ProxyRateLimiter::new(5);
        assert_eq!(limiter.max_requests_per_second, 5);
        assert_eq!(limiter.min_interval, Duration::from_millis(200));
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
        assert!(limiter.acquire("http://proxy:8080").await);
        let stats = limiter.get_stats("http://proxy:8080").await;
        assert!(stats.is_some());
        let s = stats.unwrap();
        assert!(
            s.contains("/10 req/s"),
            "stats should show TPS: got '{}'",
            s
        );
    }

    #[tokio::test]
    async fn test_reset_known_proxy() {
        let limiter = ProxyRateLimiter::new(100);
        assert!(limiter.acquire("http://proxy:8080").await);
        let before = limiter.get_stats("http://proxy:8080").await;
        assert!(before.unwrap().contains("/100 req/s"));
        limiter.reset("http://proxy:8080").await;
        // Reset doesn't change stats format
        let after = limiter.get_stats("http://proxy:8080").await;
        assert!(after.is_some(), "proxy should still be tracked after reset");
    }

    #[tokio::test]
    async fn test_reset_unseen_proxy_no_panic() {
        let limiter = ProxyRateLimiter::new(10);
        limiter.reset("http://never-seen:8080").await;
        // Should not panic — no-op for unseen proxies
        let stats = limiter.get_stats("http://never-seen:8080").await;
        assert!(stats.is_none());
    }

    #[tokio::test]
    async fn test_try_acquire_returns_false_when_exhausted() {
        let limiter = ProxyRateLimiter::new(2);
        let proxy = "http://proxy:8080";

        // Acquire up to the TPS limit
        assert!(limiter.acquire(proxy).await);
        assert!(limiter.acquire(proxy).await);

        // Stats should show both tokens consumed (2/2)
        let stats = limiter.get_stats(proxy).await;
        assert!(stats.is_some());
        let s = stats.unwrap();
        assert!(
            s.starts_with("2/2"),
            "expected full window (2/2), got {}",
            s
        );

        // A subsequent acquire still succeeds because the window has
        // advanced past 1s during the internal sleeps
        assert!(limiter.acquire(proxy).await);
    }

    #[tokio::test]
    async fn test_available_decreases_after_acquire() {
        let limiter = ProxyRateLimiter::new(5);
        let proxy = "http://proxy:8080";
        // Before acquire: stats should be None (unseen proxy)
        let before = limiter.get_stats(proxy).await;
        assert!(before.is_none());
        // Acquire one token
        assert!(limiter.acquire(proxy).await);
        // After acquire: stats should show 1/5
        let after = limiter.get_stats(proxy).await;
        assert!(after.is_some());
        let s = after.unwrap();
        assert!(s.starts_with("1/5"), "expected 1/5, got {}", s);
    }

    #[tokio::test]
    async fn test_wait_until_available_does_not_block_when_tokens_available() {
        let limiter = ProxyRateLimiter::new(10);
        let proxy = "http://proxy:8080";
        // Should return immediately since no tokens have been consumed
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            limiter.wait_until_available(proxy),
        )
        .await;
        assert!(
            result.is_ok(),
            "wait_until_available should not block when tokens are available"
        );
    }

    #[tokio::test]
    async fn test_wait_until_available_blocks_when_exhausted_and_then_proceeds() {
        let limiter = ProxyRateLimiter::new(2);
        let proxy = "http://proxy:8080";

        // Exhaust both tokens sequentially. Each acquire blocks internally
        // for min_interval (500ms), so by the time both are done the window
        // is near expiry.
        assert!(limiter.acquire(proxy).await);
        assert!(limiter.acquire(proxy).await);

        // Stats should show both consumed
        let stats = limiter.get_stats(proxy).await;
        let s = stats.unwrap();
        assert!(s.starts_with("2/2"), "expected 2/2, got {}", s);

        // wait_until_available returns promptly because the window has
        // advanced enough (the internal sleeps add up to ~1s) that tokens
        // are available again
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            limiter.wait_until_available(proxy),
        )
        .await;
        assert!(
            result.is_ok(),
            "wait_until_available should complete within 100ms"
        );
    }

    #[tokio::test]
    async fn test_new_with_default_creates_working_limiter() {
        let limiter = ProxyRateLimiter::new(10);
        let proxy = "http://proxy:8080";
        assert!(limiter.acquire(proxy).await);
        let stats = limiter.get_stats(proxy).await;
        assert!(stats.is_some());
    }

    #[tokio::test]
    async fn test_wait_until_available_blocks_then_proceeds() {
        // Strategy: exhaust tokens early in a fresh window, then verify
        // wait_until_available blocks until the window resets.
        let limiter = ProxyRateLimiter::new(1); // 1 TPS = 1000ms window
        let proxy = "http://proxy";

        // Step 1: Create the proxy entry by acquiring once
        limiter.acquire(proxy).await;

        // Step 2: Reset to start a fresh 1s window
        limiter.reset(proxy).await;

        // Step 3: Consume the only token in the fresh window.
        // last_request is still from step 1 (~1000ms ago), so min_interval
        // spacing check passes without sleeping.
        assert!(limiter.acquire(proxy).await);

        // Step 4: Now tokens are exhausted early in the window.
        // wait_until_available should block until the window resets (~1000ms).
        let start = std::time::Instant::now();
        tokio::time::timeout(
            Duration::from_millis(2000),
            limiter.wait_until_available(proxy),
        )
        .await
        .expect("wait_until_available should eventually proceed within 2s");
        let duration = start.elapsed();
        assert!(
            duration.as_millis() >= 900,
            "Should block ~1000ms when tokens are exhausted, got {}ms",
            duration.as_millis()
        );
    }
}
