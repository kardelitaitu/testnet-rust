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
///
/// ```
/// use core_logic::RetryConfig;
///
/// let cfg = RetryConfig::new(3, 1000)
///     .with_max_delay(10000)
///     .without_jitter();
/// assert_eq!(cfg.max_retries, 3);
/// assert_eq!(cfg.base_delay_ms, 1000);
/// assert!(!cfg.jitter);
/// ```
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

pub async fn with_retry<T, F, Fut>(
    config: RetryConfig,
    operation_name: &str,
    operation: F,
) -> Result<T>
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
            }
            Err(e) => {
                if attempt == config.max_retries {
                    debug!(
                        "{} failed after {} retries",
                        operation_name, config.max_retries
                    );
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
            }
        }
    }

    unreachable!()
}

pub async fn with_retry_async<T, F, Fut>(
    config: RetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<T>
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
            }
            Err(e) => {
                if attempt == config.max_retries {
                    debug!(
                        "{} failed after {} retries",
                        operation_name, config.max_retries
                    );
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
            }
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
            }
            Err(e) => {
                self.on_failure();
                Err(e)
            }
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
        self.last_failure.store(
            chrono::Utc::now().timestamp_millis() as u64,
            Ordering::SeqCst,
        );

        if failures >= self.config.failure_threshold {
            self.state.store(STATE_OPEN, Ordering::SeqCst);
            warn!(
                "Circuit breaker {} OPEN after {} failures",
                self.name, failures
            );
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

    transient_patterns
        .iter()
        .any(|pattern| error_msg.contains(pattern))
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
        assert_eq!(cfg.max_delay_ms, 15000); // 500 * 30
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
        // attempt 0: 1000 * 2^0 = 1000ms
        let d0 = cfg.calculate_delay(0);
        assert_eq!(d0, Duration::from_millis(1000));
        // attempt 1: 1000 * 2^1 = 2000ms
        let d1 = cfg.calculate_delay(1);
        assert_eq!(d1, Duration::from_millis(2000));
        // attempt 2: 1000 * 2^2 = 4000ms
        let d2 = cfg.calculate_delay(2);
        assert_eq!(d2, Duration::from_millis(4000));
    }

    #[test]
    fn test_calculate_delay_caps_at_max() {
        let cfg = RetryConfig::new(3, 1000)
            .with_max_delay(3000)
            .without_jitter();
        // attempt 2: 1000 * 2^2 = 4000, but capped at 3000
        let d = cfg.calculate_delay(2);
        assert_eq!(d, Duration::from_millis(3000));
    }

    #[test]
    fn test_calculate_delay_with_jitter() {
        let cfg = RetryConfig::new(3, 1000);
        // With jitter, delay should be between 50% and 150% of base
        let d = cfg.calculate_delay(0);
        let ms = d.as_millis();
        assert!(ms >= 500, "jitter should be at least 50%, got {}ms", ms);
        assert!(ms <= 1500, "jitter should be at most 150%, got {}ms", ms);
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
    fn test_new_custom_config() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            reset_timeout_ms: 30000,
        };
        let cb = CircuitBreaker::new("svc", config);
        assert_eq!(cb.state(), "CLOSED");
        assert_eq!(cb.config.failure_threshold, 3);
    }

    #[test]
    fn test_default_config_values() {
        let cfg = CircuitBreakerConfig::default();
        assert_eq!(cfg.failure_threshold, 5);
        assert_eq!(cfg.success_threshold, 3);
        assert_eq!(cfg.reset_timeout_ms, 60000);
    }

    #[test]
    fn test_on_failure_state_transition() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 1,
            reset_timeout_ms: 60000,
        };
        let cb = CircuitBreaker::new("svc", config);

        assert_eq!(cb.state(), "CLOSED");

        // 2 failures still CLOSED
        cb.on_failure();
        cb.on_failure();
        assert_eq!(cb.state(), "CLOSED");

        // 3rd failure → OPEN
        cb.on_failure();
        assert_eq!(cb.state(), "OPEN");
    }

    #[test]
    fn test_on_success_in_closed_resets_counter() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 1,
            reset_timeout_ms: 60000,
        };
        let cb = CircuitBreaker::new("svc", config);

        cb.on_failure(); // 1 failure
        assert_eq!(cb.state(), "CLOSED");
        cb.on_success(); // should reset counter to 0
                         // 3 more failures → should still open
        cb.on_failure();
        cb.on_failure();
        cb.on_failure();
        assert_eq!(cb.state(), "OPEN");
    }

    #[test]
    fn test_clone_preserves_state() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 1,
            reset_timeout_ms: 60000,
        };
        let cb = CircuitBreaker::new("svc", config);
        cb.on_failure();
        cb.on_failure();
        assert_eq!(cb.state(), "OPEN");

        let cloned = cb.clone();
        assert_eq!(cloned.state(), "OPEN");
    }

    #[tokio::test]
    async fn test_execute_transitions_to_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            reset_timeout_ms: 1, // 1ms timeout for quick test
        };
        let cb = CircuitBreaker::new("svc", config);
        cb.on_failure();
        assert_eq!(cb.state(), "OPEN");

        // Set last_failure far in the past so should_attempt_reset returns true
        cb.last_failure
            .store(0, std::sync::atomic::Ordering::SeqCst);

        let result = cb.execute(|| async { Ok::<_, anyhow::Error>(42) }).await;
        assert!(result.is_ok());
        assert_eq!(*result.as_ref().unwrap(), 42);
        // After success in HALF_OPEN with threshold=1, should go to CLOSED
        assert_eq!(cb.state(), "CLOSED");
    }

    #[tokio::test]
    async fn test_execute_rejects_when_open_no_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            reset_timeout_ms: 60000, // Long timeout — won't attempt reset
        };
        let cb = CircuitBreaker::new("svc", config);
        cb.on_failure();
        assert_eq!(cb.state(), "OPEN");

        let result = cb.execute(|| async { Ok::<_, anyhow::Error>(42) }).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("OPEN"));
    }

    #[tokio::test]
    async fn test_execute_half_open_failure_reopens() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            reset_timeout_ms: 1,
        };
        let cb = CircuitBreaker::new("svc", config);
        cb.on_failure();
        assert_eq!(cb.state(), "OPEN");
        cb.last_failure
            .store(0, std::sync::atomic::Ordering::SeqCst);

        // Execute a failing operation in HALF_OPEN → should go back to OPEN
        let result = cb
            .execute(|| async { Err::<i32, _>(anyhow::anyhow!("fail")) })
            .await;
        assert!(result.is_err());
        assert_eq!(cb.state(), "OPEN", "Failure in HALF_OPEN should reopen");
    }

    #[test]
    fn test_half_open_to_closed_via_on_success() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 3,
            reset_timeout_ms: 60000,
        };
        let cb = CircuitBreaker::new("svc", config);
        // Manually set to HALF_OPEN — simulates the OPEN→HALF_OPEN transition
        cb.state
            .store(STATE_HALF_OPEN, std::sync::atomic::Ordering::SeqCst);
        // on_success increments the separate success_count
        cb.on_success();     // success_count=1
        assert_eq!(cb.state(), "HALF_OPEN"); // 1 < 3
        cb.on_success();     // success_count=2
        assert_eq!(cb.state(), "HALF_OPEN"); // 2 < 3
        cb.on_success();     // success_count=3
        assert_eq!(cb.state(), "CLOSED");     // 3 >= 3 → closed
        // Verify failure_count was also reset
        assert_eq!(cb.failure_count.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn test_new_with_defaults() {
        let cb = CircuitBreaker::new_with_defaults("test");
        assert_eq!(cb.state(), "CLOSED");
        assert_eq!(cb.config.failure_threshold, 5);
        assert_eq!(cb.config.success_threshold, 3);
        assert_eq!(cb.config.reset_timeout_ms, 60000);
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn proptest_circuit_breaker_random_sequence(actions in proptest::collection::vec(0..2u8, 1..20)) {
            let cb = CircuitBreaker::new("test", CircuitBreakerConfig {
                failure_threshold: 2,
                success_threshold: 2,
                reset_timeout_ms: 50000,
            });
            for action in actions {
                match action {
                    0 => cb.on_failure(),
                    1 => cb.on_success(),
                    _ => unreachable!(),
                }
                let state = cb.state();
                assert!(state == "CLOSED" || state == "OPEN" || state == "HALF_OPEN",
                    "Invalid state: {}", state);
            }
        }
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
        let tasks = 10;
        let failures_per_task = 10;

        for _ in 0..tasks {
            let cb_clone = cb.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..failures_per_task {
                    cb_clone.on_failure();
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        // 10 tasks × 10 failures = 100, threshold = 50 → should be OPEN
        assert_eq!(cb.state(), "OPEN");
    }

    #[tokio::test]
    async fn test_concurrent_mixed_success_failure() {
        let cb = std::sync::Arc::new(CircuitBreaker::new(
            "mixed",
            CircuitBreakerConfig {
                failure_threshold: 20,
                success_threshold: 3,
                reset_timeout_ms: 60000,
            },
        ));
        let mut handles = Vec::new();

        // 4 concurrent failure generators
        for _ in 0..4 {
            let cb_clone = cb.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    cb_clone.on_failure();
                }
            }));
        }
        // 4 concurrent success generators (interleaved with failures)
        for _ in 0..4 {
            let cb_clone = cb.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    cb_clone.on_success();
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        // 4×10 = 40 failures vs threshold 20 → OPEN.
        // 4×10 = 40 successes in CLOSED state just reset the counter.
        assert_eq!(cb.state(), "OPEN");
    }
}

