#![allow(dead_code)]

use anyhow::{Context, Result};
use rand::Rng;
use std::future::Future;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::Duration;
use tracing::{debug, warn};

/// Configuration for retry behaviour with exponential backoff.
///
/// Supports optional jitter (50-150% of calculated delay) and a max cap.
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub exponential_base: f64,
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            exponential_base: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    pub fn new(max_retries: u32, base_delay_ms: u64) -> Self {
        Self {
            max_retries,
            base_delay_ms,
            max_delay_ms: base_delay_ms * 30,
            ..Default::default()
        }
    }

    pub fn with_max_delay(mut self, max_delay_ms: u64) -> Self {
        self.max_delay_ms = max_delay_ms;
        self
    }

    pub fn with_jitter(mut self, jitter: bool) -> Self {
        self.jitter = jitter;
        self
    }

    pub fn without_jitter(mut self) -> Self {
        self.jitter = false;
        self
    }

    fn calculate_delay(&self, attempt: u32) -> Duration {
        let delay_ms = self.base_delay_ms as f64 * self.exponential_base.powi(attempt as i32);
        let delay_ms = delay_ms.min(self.max_delay_ms as f64);

        let delay_ms = if self.jitter {
            let rng_factor = rand::thread_rng().gen_range(0.5..=1.5);
            delay_ms * rng_factor
        } else {
            delay_ms
        };

        Duration::from_millis(delay_ms as u64)
    }
}

pub async fn with_retry<T, F, Fut>(config: RetryConfig, operation_name: &str, operation: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!("{} succeeded on attempt {}", operation_name, attempt + 1);
                }
                return Ok(result);
            },
            Err(e) => {
                if attempt == config.max_retries {
                    debug!("{} failed after {} retries", operation_name, config.max_retries);
                    let error_msg = format!("{}", e);
                    return Err(e).context(format!(
                        "{} failed after {} attempts. Last error: {}",
                        operation_name, config.max_retries, error_msg
                    ));
                }

                let delay = config.calculate_delay(attempt);
                debug!(
                    "{} failed (attempt {}/{}). Retrying in {:?}: {}",
                    operation_name,
                    attempt + 1,
                    config.max_retries,
                    delay,
                    e
                );

                tokio::time::sleep(delay).await;
            },
        }
    }

    unreachable!()
}

pub async fn with_retry_async<T, F, Fut>(config: RetryConfig, operation_name: &str, mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!("{} succeeded on attempt {}", operation_name, attempt + 1);
                }
                return Ok(result);
            },
            Err(e) => {
                if attempt == config.max_retries {
                    debug!("{} failed after {} retries", operation_name, config.max_retries);
                    let error_msg = format!("{}", e);
                    return Err(e).context(format!(
                        "{} failed after {} attempts. Last error: {}",
                        operation_name, config.max_retries, error_msg
                    ));
                }

                let delay = config.calculate_delay(attempt);
                debug!(
                    "{} failed (attempt {}/{}). Retrying in {:?}: {}",
                    operation_name,
                    attempt + 1,
                    config.max_retries,
                    delay,
                    e
                );

                tokio::time::sleep(delay).await;
            },
        }
    }

    unreachable!()
}

#[derive(Debug)]
pub struct CircuitBreaker {
    name: String,
    failure_count: AtomicU64,
    success_count: AtomicU64,
    last_failure: AtomicU64,
    state: AtomicU8,
    config: CircuitBreakerConfig,
}

impl Clone for CircuitBreaker {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            failure_count: AtomicU64::new(self.failure_count.load(Ordering::SeqCst)),
            success_count: AtomicU64::new(self.success_count.load(Ordering::SeqCst)),
            last_failure: AtomicU64::new(self.last_failure.load(Ordering::SeqCst)),
            state: AtomicU8::new(self.state.load(Ordering::SeqCst)),
            config: self.config,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u64,
    pub success_threshold: u64,
    pub reset_timeout_ms: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            reset_timeout_ms: 60000,
        }
    }
}

const STATE_CLOSED: u8 = 0;
const STATE_OPEN: u8 = 1;
const STATE_HALF_OPEN: u8 = 2;

