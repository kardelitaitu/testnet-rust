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
                    format!("PAUSED ({}s remaining)", (paused - Instant::now()).as_secs())
                } else {
                    format!("active ({} failures)", h.failure_count)
                }
            } else {
                format!("active ({} failures)", h.failure_count)
            };
            format!("{} success, {} - {}", h.success_count, h.failure_count, status)
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
        
        proxies.iter().filter(|p| {
            if let Some(health) = health_map.get(*p) {
                if let Some(paused_until) = health.paused_until {
                    return now >= paused_until;
                }
            }
            true
        }).count()
    }
}

impl Default for ProxyHealthManager {
    fn default() -> Self {
        Self::new(3, 5) // 3 failures = 5 minute pause
    }
}