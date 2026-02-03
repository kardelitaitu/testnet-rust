//! Retry Helper - Exponential backoff with jitter for resilient operations
//!
//! This module provides retry utilities with configurable backoff strategies
//! for handling transient failures in network operations.
//!
//! # Features
//!
//! - **Exponential Backoff**: Doubles delay between retries
//! - **Jitter**: Adds randomness to prevent thundering herd
//! - **Configurable Limits**: Max retries, min/max delays
//! - **Generic**: Works with any async operation returning Result
//!
//! # Example
//!
//! ```rust,no_run
//! use tempo_spammer::utils::retry::{with_retry, RetryConfig};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let config = RetryConfig::default();
//!
//! let result = with_retry(config, || async {
//!     // Your operation here
//!     client.provider.send_transaction(tx.clone()).await
//! }).await?;
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use rand::Rng;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

/// Configuration for retry behavior
#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial delay between retries in milliseconds
    pub initial_delay_ms: u64,
    /// Maximum delay between retries in milliseconds
    pub max_delay_ms: u64,
    /// Multiplier for exponential backoff (e.g., 2.0 = double each time)
    pub backoff_multiplier: f64,
    /// Whether to add random jitter to delays
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay_ms: 100,
            max_delay_ms: 2000,
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Creates a retry config optimized for nonce errors
    ///
    /// Uses shorter delays since nonce errors resolve quickly
    pub fn for_nonce_errors() -> Self {
        Self {
            max_retries: 5,
            initial_delay_ms: 50,
            max_delay_ms: 500,
            backoff_multiplier: 1.5,
            jitter: true,
        }
    }

    /// Creates a retry config for network operations
    ///
    /// Uses longer delays for network-related failures
    pub fn for_network() -> Self {
        Self {
            max_retries: 5,
            initial_delay_ms: 100,
            max_delay_ms: 2000,
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }

    /// Creates a config with no retries (fail fast)
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            initial_delay_ms: 0,
            max_delay_ms: 0,
            backoff_multiplier: 1.0,
            jitter: false,
        }
    }
}

/// Execute an async operation with retry logic
///
/// Retries the operation up to `max_retries` times with exponential backoff.
/// Each retry waits progressively longer, with optional jitter to prevent
/// synchronized retries across multiple tasks.
///
/// # Type Parameters
///
/// * `T` - The success type of the operation
/// * `F` - The future type returned by the operation closure
/// * `Fut` - The future type
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `operation` - Async closure that returns Result<T, E>
///
/// # Returns
///
/// - `Ok(T)` - Operation succeeded (possibly after retries)
/// - `Err(E)` - Operation failed after all retries exhausted
///
/// # Example
///
/// ```rust,no_run
/// use tempo_spammer::utils::retry::{with_retry, RetryConfig};
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = RetryConfig::default();
///
/// let result = with_retry(config, || async {
///     // Simulate a flaky operation
///     if rand::random::<f64>() > 0.7 {
///         Ok("success")
///     } else {
///         Err(anyhow::anyhow!("random failure"))
///     }
/// }).await?;
/// # Ok(())
/// # }
/// ```
pub async fn with_retry<T, E, F, Fut>(config: RetryConfig, mut operation: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    tracing::debug!("Operation succeeded after {} retries", attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                last_error = Some(e);

                if attempt < config.max_retries {
                    let delay = calculate_delay(&config, attempt);
                    tracing::debug!(
                        "Operation failed (attempt {}/{}), retrying in {}ms: {}",
                        attempt + 1,
                        config.max_retries + 1,
                        delay,
                        last_error.as_ref().unwrap()
                    );
                    sleep(Duration::from_millis(delay)).await;
                }
            }
        }
    }

    Err(last_error.unwrap())
}

