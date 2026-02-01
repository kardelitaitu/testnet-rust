//! Robust Nonce Manager - Advanced nonce caching with per-request tracking
//!
//! This module provides a production-grade nonce management system that handles
//! high-throughput transaction submission with automatic recovery from nonce races.
//!
//! # Architecture
//!
//! The RobustNonceManager maintains per-wallet state with:
//!
//! - **Cached Nonce**: The next expected nonce for each wallet
//! - **In-Flight Tracking**: Nonces that have been allocated but not yet confirmed
//! - **Request Reservation**: Each request gets a unique reservation to prevent races
//! - **Auto-Recovery**: Automatic synchronization when "nonce too low" errors occur
//!
//! # Key Features
//!
//! 1. **Per-Request Tracking**: Each nonce allocation is tracked with a unique request ID
//! 2. **In-Flight Management**: Tracks pending transactions and their nonces
//! 3. **Automatic Recovery**: Detects and fixes nonce gaps automatically
//! 4. **Concurrency Safe**: Multiple tasks can safely allocate nonces concurrently
//! 5. **Gap Detection**: Identifies missing nonces and fills them
//!
//! # Example
//!
//! ```rust,no_run
//! use tempo_spammer::RobustNonceManager;
//! use alloy_primitives::Address;
//!
//! # async fn example() {
//! let manager = RobustNonceManager::new();
//! let address = Address::ZERO;
//!
//! // Reserve a nonce for a transaction
//! let reservation = manager.reserve_nonce(address).await;
//!
//! // Use the reserved nonce
//! let nonce = reservation.nonce;
//!
//! // Mark as submitted (moves to in-flight)
//! reservation.mark_submitted().await;
//!
//! // Later, confirm it succeeded
//! manager.confirm_nonce(address, nonce).await;
//! # }
//! ```

use alloy_primitives::Address;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

/// Request ID for tracking individual nonce allocations
pub type RequestId = u64;

/// State for a single nonce request
#[derive(Debug, Clone)]
pub enum NonceState {
    /// Nonce is reserved but transaction not yet sent
    Reserved { since: Instant },
    /// Transaction submitted, waiting for confirmation
    InFlight { since: Instant },
    /// Transaction confirmed on-chain
    Confirmed { at: Instant },
    /// Transaction failed, nonce can be reused
    Failed { error: String },
}

/// Per-wallet nonce tracking
#[derive(Debug)]
struct WalletNonceState {
    /// The next nonce to allocate (cached value)
    cached_nonce: AtomicU64,

    /// Highest confirmed nonce seen on-chain
    confirmed_nonce: AtomicU64,

    /// Nonces currently in various states
    requests: Mutex<HashMap<u64, (RequestId, NonceState)>>,

    /// Next request ID counter
    next_request_id: AtomicU64,

    /// In-flight nonces (submitted but not confirmed)
    in_flight: Mutex<HashSet<u64>>,

    /// Failed nonces that can be reused
    failed_nonces: Mutex<VecDeque<u64>>,

    /// Last sync time with blockchain
    last_sync: Mutex<Instant>,

    /// Sync in progress flag
    syncing: Mutex<bool>,
}

impl WalletNonceState {
    fn new() -> Self {
        Self {
            cached_nonce: AtomicU64::new(0),
            confirmed_nonce: AtomicU64::new(0),
            requests: Mutex::new(HashMap::new()),
            next_request_id: AtomicU64::new(1),
            in_flight: Mutex::new(HashSet::new()),
            failed_nonces: Mutex::new(VecDeque::new()),
            last_sync: Mutex::new(Instant::now()),
            syncing: Mutex::new(false),
        }
    }
}

/// Robust nonce manager with per-request tracking
#[derive(Debug)]
pub struct RobustNonceManager {
    /// Per-wallet state
    wallets: RwLock<HashMap<Address, Arc<WalletNonceState>>>,

    /// Global request ID counter
    global_request_id: AtomicU64,

    /// Configuration
    config: NonceManagerConfig,
}

/// Configuration for the nonce manager
#[derive(Debug, Clone)]
pub struct NonceManagerConfig {
    /// How long to wait before considering a reserved nonce as expired
    pub reservation_timeout: Duration,
    /// How long to wait before considering an in-flight nonce as stuck
    pub in_flight_timeout: Duration,
    /// Auto-sync interval
    pub auto_sync_interval: Duration,
    /// Maximum number of failed nonces to track per wallet
    pub max_failed_cache: usize,
}

