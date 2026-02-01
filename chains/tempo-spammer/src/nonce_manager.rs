//! Nonce Manager - Thread-safe nonce caching for high-throughput transaction submission
//!
//! This module provides a caching layer for Ethereum transaction nonces, enabling
//! high-throughput transaction submission without requiring an RPC call for every
//! transaction.
//!
//! # Problem
//!
//! When submitting many transactions rapidly, each transaction needs a unique nonce.
//! Fetching the nonce from the RPC for every transaction creates significant overhead
//! and can become a bottleneck.
//!
//! # Solution
//!
//! The NonceManager maintains a local cache of the next expected nonce for each wallet:
//!
//! 1. **First Call**: Cache miss, fetch from RPC via `eth_getTransactionCount`
//! 2. **Cache Hit**: Return cached nonce and atomically increment
//! 3. **Reset**: Clear cache on "nonce too low" errors to resynchronize
//!
//! # Thread Safety
//!
//! Uses a [`Mutex`] to ensure atomic read-modify-write operations on the nonce cache.
//! This allows multiple concurrent tasks to safely acquire nonces for the same wallet.
//!
//! # Example
//!
//! ```rust,no_run
//! use tempo_spammer::NonceManager;
//! use alloy_primitives::Address;
//!
//! # async fn example() {
//! let manager = NonceManager::new();
//! let address = Address::ZERO;
//!
//! // Simulate RPC fetch on first use
//! let rpc_nonce = 5u64;
//! manager.set(address, rpc_nonce).await;
//!
//! // Subsequent calls use cache
//! let nonce1 = manager.get_and_increment(address).await; // Some(5)
//! let nonce2 = manager.get_and_increment(address).await; // Some(6)
//!
//! // Reset on error
//! manager.reset(address).await;
//! # }
//! ```
//!
//! # Integration with TempoClient
//!
//! The [`TempoClient`] optionally integrates with NonceManager:
//!
//! ```rust,no_run
//! use tempo_spammer::{TempoClient, NonceManager};
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let nonce_manager = Some(Arc::new(NonceManager::new()));
//!
//! let client = TempoClient::new_from_reqwest(
//!     "https://rpc.moderato.tempo.xyz",
//!     "0x...",
//!     reqwest::Client::new(),
//!     None,
//!     None,
//!     nonce_manager,
//! ).await?;
//!
//! // Client will now use nonce caching
//! # Ok(())
//! # }
//! ```

use alloy_primitives::Address;
use std::collections::HashMap;
use tokio::sync::Mutex;

/// Thread-safe nonce cache for multiple wallets
///
/// Maintains a mapping of wallet addresses to their next expected nonce.
/// All operations are atomic and thread-safe.
///
/// # Implementation Details
///
/// - Uses [`Mutex<HashMap>`] for thread-safe access
/// - Stores the NEXT nonce to use (not the current transaction count)
/// - Lazy initialization - nonces are only cached after first use
#[derive(Debug, Default)]
pub struct NonceManager {
    /// Maps wallet address to the NEXT nonce to use
    nonces: Mutex<HashMap<Address, u64>>,
}

impl NonceManager {
    /// Creates a new empty nonce manager
    ///
    /// Initializes with an empty cache. Nonces are added on first use.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tempo_spammer::NonceManager;
    ///
    /// let manager = NonceManager::new();
    /// ```
    pub fn new() -> Self {
        Self {
            nonces: Mutex::new(HashMap::new()),
        }
    }

    /// Gets the next nonce from cache and atomically increments it
    ///
    /// This is the primary method for acquiring nonces. If the address is in the cache,
    /// returns the cached nonce and increments the stored value for next time.
    ///
    /// # Arguments
    ///
    /// * `address` - The wallet address to get nonce for
    ///
    /// # Returns
    ///
    /// - `Some(nonce)` - The next nonce to use for this address
    /// - `None` - Address not in cache, needs initialization from RPC
    ///
    /// # Thread Safety
    ///
    /// This operation is atomic. Concurrent calls for the same address will receive
    /// unique, sequential nonces.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tempo_spammer::NonceManager;
    /// use alloy_primitives::Address;
    ///
    /// # async fn example() {
    /// let manager = NonceManager::new();
    /// let addr = Address::ZERO;
    ///
    /// // First, initialize from RPC
    /// manager.set(addr, 5).await;
    ///
    /// // Get nonces
    /// let n1 = manager.get_and_increment(addr).await.unwrap(); // 5
    /// let n2 = manager.get_and_increment(addr).await.unwrap(); // 6
    /// # }
    /// ```
    pub async fn get_and_increment(&self, address: Address) -> Option<u64> {
        let mut map = self.nonces.lock().await;
        if let Some(nonce) = map.get_mut(&address) {
            let current = *nonce;
            *nonce += 1;
            Some(current)
        } else {
            None
        }
    }

    /// Sets or updates the cached nonce for an address
    ///
    /// Call this after fetching the transaction count from RPC to initialize or
    /// update the cache. The value stored should be the NEXT nonce to use.
    ///
    /// # Arguments
    ///
    /// * `address` - The wallet address
    /// * `next_nonce` - The next nonce to use (typically from `eth_getTransactionCount`)
    ///
    /// # RPC Integration
    ///
    /// When fetching from RPC via `eth_getTransactionCount`, the result is the count
    /// of confirmed transactions, which equals the next usable nonce. Store this
    /// value directly.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tempo_spammer::NonceManager;
    /// use alloy_primitives::Address;
    ///
    /// # async fn example() {
    /// let manager = NonceManager::new();
    /// let addr = Address::ZERO;
    ///
    /// // After fetching from RPC: eth_getTransactionCount returns 5
    /// manager.set(addr, 5).await;
    ///
    /// // Next transaction will use nonce 5
    /// let nonce = manager.get_and_increment(addr).await; // Some(5)
    /// # }
    /// ```
    pub async fn set(&self, address: Address, next_nonce: u64) {
        let mut map = self.nonces.lock().await;
        map.insert(address, next_nonce);
    }

    /// Resets the cache for an address, forcing RPC fetch on next use
    ///
    /// Use this when:
    /// - You receive a "nonce too low" error (indicates cache is out of sync)
    /// - Transactions were submitted outside this manager
    /// - You want to force a resynchronization with the blockchain
    ///
    /// # Arguments
    ///
    /// * `address` - The wallet address to reset
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tempo_spammer::NonceManager;
    /// use alloy_primitives::Address;
    ///
    /// # async fn example() {
    /// let manager = NonceManager::new();
    /// let addr = Address::ZERO;
    ///
    /// // Initialize
    /// manager.set(addr, 5).await;
    ///
    /// // After a "nonce too low" error, reset
    /// manager.reset(addr).await;
    ///
    /// // Next call will return None, forcing RPC fetch
    /// let nonce = manager.get_and_increment(addr).await; // None
    /// # }
    /// ```
    pub async fn reset(&self, address: Address) {
        let mut map = self.nonces.lock().await;
        map.remove(&address);
    }
}
