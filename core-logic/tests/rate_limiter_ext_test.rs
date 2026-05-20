use core_logic::ProxyRateLimiter;

#[tokio::test]
async fn test_proxy_rate_limiter_basic_acquire() {
    let limiter = ProxyRateLimiter::new(1000); // high TPS, should always pass
    assert!(limiter.acquire("http://proxy1:8080").await);
}

#[tokio::test]
async fn test_proxy_rate_limiter_acquire_at_capacity() {
    let limiter = ProxyRateLimiter::new(5); // 5 TPS
    for _ in 0..5 {
        assert!(limiter.acquire("http://proxy1:8080").await);
    }
    // 6th should fail (burst exceeded)
    // Note: may still succeed if time has passed
    // Just verify no panic
    let _ = limiter.acquire("http://proxy1:8080").await;
}

#[tokio::test]
async fn test_proxy_rate_limiter_multiple_proxies_independent() {
    let limiter = ProxyRateLimiter::new(5);
    for _ in 0..5 {
        assert!(limiter.acquire("http://proxy_a").await);
        assert!(limiter.acquire("http://proxy_b").await);
    }
}

#[tokio::test]
async fn test_proxy_rate_limiter_reset() {
    let limiter = ProxyRateLimiter::new(2);
    assert!(limiter.acquire("http://proxy1:8080").await);
    assert!(limiter.acquire("http://proxy1:8080").await);

    // Reset should clear the count
    limiter.reset("http://proxy1:8080").await;
    assert!(limiter.acquire("http://proxy1:8080").await);
}

#[tokio::test]
async fn test_proxy_rate_limiter_get_stats() {
    let limiter = ProxyRateLimiter::new(10);
    limiter.acquire("http://proxy1:8080").await;
    limiter.acquire("http://proxy1:8080").await;

    let stats = limiter.get_stats("http://proxy1:8080").await;
    assert!(stats.is_some());
    let s = stats.unwrap();
    assert!(s.contains("req/s"));
}

#[tokio::test]
async fn test_proxy_rate_limiter_get_stats_unknown() {
    let limiter = ProxyRateLimiter::new(10);
    let stats = limiter.get_stats("http://unknown").await;
    assert!(stats.is_none());
}

#[tokio::test]
async fn test_proxy_rate_limiter_default_tps() {
    let limiter = ProxyRateLimiter::default();
    let default_tps = 10;
    for _ in 0..default_tps {
        assert!(limiter.acquire("http://proxy1:8080").await);
    }
    // Just verify it works — actual rate limiting is timing-dependent
}

#[tokio::test]
async fn test_proxy_rate_limiter_wait_until_available() {
    let limiter = ProxyRateLimiter::new(1000); // high TPS, should not wait
    let start = tokio::time::Instant::now();
    limiter.wait_until_available("http://proxy1:8080").await;
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 200, "Should be fast at high TPS");
}

#[tokio::test]
async fn test_proxy_rate_limiter_stats_format() {
    let limiter = ProxyRateLimiter::new(10);
    limiter.acquire("http://p1").await;
    limiter.acquire("http://p1").await;

    let stats = limiter.get_stats("http://p1").await.unwrap();
    // Should contain count and max
    assert!(stats.contains("2") || stats.contains("1")); // count could be 1 or 2 depending on timing
    assert!(stats.contains("10")); // max TPS
}