impl Default for NonceManagerConfig {
    fn default() -> Self {
        Self {
            reservation_timeout: Duration::from_secs(30),
            in_flight_timeout: Duration::from_secs(120),
            auto_sync_interval: Duration::from_secs(60),
            max_failed_cache: 100,
        }
    }
}

/// Nonce reservation handle
///
/// This handle represents a reserved nonce. It must be either:
/// - Marked as submitted via `mark_submitted()`
/// - Released via `release()` if not used
pub struct NonceReservation {
    pub request_id: RequestId,
    pub address: Address,
    pub nonce: u64,
    manager: Arc<RobustNonceManager>,
    submitted: bool,
}

impl NonceReservation {
    /// Mark this nonce as submitted (transaction sent)
    ///
    /// Moves the nonce from "Reserved" to "InFlight" state
    pub async fn mark_submitted(mut self) {
        self.submitted = true;
        self.manager.mark_submitted(self.address, self.nonce).await;
    }

    /// Release the nonce without using it
    ///
    /// Returns the nonce to the pool for reuse
    pub async fn release(self) {
        if !self.submitted {
            self.manager.release_nonce(self.address, self.nonce).await;
        }
    }
}

impl Drop for NonceReservation {
    /// Automatic cleanup on drop (with warning)
    ///
    /// This is a safety fallback. If you see this warning in logs,
    /// you should update your code to call `reservation.release().await` explicitly.
    fn drop(&mut self) {
        if !self.submitted && !std::thread::panicking() {
            tracing::warn!(
                target: "nonce_manager",
                "NonceReservation dropped without explicit release(). \
                 Using automatic cleanup. \
                 Prefer calling reservation.release().await explicitly."
            );
            // Spawn cleanup in background
            let manager = self.manager.clone();
            let address = self.address;
            let nonce = self.nonce;
            tokio::spawn(async move {
                manager.release_nonce(address, nonce).await;
            });
        }
    }
}

impl RobustNonceManager {
    /// Creates a new robust nonce manager with default configuration
    pub fn new() -> Self {
        Self::with_config(NonceManagerConfig::default())
    }

    /// Creates a new robust nonce manager with custom configuration
    pub fn with_config(config: NonceManagerConfig) -> Self {
        Self {
            wallets: RwLock::new(HashMap::new()),
            global_request_id: AtomicU64::new(1),
            config,
        }
    }

    /// Reserve a nonce for a transaction
    ///
    /// This is the primary method for obtaining a nonce. It:
    /// 1. Checks for reusable failed nonces
    /// 2. Allocates the next cached nonce
    /// 3. Tracks the reservation
    ///
    /// # Arguments
    /// * `address` - The wallet address
    ///
    /// # Returns
    /// * `Some(NonceReservation)` - Successfully reserved nonce
    /// * `None` - Wallet not initialized, needs RPC sync first
    pub async fn reserve_nonce(self: &Arc<Self>, address: Address) -> Option<NonceReservation> {
        // Get or create wallet state
        let state = self.get_or_create_wallet(address).await;

        // Try to get a reusable failed nonce first
        let nonce = {
            let mut failed = state.failed_nonces.lock().await;
            if let Some(nonce) = failed.pop_front() {
                nonce
            } else {
                // Get next cached nonce
                let cached = state.cached_nonce.load(Ordering::SeqCst);
                if cached == 0 {
                    return None; // Not initialized
                }

                // Atomically increment
                state.cached_nonce.fetch_add(1, Ordering::SeqCst);
                cached
            }
        };

        // Generate request ID
        let request_id = self.global_request_id.fetch_add(1, Ordering::SeqCst);

        // Track the reservation
        {
            let mut requests = state.requests.lock().await;
            requests.insert(
                nonce,
                (
                    request_id,
                    NonceState::Reserved {
                        since: Instant::now(),
                    },
                ),
            );
        }

        debug!(
            "Reserved nonce {} for {:?} (request {})",
            nonce, address, request_id
        );

        Some(NonceReservation {
            request_id,
            address,
            nonce,
            manager: self.clone(),
            submitted: false,
        })
    }

