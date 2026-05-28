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
//! Uses a [`RwLock`] to ensure atomic read-modify-write operations on the nonce cache.
//! This allows multiple concurrent tasks to safely acquire nonces for the same wallet,
//! with concurrent reads and exclusive writes.
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
use tokio::sync::RwLock;

/// Thread-safe nonce cache for multiple wallets
///
/// Maintains a mapping of wallet addresses to their next expected nonce.
/// All operations are atomic and thread-safe.
///
/// # Implementation Details
///
/// - Uses [`RwLock<HashMap>`] for thread-safe access with concurrent reads
/// - Stores the NEXT nonce to use (not the current transaction count)
/// - Lazy initialization - nonces are only cached after first use
#[derive(Debug, Default)]
pub struct NonceManager {
    /// Maps wallet address to the NEXT nonce to use
    nonces: RwLock<HashMap<Address, u64>>,
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
            nonces: RwLock::new(HashMap::new()),
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
        let mut map = self.nonces.write().await;
        if let Some(nonce) = map.get_mut(&address) {
            let current = *nonce;
            *nonce += 1;
            Some(current)
        } else {
            None
        }
    }

    /// Gets the current cached nonce for an address without incrementing
    ///
    /// This is useful for read-only operations where you need to know
    /// the next nonce without consuming it.
    ///
    /// # Arguments
    ///
    /// * `address` - The wallet address to peek
    ///
    /// # Returns
    ///
    /// - `Some(nonce)` - The next nonce to use for this address
    /// - `None` - Address not in cache
    pub async fn peek(&self, address: Address) -> Option<u64> {
        let map = self.nonces.read().await;
        map.get(&address).copied()
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
        let mut map = self.nonces.write().await;
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
        let mut map = self.nonces.write().await;
        map.remove(&address);
    }

    /// Clears all cached nonces to free memory
    pub async fn clear(&self) {
        let mut map = self.nonces.write().await;
        let count = map.len();
        map.clear();
        tracing::debug!("Cleared NonceManager cache ({} addresses)", count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    fn addr1() -> Address {
        address!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    }

    fn addr2() -> Address {
        address!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    }

    #[tokio::test]
    async fn test_new_empty() {
        let mgr = NonceManager::new();
        assert_eq!(mgr.get_and_increment(addr1()).await, None);
        assert_eq!(mgr.peek(addr1()).await, None);
    }

    #[tokio::test]
    async fn test_set_and_get() {
        let mgr = NonceManager::new();
        mgr.set(addr1(), 42).await;
        assert_eq!(mgr.get_and_increment(addr1()).await, Some(42));
    }

    #[tokio::test]
    async fn test_get_and_increment_increases() {
        let mgr = NonceManager::new();
        mgr.set(addr1(), 5).await;
        assert_eq!(mgr.get_and_increment(addr1()).await, Some(5));
        assert_eq!(mgr.get_and_increment(addr1()).await, Some(6));
        assert_eq!(mgr.get_and_increment(addr1()).await, Some(7));
    }

    #[tokio::test]
    async fn test_multiple_addresses_independent() {
        let mgr = NonceManager::new();
        mgr.set(addr1(), 100).await;
        mgr.set(addr2(), 200).await;
        assert_eq!(mgr.get_and_increment(addr1()).await, Some(100));
        assert_eq!(mgr.get_and_increment(addr2()).await, Some(200));
        assert_eq!(mgr.get_and_increment(addr1()).await, Some(101));
        assert_eq!(mgr.get_and_increment(addr2()).await, Some(201));
    }

    #[tokio::test]
    async fn test_peek_does_not_increment() {
        let mgr = NonceManager::new();
        mgr.set(addr1(), 10).await;
        assert_eq!(mgr.peek(addr1()).await, Some(10));
        assert_eq!(mgr.peek(addr1()).await, Some(10)); // Still 10
        assert_eq!(mgr.get_and_increment(addr1()).await, Some(10)); // Now 10 consumed
        assert_eq!(mgr.peek(addr1()).await, Some(11)); // Now 11
    }

    #[tokio::test]
    async fn test_reset_removes_address() {
        let mgr = NonceManager::new();
        mgr.set(addr1(), 5).await;
        assert_eq!(mgr.get_and_increment(addr1()).await, Some(5));
        mgr.reset(addr1()).await;
        assert_eq!(mgr.get_and_increment(addr1()).await, None);
    }

    #[tokio::test]
    async fn test_clear_removes_all() {
        let mgr = NonceManager::new();
        mgr.set(addr1(), 1).await;
        mgr.set(addr2(), 2).await;
        mgr.clear().await;
        assert_eq!(mgr.get_and_increment(addr1()).await, None);
        assert_eq!(mgr.get_and_increment(addr2()).await, None);
    }

    #[tokio::test]
    async fn test_default_is_empty() {
        let mgr: NonceManager = Default::default();
        assert_eq!(mgr.get_and_increment(addr1()).await, None);
    }

    #[tokio::test]
    async fn test_set_overwrites_existing() {
        let mgr = NonceManager::new();
        mgr.set(addr1(), 10).await;
        assert_eq!(mgr.get_and_increment(addr1()).await, Some(10));
        mgr.set(addr1(), 50).await; // Overwrite
        assert_eq!(mgr.get_and_increment(addr1()).await, Some(50));
    }

    #[tokio::test]
    async fn test_concurrent_get_and_increment_unique_sequential() {
        let mgr = std::sync::Arc::new(NonceManager::new());
        let start = 100u64;
        mgr.set(addr1(), start).await;

        let num_tasks = 20;
        let mut handles = Vec::with_capacity(num_tasks);

        for _ in 0..num_tasks {
            let mgr = mgr.clone();
            let addr = addr1();
            handles.push(tokio::spawn(async move { mgr.get_and_increment(addr).await }));
        }

        let mut results: Vec<u64> = Vec::with_capacity(num_tasks);
        for h in handles {
            let r = h.await.unwrap();
            assert!(r.is_some(), "All concurrent calls should return Some");
            results.push(r.unwrap());
        }

        // All nonces should be unique
        let mut sorted = results.clone();
        sorted.sort();
        let expected: Vec<u64> = (start..start + num_tasks as u64).collect();
        assert_eq!(
            sorted,
            expected,
            "Nonces should be sequential from {} to {}",
            start,
            start + num_tasks as u64 - 1
        );
        assert_eq!(
            mgr.peek(addr1()).await,
            Some(start + num_tasks as u64),
            "Final cached value should reflect all consumed nonces"
        );
    }

    #[tokio::test]
    async fn test_concurrent_many_tasks_same_address() {
        let mgr = std::sync::Arc::new(NonceManager::new());
        mgr.set(addr1(), 0).await;

        let num_tasks = 50;
        let mut handles = Vec::with_capacity(num_tasks);

        for _ in 0..num_tasks {
            let mgr = mgr.clone();
            let addr = addr1();
            handles.push(tokio::spawn(async move { mgr.get_and_increment(addr).await }));
        }

        let mut results: Vec<u64> = Vec::with_capacity(num_tasks);
        for h in handles {
            if let Some(n) = h.await.unwrap() {
                results.push(n);
            }
        }

        assert_eq!(results.len(), 50, "All 50 tasks should get a nonce");
        results.sort();
        let expected: Vec<u64> = (0..50).collect();
        assert_eq!(results, expected, "50 tasks should get nonces 0..49");
    }

    #[tokio::test]
    async fn test_concurrent_different_addresses_independent() {
        let mgr = std::sync::Arc::new(NonceManager::new());
        mgr.set(addr1(), 0).await;
        mgr.set(addr2(), 0).await;

        let mut handles = Vec::new();

        // Spawn 10 tasks for addr1 and 10 for addr2 interleaved
        for _ in 0..10 {
            let mgr_a = mgr.clone();
            let a1 = addr1();
            handles.push(tokio::spawn(async move { (a1, mgr_a.get_and_increment(a1).await) }));
            let mgr_b = mgr.clone();
            let a2 = addr2();
            handles.push(tokio::spawn(async move { (a2, mgr_b.get_and_increment(a2).await) }));
        }

        let mut addr1_nonces = Vec::new();
        let mut addr2_nonces = Vec::new();
        for h in handles {
            let (addr, nonce) = h.await.unwrap();
            assert!(nonce.is_some());
            if addr == addr1() {
                addr1_nonces.push(nonce.unwrap());
            } else {
                addr2_nonces.push(nonce.unwrap());
            }
        }

        addr1_nonces.sort();
        addr2_nonces.sort();
        assert_eq!(
            addr1_nonces,
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
            "addr1 should get 0..9 independently"
        );
        assert_eq!(
            addr2_nonces,
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
            "addr2 should get 0..9 independently"
        );
    }
}