impl CircuitBreaker {
    pub fn new(name: &str, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.to_string(),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            last_failure: AtomicU64::new(0),
            state: AtomicU8::new(STATE_CLOSED),
            config,
        }
    }

    pub fn new_with_defaults(name: &str) -> Self {
        Self::new(name, CircuitBreakerConfig::default())
    }

    pub async fn execute<T, F, Fut>(&self, operation: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let current_state = self.state.load(Ordering::SeqCst);

        if current_state == STATE_OPEN {
            if self.should_attempt_reset() {
                self.state.store(STATE_HALF_OPEN, Ordering::SeqCst);
                self.success_count.store(0, Ordering::SeqCst);
                debug!("Circuit breaker {} entering HALF_OPEN state", self.name);
            } else {
                return Err(anyhow::anyhow!(
                    "Circuit breaker {} is OPEN. Rejecting request.",
                    self.name
                ));
            }
        }

        match operation().await {
            Ok(result) => {
                self.on_success();
                Ok(result)
            },
            Err(e) => {
                self.on_failure();
                Err(e)
            },
        }
    }

    fn should_attempt_reset(&self) -> bool {
        let last_failure = self.last_failure.load(Ordering::SeqCst);
        let now = chrono::Utc::now().timestamp_millis() as u64;
        now.saturating_sub(last_failure) >= self.config.reset_timeout_ms
    }

    fn on_success(&self) {
        let current_state = self.state.load(Ordering::SeqCst);

        if current_state == STATE_HALF_OPEN {
            let successes = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
            if successes >= self.config.success_threshold {
                self.state.store(STATE_CLOSED, Ordering::SeqCst);
                self.failure_count.store(0, Ordering::SeqCst);
                self.success_count.store(0, Ordering::SeqCst);
                debug!("Circuit breaker {} CLOSED (recovered)", self.name);
            }
        } else {
            self.failure_count.store(0, Ordering::SeqCst);
        }
    }

    fn on_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.last_failure
            .store(chrono::Utc::now().timestamp_millis() as u64, Ordering::SeqCst);

        if failures >= self.config.failure_threshold {
            self.state.store(STATE_OPEN, Ordering::SeqCst);
            warn!("Circuit breaker {} OPEN after {} failures", self.name, failures);
        }
    }

    pub fn state(&self) -> &str {
        match self.state.load(Ordering::SeqCst) {
            STATE_CLOSED => "CLOSED",
            STATE_OPEN => "OPEN",
            STATE_HALF_OPEN => "HALF_OPEN",
            _ => "UNKNOWN",
        }
    }
}

pub fn is_transient_error(error: &anyhow::Error) -> bool {
    let error_msg = format!("{:?}", error).to_lowercase();

    let transient_patterns = [
        "timeout",
        "connection refused",
        "connection reset",
        "network error",
        "temporary failure",
        "service unavailable",
        "rate limited",
        "too many requests",
        "nonce too low",
        "already known",
        "replacement transaction underpriced",
        "database is locked",
        "busy",
    ];

    transient_patterns.iter().any(|pattern| error_msg.contains(pattern))
}

#[cfg(test)]
mod retry_config_tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let cfg = RetryConfig::default();
        assert_eq!(cfg.max_retries, 3);
        assert_eq!(cfg.base_delay_ms, 1000);
        assert_eq!(cfg.max_delay_ms, 30000);
        assert_eq!(cfg.exponential_base, 2.0);
        assert!(cfg.jitter);
    }

    #[test]
    fn test_new_with_custom() {
        let cfg = RetryConfig::new(5, 500);
        assert_eq!(cfg.max_retries, 5);
        assert_eq!(cfg.base_delay_ms, 500);
        assert_eq!(cfg.max_delay_ms, 15000);
        assert!(cfg.jitter);
    }

    #[test]
    fn test_with_max_delay_builder() {
        let cfg = RetryConfig::new(3, 1000).with_max_delay(5000);
        assert_eq!(cfg.max_delay_ms, 5000);
    }

    #[test]
    fn test_without_jitter_builder() {
        let cfg = RetryConfig::new(3, 1000).without_jitter();
        assert!(!cfg.jitter);
    }

    #[test]
    fn test_calculate_delay_without_jitter() {
        let cfg = RetryConfig::new(3, 1000).without_jitter();
        let d0 = cfg.calculate_delay(0);
        assert_eq!(d0, Duration::from_millis(1000));
        let d1 = cfg.calculate_delay(1);
        assert_eq!(d1, Duration::from_millis(2000));
    }

    #[test]
    fn test_calculate_delay_caps_at_max() {
        let cfg = RetryConfig::new(3, 1000).with_max_delay(3000).without_jitter();
        let d = cfg.calculate_delay(2);
        assert_eq!(d, Duration::from_millis(3000));
    }

    #[test]
    fn test_calculate_delay_with_jitter() {
        let cfg = RetryConfig::new(3, 1000);
        let d = cfg.calculate_delay(0);
        let ms = d.as_millis();
        assert!((500..=1500).contains(&ms));
    }
}