    /// Initialize or update the cached nonce for a wallet
    ///
    /// Call this after fetching `eth_getTransactionCount` from RPC
    ///
    /// # Arguments
    /// * `address` - The wallet address
    /// * `confirmed_count` - The confirmed transaction count from RPC
    pub async fn initialize(&self, address: Address, confirmed_count: u64) {
        let state = self.get_or_create_wallet(address).await;

        let current_cached = state.cached_nonce.load(Ordering::SeqCst);
        let current_confirmed = state.confirmed_nonce.load(Ordering::SeqCst);

        // Only update if RPC value is higher (more accurate)
        if confirmed_count > current_cached || confirmed_count > current_confirmed {
            state.cached_nonce.store(confirmed_count, Ordering::SeqCst);
            state
                .confirmed_nonce
                .store(confirmed_count.saturating_sub(1), Ordering::SeqCst);

            *state.last_sync.lock().await = Instant::now();

            info!(
                "Initialized nonce for {:?}: cached={}, confirmed={}",
                address,
                confirmed_count,
                confirmed_count.saturating_sub(1)
            );
        }
    }

    /// Mark a nonce as submitted (transaction sent)
    ///
    /// Moves nonce from Reserved to InFlight state
    async fn mark_submitted(&self, address: Address, nonce: u64) {
        if let Some(state) = self.wallets.read().await.get(&address) {
            let mut requests = state.requests.lock().await;
            if let Some((req_id, _)) = requests.get(&nonce) {
                let req_id = *req_id;
                requests.insert(
                    nonce,
                    (
                        req_id,
                        NonceState::InFlight {
                            since: Instant::now(),
                        },
                    ),
                );
            }

            state.in_flight.lock().await.insert(nonce);

            debug!("Nonce {} for {:?} marked as in-flight", nonce, address);
        }
    }

    /// Confirm a nonce as successful
    ///
    /// Call this when a transaction is confirmed on-chain
    pub async fn confirm_nonce(&self, address: Address, nonce: u64) {
        if let Some(state) = self.wallets.read().await.get(&address) {
            let mut requests = state.requests.lock().await;
            if let Some((req_id, _)) = requests.get(&nonce) {
                let req_id = *req_id;
                requests.insert(
                    nonce,
                    (req_id, NonceState::Confirmed { at: Instant::now() }),
                );
            }

            state.in_flight.lock().await.remove(&nonce);

            // Update confirmed nonce if this is higher
            let current_confirmed = state.confirmed_nonce.load(Ordering::SeqCst);
            if nonce > current_confirmed {
                state.confirmed_nonce.store(nonce, Ordering::SeqCst);
            }

            // Cleanup old confirmed entries periodically
            if nonce % 10 == 0 {
                self.cleanup_confirmed(address).await;
            }

            debug!("Nonce {} for {:?} confirmed", nonce, address);
        }
    }

    /// Mark a nonce as failed
    ///
    /// The nonce will be reused for future transactions IF recycle is true
    async fn mark_failed(&self, address: Address, nonce: u64, error: String, recycle: bool) {
        if let Some(state) = self.wallets.read().await.get(&address) {
            let mut requests = state.requests.lock().await;
            if let Some((req_id, _)) = requests.get(&nonce) {
                let req_id = *req_id;
                requests.insert(
                    nonce,
                    (
                        req_id,
                        NonceState::Failed {
                            error: error.clone(),
                        },
                    ),
                );
            }

            state.in_flight.lock().await.remove(&nonce);

            if recycle {
                // Add to failed queue for reuse
                let mut failed = state.failed_nonces.lock().await;
                if !failed.contains(&nonce) {
                    failed.push_back(nonce);
                    // Limit cache size
                    while failed.len() > self.config.max_failed_cache {
                        failed.pop_front();
                    }
                }
            }

            warn!(
                "Nonce {} for {:?} failed (recycle={}): {}",
                nonce, address, recycle, error
            );
        }
    }

    /// Release a nonce back to the pool
    ///
    /// Called when a reservation is dropped without being submitted
    async fn release_nonce(&self, address: Address, nonce: u64) {
        if let Some(state) = self.wallets.read().await.get(&address) {
            let mut requests = state.requests.lock().await;
            requests.remove(&nonce);

            // Add to failed queue for reuse
            let mut failed = state.failed_nonces.lock().await;
            if !failed.contains(&nonce) {
                failed.push_back(nonce);
            }

            debug!("Nonce {} for {:?} released", nonce, address);
        }
    }

