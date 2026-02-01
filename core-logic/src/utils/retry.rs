#![allow(dead_code)]

use anyhow::{Context, Result};
use rand::Rng;
use std::future::Future;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::Duration;
use tracing::{debug, warn};

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
    last_failure: AtomicU64,
    state: AtomicU8,
    config: CircuitBreakerConfig,
}

impl Clone for CircuitBreaker {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            failure_count: AtomicU64::new(self.failure_count.load(Ordering::SeqCst)),
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
            let successes = self.failure_count.load(Ordering::SeqCst);
            if successes >= self.config.success_threshold {
                self.state.store(STATE_CLOSED, Ordering::SeqCst);
                self.failure_count.store(0, Ordering::SeqCst);
                debug!("Circuit breaker {} CLOSED (recovered)", self.name);
            } else {
                self.failure_count.fetch_add(1, Ordering::SeqCst);
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
