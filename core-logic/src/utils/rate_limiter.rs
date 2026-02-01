//! # Core Logic - Rate Limiting Utilities
//!
//! Generic rate limiting utilities that can be used across different
//! blockchain implementations.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::debug;

/// Thread-safe token bucket implementation for rate limiting
#[derive(Debug)]
pub struct TokenBucket {
    tokens: AtomicU64,
    capacity: u64,
    refill_rate: u64,
    last_refill: AtomicU64,
}

impl TokenBucket {
    /// Create a new token bucket with given capacity and refill rate
    pub fn new(capacity: u64, refill_rate: u64) -> Self {
        Self {
            tokens: AtomicU64::new(capacity),
            capacity,
            refill_rate,
            last_refill: AtomicU64::new(now_ms()),
        }
    }

    fn refill(&self) {
        let now = now_ms();
        let last = self.last_refill.load(Ordering::SeqCst);
        let elapsed = now.saturating_sub(last);

        if elapsed > 0 {
            let added = (elapsed * self.refill_rate) / 1000;
            let current = self.tokens.load(Ordering::SeqCst);
            let new_tokens = (current + added).min(self.capacity);

            if self
                .tokens
                .compare_exchange(current, new_tokens, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                self.last_refill.store(now, Ordering::SeqCst);
            }
        }
    }

    /// Try to acquire tokens, returns true if successful
    pub fn try_acquire(&self, cost: u64) -> bool {
        self.refill();
        loop {
            let current = self.tokens.load(Ordering::SeqCst);

            if current >= cost {
                if self
                    .tokens
                    .compare_exchange(current, current - cost, Ordering::SeqCst, Ordering::Relaxed)
                    .is_ok()
                {
                    return true;
                }
            } else {
                return false;
            }
        }
    }

    /// Get available tokens
    pub fn available(&self) -> u64 {
        self.refill();
        self.tokens.load(Ordering::SeqCst)
    }
}

fn now_ms() -> u64 {
    Instant::now().elapsed().as_millis() as u64
}

/// Configuration for rate limiting
#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    /// Transactions per second
    pub tps: u32,
    /// Burst multiplier (tokens = tps * burst_multiplier)
    pub burst_multiplier: u32,
    /// Backoff factor when rate limited
    pub backoff_factor: u32,
    /// Maximum backoff time in milliseconds
    pub max_backoff_ms: u64,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            tps: 10,
            burst_multiplier: 2,
            backoff_factor: 2,
            max_backoff_ms: 30000,
        }
    }
}

/// Per-wallet rate limiter with automatic backoff on 429 errors
#[derive(Debug)]
pub struct PerWalletRateLimiter {
    buckets: Mutex<HashMap<String, Arc<TokenBucket>>>,
    global_bucket: Mutex<TokenBucket>,
    config: Mutex<RateLimiterConfig>,
    backoff_ms: Mutex<HashMap<String, u64>>,
}

impl PerWalletRateLimiter {
    /// Create a new rate limiter with default config
    pub fn new(tps: u32) -> Self {
        let capacity = (tps as u64) * 2;
        Self {
            buckets: Mutex::new(HashMap::new()),
            global_bucket: Mutex::new(TokenBucket::new(capacity, tps as u64)),
            config: Mutex::new(RateLimiterConfig {
                tps,
                ..Default::default()
            }),
            backoff_ms: Mutex::new(HashMap::new()),
        }
    }

    /// Create a rate limiter with custom config
    pub fn with_config(config: RateLimiterConfig) -> Self {
        let capacity = (config.tps as u64) * config.burst_multiplier as u64;
        Self {
            buckets: Mutex::new(HashMap::new()),
            global_bucket: Mutex::new(TokenBucket::new(capacity, config.tps as u64)),
            config: Mutex::new(config),
            backoff_ms: Mutex::new(HashMap::new()),
        }
    }

    fn get_or_create_bucket(&self, wallet_id: &str) -> Arc<TokenBucket> {
        let mut buckets = self.buckets.lock().unwrap();
        let config = self.config.lock().unwrap();
        let capacity = (config.tps as u64) * config.burst_multiplier as u64;

        if let Some(bucket) = buckets.get(wallet_id) {
            return bucket.clone();
        }

        let bucket = Arc::new(TokenBucket::new(capacity, config.tps as u64));
        buckets.insert(wallet_id.to_string(), bucket.clone());
        bucket
    }

    /// Try to acquire a rate limit slot for the given wallet
    pub async fn acquire(&self, wallet_id: &str) -> bool {
        let backoff = {
            let backoffs = self.backoff_ms.lock().unwrap();
            backoffs.get(wallet_id).copied().unwrap_or(0)
        };

        if backoff > 0 {
            debug!("Wallet {} is in backoff for {}ms", wallet_id, backoff);
            sleep(Duration::from_millis(backoff)).await;
            let mut backoffs = self.backoff_ms.lock().unwrap();
            backoffs.remove(wallet_id);
        }

        let wallet_bucket = self.get_or_create_bucket(wallet_id);
        let global_bucket = self.global_bucket.lock().unwrap();

        if wallet_bucket.try_acquire(1) && global_bucket.try_acquire(1) {
            return true;
        }

        false
    }

    /// Acquire a slot, waiting if necessary
    pub async fn acquire_with_wait(&self, wallet_id: &str) {
        let config = self.config.lock().unwrap();
        let delay_ms = 1000 / config.tps.max(1) as u64;
        drop(config);

        while !self.acquire(wallet_id).await {
            sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    /// Handle a 429 (rate limited) response for the given wallet
    pub fn on_429(&self, wallet_id: &str) {
        let mut backoffs = self.backoff_ms.lock().unwrap();
        let config = self.config.lock().unwrap();
        let current = backoffs.get(wallet_id).copied().unwrap_or(100);

        let new_backoff = (current * config.backoff_factor as u64).min(config.max_backoff_ms);
        backoffs.insert(wallet_id.to_string(), new_backoff);

        debug!(
            "Wallet {} received 429, backing off for {}ms",
            wallet_id, new_backoff
        );
    }

    /// Clear backoff after successful request
    pub fn on_success(&self, wallet_id: &str) {
        let mut backoffs = self.backoff_ms.lock().unwrap();
        backoffs.remove(wallet_id);
    }

    /// Update TPS setting at runtime
    pub fn set_tps(&self, tps: u32) {
        let mut config = self.config.lock().unwrap();
        config.tps = tps;
        let capacity = (tps as u64) * config.burst_multiplier as u64;
        drop(config);

        let mut global_bucket = self.global_bucket.lock().unwrap();
        *global_bucket = TokenBucket::new(capacity, tps as u64);
    }

    /// Get current TPS setting
    pub fn current_tps(&self) -> u32 {
        self.config.lock().unwrap().tps
    }

    /// Get number of tracked wallets
    pub fn wallet_count(&self) -> usize {
        self.buckets.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_bucket_acquire() {
        let bucket = TokenBucket::new(10, 10);
        assert!(bucket.try_acquire(5));
        assert_eq!(bucket.available(), 5);
    }

    #[tokio::test]
    async fn test_rate_limiter_per_wallet() {
        let limiter = PerWalletRateLimiter::new(10);
        assert!(limiter.acquire("wallet1").await);
        assert!(limiter.acquire("wallet2").await);
        assert_eq!(limiter.wallet_count(), 2);
    }
}