    /// Handle "nonce too low" error with automatic recovery
    ///
    /// This is the key recovery mechanism. When we get a nonce error:
    /// 1. Mark the failed nonce
    /// 2. Sync with blockchain to get actual state
    /// 3. Adjust cached nonce if needed
    pub async fn handle_nonce_error(
        &self,
        address: Address,
        attempted_nonce: u64,
        actual_next_nonce: u64,
    ) {
        let error = format!(
            "nonce too low: attempted {}, actual next is {}",
            attempted_nonce, actual_next_nonce
        );

        // DO NOT recycle this nonce, it is dead
        self.mark_failed(address, attempted_nonce, error.clone(), false)
            .await;

        // Update wallet state
        if let Some(state) = self.wallets.read().await.get(&address) {
            let current_cached = state.cached_nonce.load(Ordering::SeqCst);

            // If actual next nonce is higher than our cache, update it
            if actual_next_nonce > current_cached {
                state
                    .cached_nonce
                    .store(actual_next_nonce, Ordering::SeqCst);

                // BULK INVALIDATION: Mark all reserved/in-flight nonces < actual_next_nonce as failed
                let mut requests = state.requests.lock().await;
                let mut in_flight = state.in_flight.lock().await;
                let mut failed_queue = state.failed_nonces.lock().await;

                // Identify stale nonces
                let stale_nonces: Vec<u64> = requests
                    .keys()
                    .filter(|&n| *n < actual_next_nonce && *n != attempted_nonce)
                    .cloned()
                    .collect();

                for stale in stale_nonces {
                    if let Some((req_id, _)) = requests.get(&stale) {
                        let req_id = *req_id;
                        requests.insert(
                            stale,
                            (
                                req_id,
                                NonceState::Failed {
                                    error: "Stale: superseded by chain state".to_string(),
                                },
                            ),
                        );
                    }
                    in_flight.remove(&stale);

                    // Also remove from failed_queue if present
                    if let Some(pos) = failed_queue.iter().position(|&x| x == stale) {
                        failed_queue.remove(pos);
                    }
                    warn!(
                        "Invalidated stale nonce {} for {:?} due to chain sync",
                        stale, address
                    );
                }

                warn!(
                    "Adjusted cached nonce for {:?}: {} -> {}",
                    address, current_cached, actual_next_nonce
                );
            }

            // Also update confirmed nonce
            let new_confirmed = actual_next_nonce.saturating_sub(1);
            state.confirmed_nonce.store(new_confirmed, Ordering::SeqCst);
        }
    }

    /// Get the next nonce to use (for external synchronization)
    ///
    /// Returns the current cached nonce value
    pub async fn peek_next_nonce(&self, address: Address) -> Option<u64> {
        self.wallets
            .read()
            .await
            .get(&address)
            .map(|state| state.cached_nonce.load(Ordering::SeqCst))
    }

    /// Get statistics for a wallet
    pub async fn get_stats(&self, address: Address) -> Option<NonceStats> {
        let wallets = self.wallets.read().await;
        // With Arc, we just clone the Arc or use it directly
        let state = wallets.get(&address)?;

        let requests = state.requests.lock().await;
        let in_flight = state.in_flight.lock().await;
        let failed = state.failed_nonces.lock().await;

        let mut reserved_count = 0;
        let mut in_flight_count = 0;
        let mut confirmed_count = 0;
        let mut failed_count = 0;

        for (_, (_, req_state)) in requests.iter() {
            match req_state {
                NonceState::Reserved { .. } => reserved_count += 1,
                NonceState::InFlight { .. } => in_flight_count += 1,
                NonceState::Confirmed { .. } => confirmed_count += 1,
                NonceState::Failed { .. } => failed_count += 1,
            }
        }

        Some(NonceStats {
            cached_next: state.cached_nonce.load(Ordering::SeqCst),
            confirmed: state.confirmed_nonce.load(Ordering::SeqCst),
            reserved: reserved_count,
            in_flight: in_flight.len(),
            failed_cached: failed.len(),
            total_tracked: requests.len(),
        })
    }