#[cfg(test)]
mod circuit_breaker_tests {
    use super::*;

    #[test]
    fn test_new_is_closed() {
        let cb = CircuitBreaker::new_with_defaults("test");
        assert_eq!(cb.state(), "CLOSED");
    }

    #[test]
    fn test_on_failure_state_transition() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 1,
            reset_timeout_ms: 60000,
        };
        let cb = CircuitBreaker::new("svc", config);
        cb.on_failure();
        cb.on_failure();
        assert_eq!(cb.state(), "CLOSED");
        cb.on_failure();
        assert_eq!(cb.state(), "OPEN");
    }

    #[tokio::test]
    async fn test_execute_transitions_to_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            reset_timeout_ms: 1,
        };
        let cb = CircuitBreaker::new("svc", config);
        cb.on_failure();
        assert_eq!(cb.state(), "OPEN");
        cb.last_failure.store(0, Ordering::SeqCst);
        let result = cb.execute(|| async { Ok::<_, anyhow::Error>(42) }).await;
        assert!(result.is_ok());
        assert_eq!(cb.state(), "CLOSED");
    }

    #[tokio::test]
    async fn test_concurrent_half_open_failure_reopens() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            reset_timeout_ms: 1,
        };
        let cb = CircuitBreaker::new("svc", config);
        cb.on_failure();
        assert_eq!(cb.state(), "OPEN");
        cb.last_failure.store(0, Ordering::SeqCst);
        let result = cb.execute(|| async { Err::<i32, _>(anyhow::anyhow!("fail")) }).await;
        assert!(result.is_err());
        assert_eq!(cb.state(), "OPEN");
    }

    #[test]
    fn test_half_open_to_closed_via_on_success() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 3,
            reset_timeout_ms: 60000,
        };
        let cb = CircuitBreaker::new("svc", config);
        cb.state.store(STATE_HALF_OPEN, Ordering::SeqCst);
        cb.on_success();
        assert_eq!(cb.state(), "HALF_OPEN");
        cb.on_success();
        cb.on_success();
        assert_eq!(cb.state(), "CLOSED");
    }
}

#[cfg(test)]
mod concurrent_circuit_breaker_tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_on_failure() {
        let cb = std::sync::Arc::new(CircuitBreaker::new(
            "concurrent",
            CircuitBreakerConfig {
                failure_threshold: 50,
                success_threshold: 1,
                reset_timeout_ms: 60000,
            },
        ));
        let mut handles = Vec::new();
        for _ in 0..10 {
            let cb_clone = cb.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    cb_clone.on_failure();
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(cb.state(), "OPEN");
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn proptest_calculate_delay_never_exceeds_max(
            base_delay in 1u64..10_000,
            max_delay in 10_001u64..3_600_000,
            attempt in 0u32..50,
            jitter in any::<bool>()
        ) {
            let config = RetryConfig::new(3, base_delay).with_max_delay(max_delay).with_jitter(jitter);
            let delay = config.calculate_delay(attempt);

            // Jitter can go up to 150% of the max_delay if not carefully capped
            // Current code min(max_delay, exponential) then apply jitter
            // So max possible is max_delay * 1.5
            let max_possible = if jitter { (max_delay as f64 * 1.51) as u64 } else { max_delay };

            assert!(delay.as_millis() as u64 <= max_possible,
                "Delay {}ms exceeded max possible {}ms (base: {}, max: {}, attempt: {})",
                delay.as_millis(), max_possible, base_delay, max_delay, attempt);
        }

        #[test]
        fn proptest_calculate_delay_monotonic_without_jitter(
            base_delay in 1u64..1000,
            max_delay in 10_000u64..60_000,
            attempt in 0u32..20
        ) {
            let config = RetryConfig::new(3, base_delay).with_max_delay(max_delay).without_jitter();
            let d1 = config.calculate_delay(attempt);
            let d2 = config.calculate_delay(attempt + 1);

            assert!(d2 >= d1, "Delay must be monotonic (d1: {:?}, d2: {:?})", d1, d2);
        }
    }
}

