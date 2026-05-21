use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct ProxyHealth {
    pub failure_count: u32,
    pub last_failure: Option<Instant>,
    pub paused_until: Option<Instant>,
    pub success_count: u32,
}

impl Default for ProxyHealth {
    fn default() -> Self {
        Self {
            failure_count: 0,
            last_failure: None,
            paused_until: None,
            success_count: 0,
        }
    }
}

pub struct ProxyHealthManager {
    inner: Arc<RwLock<HashMap<String, ProxyHealth>>>,
    failure_threshold: u32,
    pause_duration: Duration,
}

impl ProxyHealthManager {
    pub fn new(failure_threshold: u32, pause_minutes: u64) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            failure_threshold,
            pause_duration: Duration::from_secs(pause_minutes * 60),
        }
    }

    pub async fn record_failure(&self, proxy_url: &str) {
        let mut health_map = self.inner.write().await;
        let health = health_map.entry(proxy_url.to_string()).or_default();

        health.failure_count += 1;
        health.last_failure = Some(Instant::now());

        if health.failure_count >= self.failure_threshold {
            health.paused_until = Some(Instant::now() + self.pause_duration);
            health.failure_count = 0; // Reset after pausing
            warn!(
                "Proxy {} paused for {} minutes ({} failures)",
                proxy_url,
                self.pause_duration.as_secs() / 60,
                self.failure_threshold
            );
        } else {
            debug!(
                "Proxy {} failure count: {}/{}",
                proxy_url, health.failure_count, self.failure_threshold
            );
        }
    }

    pub async fn record_success(&self, proxy_url: &str) {
        let mut health_map = self.inner.write().await;
        let health = health_map.entry(proxy_url.to_string()).or_default();

        health.failure_count = 0;
        health.success_count += 1;

        // If proxy was paused but now succeeding, it might have recovered
        if health.paused_until.is_some() {
            info!("Proxy {} recovered (success after pause)", proxy_url);
        }

        health.paused_until = None; // Clear any pause
    }

    pub async fn is_available(&self, proxy_url: &str) -> bool {
        let health_map = self.inner.read().await;
        if let Some(health) = health_map.get(proxy_url) {
            if let Some(paused_until) = health.paused_until {
                if Instant::now() < paused_until {
                    return false;
                }
            }
        }
        true
    }

    pub async fn get_status(&self, proxy_url: &str) -> Option<String> {
        let health_map = self.inner.read().await;
        health_map.get(proxy_url).map(|h| {
            let status = if let Some(paused) = h.paused_until {
                if Instant::now() < paused {
                    format!(
                        "PAUSED ({}s remaining)",
                        (paused - Instant::now()).as_secs()
                    )
                } else {
                    format!("active ({} failures)", h.failure_count)
                }
            } else {
                format!("active ({} failures)", h.failure_count)
            };
            format!(
                "{} success, {} - {}",
                h.success_count, h.failure_count, status
            )
        })
    }

    pub async fn cleanup_expired(&self) {
        let mut health_map = self.inner.write().await;
        let now = Instant::now();

        for (_, health) in health_map.iter_mut() {
            if let Some(paused_until) = health.paused_until {
                if now >= paused_until {
                    health.paused_until = None;
                    info!("Proxy pause expired, proxy available again");
                }
            }
        }
    }

    pub async fn get_healthy_count(&self, proxies: &[String]) -> usize {
        let health_map = self.inner.read().await;
        let now = Instant::now();

        proxies
            .iter()
            .filter(|p| {
                if let Some(health) = health_map.get(*p) {
                    if let Some(paused_until) = health.paused_until {
                        return now >= paused_until;
                    }
                }
                true
            })
            .count()
    }
}