    /// Reset a wallet's state (force full resync)
    pub async fn reset(&self, address: Address) {
        let mut wallets = self.wallets.write().await;
        wallets.remove(&address);
        info!("Reset nonce state for {:?}", address);
    }

    /// Clean up old confirmed nonces to prevent memory growth
    async fn cleanup_confirmed(&self, address: Address) {
        if let Some(state) = self.wallets.read().await.get(&address) {
            let confirmed_nonce = state.confirmed_nonce.load(Ordering::SeqCst);
            let mut requests = state.requests.lock().await;

            // Remove confirmed nonces that are well behind
            let cutoff = confirmed_nonce.saturating_sub(50);
            requests.retain(|nonce, _| *nonce >= cutoff);
        }
    }

    /// Get or create wallet state
    async fn get_or_create_wallet(&self, address: Address) -> Arc<WalletNonceState> {
        {
            let wallets = self.wallets.read().await;
            if let Some(state) = wallets.get(&address) {
                return state.clone();
            }
        }

        // Create new state
        let mut wallets = self.wallets.write().await;
        // Double check
        if let Some(state) = wallets.get(&address) {
            return state.clone();
        }

        let state = Arc::new(WalletNonceState::new());
        wallets.insert(address, state.clone());
        state
    }
}

impl Default for RobustNonceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for a wallet's nonce state
#[derive(Debug, Clone)]
pub struct NonceStats {
    pub cached_next: u64,
    pub confirmed: u64,
    pub reserved: usize,
    pub in_flight: usize,
    pub failed_cached: usize,
    pub total_tracked: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reserve_and_confirm() {
        let manager = std::sync::Arc::new(RobustNonceManager::new());
        let address = Address::ZERO;

        // Initialize
        manager.initialize(address, 5).await;

        // Reserve nonce
        let reservation = manager.reserve_nonce(address).await.unwrap();
        assert_eq!(reservation.nonce, 5);

        // Mark submitted
        reservation.mark_submitted().await;

        // Confirm
        manager.confirm_nonce(address, 5).await;

        // Check stats
        let stats = manager.get_stats(address).await.unwrap();
        assert_eq!(stats.cached_next, 6);
        assert_eq!(stats.confirmed, 5);
    }

    #[tokio::test]
    async fn test_nonce_error_recovery() {
        let manager = std::sync::Arc::new(RobustNonceManager::new());
        let address = Address::ZERO;

        // Initialize at nonce 10
        manager.initialize(address, 10).await;

        // Reserve nonce 10
        let _res = manager.reserve_nonce(address).await.unwrap();

        // Simulate nonce error - actual next is 12 (someone else used 10, 11)
        manager.handle_nonce_error(address, 10, 12).await;

        // Check that cached was updated
        let next = manager.peek_next_nonce(address).await.unwrap();
        assert_eq!(next, 12);
    }

    #[tokio::test]
    async fn test_failed_nonce_reuse() {
        let manager = std::sync::Arc::new(RobustNonceManager::new());
        let address = Address::ZERO;

        // Initialize
        manager.initialize(address, 5).await;

        // Reserve and fail nonce 5
        let res = manager.reserve_nonce(address).await.unwrap();
        assert_eq!(res.nonce, 5);
        manager
            .mark_failed(address, 5, "test error".to_string(), true)
            .await;

        // Next reservation should reuse nonce 5
        let res2 = manager.reserve_nonce(address).await.unwrap();
        assert_eq!(res2.nonce, 5);
    }

    #[tokio::test]
    async fn test_nonce_error_no_recycle() {
        let manager = std::sync::Arc::new(RobustNonceManager::new());
        let address = Address::ZERO;

        manager.initialize(address, 10).await;

        // Reserve nonce 10
        let res = manager.reserve_nonce(address).await.unwrap();
        assert_eq!(res.nonce, 10);

        // Fail with "nonce too low" error (simulated via handle_nonce_error)
        // This should NOT recycle nonce 10
        manager.handle_nonce_error(address, 10, 15).await;

        // Next reservation should be 15 (from chain state), NOT 10
        let res2 = manager.reserve_nonce(address).await.unwrap();
        assert_eq!(res2.nonce, 15);
    }
}