#[cfg(test)]
mod transient_error_tests {
    use super::*;

    #[test]
    fn test_timeout_is_transient() {
        let err = anyhow::anyhow!("connection timeout");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_rate_limited_is_transient() {
        let err = anyhow::anyhow!("rate limited: too many requests");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_nonce_too_low_is_transient() {
        let err = anyhow::anyhow!("nonce too low");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_generic_error_not_transient() {
        let err = anyhow::anyhow!("invalid input format");
        assert!(!is_transient_error(&err));
    }

    #[test]
    fn test_empty_error_not_transient() {
        let err = anyhow::anyhow!("");
        assert!(!is_transient_error(&err));
    }

    #[test]
    fn test_connection_refused_is_transient() {
        let err = anyhow::anyhow!("connection refused: port 8545");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_connection_reset_is_transient() {
        let err = anyhow::anyhow!("connection reset by peer");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_too_many_requests_is_transient() {
        let err = anyhow::anyhow!("too many requests: retry later");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_database_locked_is_transient() {
        let err = anyhow::anyhow!("database is locked");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_already_known_is_transient() {
        let err = anyhow::anyhow!("already known: tx 0xabc");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_network_error_is_transient() {
        let err = anyhow::anyhow!("network error: DNS resolution failed");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_service_unavailable_is_transient() {
        let err = anyhow::anyhow!("service unavailable");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_temporary_failure_is_transient() {
        let err = anyhow::anyhow!("temporary failure in name resolution");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_replacement_underpriced_is_transient() {
        let err = anyhow::anyhow!("replacement transaction underpriced");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_busy_is_transient() {
        let err = anyhow::anyhow!("database busy: retry");
        assert!(is_transient_error(&err));
    }
}