/// Execute an operation with retry, specifically for nonce errors
///
/// This is a convenience wrapper that uses `RetryConfig::for_nonce_errors()`
/// and automatically resets the nonce cache on "nonce too low" errors.
///
/// # Arguments
///
/// * `operation` - Async closure that performs the operation
/// * `reset_fn` - Async closure to reset nonce cache on error
///
/// # Example
///
/// ```rust,no_run
/// use tempo_spammer::utils::retry::with_nonce_retry;
///
/// # async fn example(client: &TempoClient) -> anyhow::Result<()> {
/// let result = with_nonce_retry(
///     || async { client.provider.send_transaction(tx.clone()).await },
///     || async { client.reset_nonce_cache().await },
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn with_nonce_retry<T, E, F, Fut, R, RFut>(
    mut operation: F,
    mut reset_fn: R,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    R: FnMut() -> RFut,
    RFut: Future<Output = ()>,
    E: std::fmt::Display + AsRef<str>,
{
    let config = RetryConfig::for_nonce_errors();
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    tracing::debug!("Operation succeeded after {} retries", attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                // Check if it's a nonce error before storing the error
                let is_nonce_error =
                    e.as_ref().contains("nonce too low") || e.as_ref().contains("already known");

                if is_nonce_error {
                    tracing::debug!("Detected nonce error, resetting cache");
                    reset_fn().await;
                }

                last_error = Some(e);

                if attempt < config.max_retries {
                    let delay = calculate_delay(&config, attempt);
                    tracing::debug!(
                        "Operation failed (attempt {}/{}), retrying in {}ms",
                        attempt + 1,
                        config.max_retries + 1,
                        delay
                    );
                    sleep(Duration::from_millis(delay)).await;
                }
            }
        }
    }

    Err(last_error.unwrap())
}

/// Calculate the delay for a specific retry attempt
///
/// Uses exponential backoff formula: delay = initial * multiplier^attempt
/// With optional jitter: ±25% random variation
fn calculate_delay(config: &RetryConfig, attempt: u32) -> u64 {
    // Calculate base delay with exponential backoff
    let base_delay =
        config.initial_delay_ms as f64 * config.backoff_multiplier.powi(attempt as i32);

    // Cap at max delay
    let base_delay = base_delay.min(config.max_delay_ms as f64);

    if config.jitter {
        // Add ±25% jitter
        let jitter_range = base_delay / 4.0;
        let jitter = rand::thread_rng().gen_range(-jitter_range..jitter_range);
        (base_delay + jitter).max(0.0) as u64
    } else {
        base_delay as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_delay_exponential_growth() {
        let config = RetryConfig {
            max_retries: 5,
            initial_delay_ms: 100,
            max_delay_ms: 2000,
            backoff_multiplier: 2.0,
            jitter: false,
        };

        // Without jitter, delays should be exact
        assert_eq!(calculate_delay(&config, 0), 100); // 100 * 2^0 = 100
        assert_eq!(calculate_delay(&config, 1), 200); // 100 * 2^1 = 200
        assert_eq!(calculate_delay(&config, 2), 400); // 100 * 2^2 = 400
        assert_eq!(calculate_delay(&config, 3), 800); // 100 * 2^3 = 800
        assert_eq!(calculate_delay(&config, 4), 1600); // 100 * 2^4 = 1600
    }

    #[test]
    fn test_calculate_delay_respects_max() {
        let config = RetryConfig {
            max_retries: 10,
            initial_delay_ms: 1000,
            max_delay_ms: 2000,
            backoff_multiplier: 2.0,
            jitter: false,
        };

        // Should cap at max_delay_ms
        assert_eq!(calculate_delay(&config, 0), 1000);
        assert_eq!(calculate_delay(&config, 1), 2000); // Would be 2000, capped
        assert_eq!(calculate_delay(&config, 2), 2000); // Would be 4000, capped
    }

    #[test]
    fn test_calculate_delay_with_jitter() {
        let config = RetryConfig {
            max_retries: 5,
            initial_delay_ms: 100,
            max_delay_ms: 2000,
            backoff_multiplier: 2.0,
            jitter: true,
        };

        // With jitter, delay should be within ±25%
        let delay = calculate_delay(&config, 1); // Base: 200ms
        assert!(
            delay >= 150 && delay <= 250,
            "Delay {} outside expected range",
            delay
        );
    }

    #[test]
    fn test_retry_config_for_nonce_errors() {
        let config = RetryConfig::for_nonce_errors();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay_ms, 50);
        assert_eq!(config.max_delay_ms, 500);
        assert_eq!(config.backoff_multiplier, 1.5);
        assert!(config.jitter);
    }

    #[test]
    fn test_retry_config_for_network() {
        let config = RetryConfig::for_network();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 2000);
        assert_eq!(config.backoff_multiplier, 2.0);
        assert!(config.jitter);
    }

    #[test]
    fn test_retry_config_no_retry() {
        let config = RetryConfig::no_retry();
        assert_eq!(config.max_retries, 0);
        assert_eq!(config.initial_delay_ms, 0);
        assert_eq!(config.max_delay_ms, 0);
        assert_eq!(config.backoff_multiplier, 1.0);
        assert!(!config.jitter);
    }
}