impl Default for ProxyHealthManager {
    fn default() -> Self {
        Self::new(3, 5) // 3 failures = 5 minute pause
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_default_threshold() {
        let mgr = ProxyHealthManager::default();
        assert_eq!(mgr.failure_threshold, 3);
        assert_eq!(mgr.pause_duration, Duration::from_secs(5 * 60));
    }

    #[tokio::test]
    async fn test_new_custom_values() {
        let mgr = ProxyHealthManager::new(5, 10);
        assert_eq!(mgr.failure_threshold, 5);
        assert_eq!(mgr.pause_duration, Duration::from_secs(10 * 60));
    }

    #[tokio::test]
    async fn test_available_for_new_proxy() {
        let mgr = ProxyHealthManager::default();
        assert!(mgr.is_available("http://proxy:8080").await);
    }

    #[tokio::test]
    async fn test_success_resets_failures() {
        let mgr = ProxyHealthManager::new(3, 5);
        mgr.record_failure("http://proxy:8080").await;
        mgr.record_failure("http://proxy:8080").await;
        mgr.record_success("http://proxy:8080").await;
        // After success, failures should be 0 — not paused
        assert!(mgr.is_available("http://proxy:8080").await);
    }

    #[tokio::test]
    async fn test_failure_threshold_pauses() {
        let mgr = ProxyHealthManager::new(2, 60); // 2 failures = pause
        mgr.record_failure("http://proxy:8080").await;
        assert!(mgr.is_available("http://proxy:8080").await);
        mgr.record_failure("http://proxy:8080").await;
        // Second failure should hit threshold and pause
        assert!(!mgr.is_available("http://proxy:8080").await);
    }

    #[tokio::test]
    async fn test_success_clears_pause() {
        let mgr = ProxyHealthManager::new(2, 60);
        mgr.record_failure("http://proxy:8080").await;
        mgr.record_failure("http://proxy:8080").await;
        assert!(!mgr.is_available("http://proxy:8080").await); // paused
        mgr.record_success("http://proxy:8080").await;
        assert!(mgr.is_available("http://proxy:8080").await); // recovered
    }

    #[tokio::test]
    async fn test_get_status_format() {
        let mgr = ProxyHealthManager::default();
        let status = mgr.get_status("http://unknown:8080").await;
        assert!(status.is_none(), "unknown proxy has no status");

        mgr.record_success("http://proxy:8080").await;
        mgr.record_success("http://proxy:8080").await;
        mgr.record_failure("http://proxy:8080").await;
        let status = mgr.get_status("http://proxy:8080").await;
        assert!(status.is_some());
        let s = status.unwrap();
        assert!(s.contains("2 success"), "should show 2 success: {}", s);
    }

    #[tokio::test]
    async fn test_healthy_count_all_available() {
        let mgr = ProxyHealthManager::default();
        let proxies = vec!["http://a:8080".into(), "http://b:8080".into()];
        assert_eq!(mgr.get_healthy_count(&proxies).await, 2);
    }

    #[tokio::test]
    async fn test_healthy_count_with_paused() {
        let mgr = ProxyHealthManager::new(1, 60);
        mgr.record_failure("http://bad:8080").await;
        let proxies = vec!["http://good:8080".into(), "http://bad:8080".into()];
        assert_eq!(mgr.get_healthy_count(&proxies).await, 1);
    }

    #[tokio::test]
    async fn test_cleanup_expired_does_not_crash() {
        let mgr = ProxyHealthManager::new(1, 0); // 0 minute pause
        mgr.record_failure("http://proxy:8080").await;
        // cleanup should not crash even with expired pauses
        mgr.cleanup_expired().await;
        // Proxy may or may not be available depending on timing
        // The important thing is no panic
        let _ = mgr.is_available("http://proxy:8080").await;
    }

    #[test]
    fn test_proxy_health_default() {
        let h = ProxyHealth::default();
        assert_eq!(h.failure_count, 0);
        assert!(h.last_failure.is_none());
        assert!(h.paused_until.is_none());
        assert_eq!(h.success_count, 0);
    }

    #[test]
    fn test_proxy_health_clone() {
        let h = ProxyHealth::default();
        let c = h.clone();
        assert_eq!(h.failure_count, c.failure_count);
    }

    #[tokio::test]
    async fn test_get_status_active_format() {
        let mgr = ProxyHealthManager::new(3, 5);
        mgr.record_success("http://proxy:8080").await;
        mgr.record_failure("http://proxy:8080").await;
        let status = mgr.get_status("http://proxy:8080").await.unwrap();
        assert!(status.contains("1 success"), "status: {}", status);
        assert!(status.contains("active"), "should show active: {}", status);
    }

    #[tokio::test]
    async fn test_healthy_count_empty_list() {
        let mgr = ProxyHealthManager::default();
        assert_eq!(mgr.get_healthy_count(&[]).await, 0);
    }

    #[tokio::test]
    async fn test_record_failure_zero_threshold_pauses() {
        let mgr = ProxyHealthManager::new(0, 10);
        mgr.record_failure("http://proxy:8080").await;
        // With threshold=0, even a single failure should pause
        assert!(!mgr.is_available("http://proxy:8080").await);
    }

    #[tokio::test]
    async fn test_get_status_paused_format() {
        let mgr = ProxyHealthManager::new(1, 10);
        mgr.record_failure("http://proxy:8080").await;
        let status = mgr.get_status("http://proxy:8080").await;
        assert!(status.is_some());
        let s = status.unwrap();
        assert!(
            s.contains("PAUSED"),
            "paused proxy should show PAUSED: {}",
            s
        );
        assert!(s.contains("remaining"), "should show time remaining: {}", s);
    }

    #[tokio::test]
    async fn test_multiple_proxies_independent() {
        let mgr = ProxyHealthManager::new(2, 10);
        mgr.record_failure("http://proxy-a:8080").await;
        mgr.record_failure("http://proxy-a:8080").await;
        // proxy-a should be paused
        assert!(!mgr.is_available("http://proxy-a:8080").await);
        // proxy-b should still be available
        assert!(mgr.is_available("http://proxy-b:8080").await);
    }
}