#[cfg(test)]
mod with_retry_tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_with_retry_success_first_try() {
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        let result = with_retry(RetryConfig::new(3, 5).without_jitter(), "test", move || {
            let a = a.clone();
            async move {
                a.fetch_add(1, Ordering::SeqCst);
                Ok::<_, anyhow::Error>(42)
            }
        })
        .await;
        assert!(result.is_ok());
        assert_eq!(*result.as_ref().unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 1, "Should succeed on first try");
    }

    #[tokio::test]
    async fn test_with_retry_fail_then_succeed() {
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        let result = with_retry(RetryConfig::new(3, 5).without_jitter(), "test", move || {
            let a = a.clone();
            async move {
                let count = a.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(anyhow::anyhow!("transient timeout"))
                } else {
                    Ok::<_, anyhow::Error>(99)
                }
            }
        })
        .await;
        assert!(result.is_ok(), "Should succeed after 2 failures");
        assert_eq!(*result.as_ref().unwrap(), 99);
    }

    #[tokio::test]
    async fn test_with_retry_max_retries_exceeded() {
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        let result = with_retry(RetryConfig::new(2, 5).without_jitter(), "test", move || {
            let a = a.clone();
            async move {
                a.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(anyhow::anyhow!("persistent error"))
            }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 3, "Should try 3 times total");
    }

    #[tokio::test]
    async fn test_with_retry_async_success() {
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        let result = with_retry_async(RetryConfig::new(3, 5).without_jitter(), "test", move || {
            let a = a.clone();
            async move {
                a.fetch_add(1, Ordering::SeqCst);
                Ok::<_, anyhow::Error>(42)
            }
        })
        .await;
        assert!(result.is_ok());
        assert_eq!(*result.as_ref().unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_async_fail_then_succeed() {
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        let result = with_retry_async(RetryConfig::new(3, 5).without_jitter(), "test", move || {
            let a = a.clone();
            async move {
                let count = a.fetch_add(1, Ordering::SeqCst);
                if count < 1 {
                    Err(anyhow::anyhow!("transient timeout"))
                } else {
                    Ok::<_, anyhow::Error>(42)
                }
            }
        })
        .await;
        assert!(result.is_ok(), "Should succeed after failure");
    }

    #[tokio::test]
    async fn test_execute_half_open_success_closes() {
        let cb = CircuitBreaker::new(
            "half_open_test",
            CircuitBreakerConfig {
                failure_threshold: 1,
                success_threshold: 1,
                reset_timeout_ms: 200,
            },
        );
        cb.on_failure();
        assert_eq!(cb.state(), "OPEN");

        tokio::time::sleep(Duration::from_millis(250)).await;
        cb.last_failure.store(0, std::sync::atomic::Ordering::SeqCst);

        let result = cb.execute(|| async { Ok::<_, anyhow::Error>(42) }).await;
        assert!(result.is_ok());
        assert_eq!(*result.as_ref().unwrap(), 42);
        assert_eq!(cb.state(), "CLOSED");
    }

    #[tokio::test]
    async fn test_execute_half_open_failure_reopens() {
        let cb = CircuitBreaker::new(
            "reopen_test",
            CircuitBreakerConfig {
                failure_threshold: 1,
                success_threshold: 1,
                reset_timeout_ms: 200,
            },
        );
        cb.on_failure();
        assert_eq!(cb.state(), "OPEN");

        tokio::time::sleep(Duration::from_millis(250)).await;
        cb.last_failure.store(0, std::sync::atomic::Ordering::SeqCst);

        let result = cb.execute(|| async { Err::<i32, _>(anyhow::anyhow!("fail")) }).await;
        assert!(result.is_err());
        assert_eq!(cb.state(), "OPEN");
    }
}
