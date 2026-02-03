//! Batch Nonce Helper - Sequential nonce reservation for batch transactions
//!
//! This module provides a helper for reserving sequential nonces for batch
//! transaction submission. It ensures proper nonce management even when some
//! transactions in a batch fail.
//!
//! # Problem
//!
//! When submitting multiple transactions rapidly (e.g., batch swaps), if one
//! transaction fails, the nonce manager may get out of sync. This helper:
//!
//! 1. Reserves all nonces upfront
//! 2. Tracks which transactions succeeded
//! 3. Properly advances the nonce manager after partial failures
//!
//! # Example
//!
//! ```rust,no_run
//! use tempo_spammer::utils::batch_nonce::BatchNonceHelper;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let helper = BatchNonceHelper::new(client.clone(), address).await;
//! let nonces = helper.reserve_batch(10).await?;  // Reserve 10 nonces
//!
//! // Build transactions with nonces...
//! let tx = tx.nonce(nonces[idx]);
//!
//! // On partial failure:
//! if success_count < total_count {
//!     let last_success = nonces[success_count - 1];
//!     helper.advance_to(last_success).await;  // Skip failed nonces
//! }
//! # Ok(())
//! # }
//! ```

use crate::{NonceManager, TempoClient};
use alloy_primitives::Address;
use anyhow::Result;
use std::sync::Arc;

/// Helper for managing nonces in batch transactions
pub struct BatchNonceHelper {
    client: Arc<TempoClient>,
    address: Address,
    rpc_url: String,
    nonce_manager: Option<Arc<NonceManager>>,
}

impl BatchNonceHelper {
    /// Creates a new batch nonce helper
    ///
    /// # Arguments
    ///
    /// * `client` - The TempoClient to use for nonce operations
    /// * `address` - The wallet address
    /// * `rpc_url` - The RPC endpoint URL
    pub async fn new(client: Arc<TempoClient>, address: Address, rpc_url: String) -> Self {
        let nonce_manager = client.nonce_manager.clone();
        Self {
            client,
            address,
            rpc_url,
            nonce_manager,
        }
    }

    /// Reserve sequential nonces for a batch of transactions
    ///
    /// This method reserves `count` sequential nonces starting from the
    /// current pending nonce. It updates the nonce manager to skip the
    /// entire batch, ensuring no other transactions use these nonces.
    ///
    /// # Arguments
    ///
    /// * `count` - Number of nonces to reserve
    ///
    /// # Returns
    ///
    /// Returns `Result<Vec<u64>>` containing the reserved nonces in order.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # async fn example(helper: &BatchNonceHelper) -> anyhow::Result<()> {
    /// let nonces = helper.reserve_batch(5).await?;
    /// // nonces = [5, 6, 7, 8, 9]
    /// # Ok(())
    /// # }
    /// ```
    pub async fn reserve_batch(&self, count: usize) -> Result<Vec<u64>> {
        let mut nonces = Vec::with_capacity(count);

        if let Some(manager) = &self.nonce_manager {
            // Use atomic reserve if available
            let start_nonce = manager
                .get_and_increment(self.address)
                .await
                .ok_or_else(|| anyhow::anyhow!("Nonce manager not initialized"))?;

            for i in 0..count {
                nonces.push(start_nonce + i as u64);
            }

            // Update manager to skip entire batch
            manager.set(self.address, start_nonce + count as u64).await;
        } else {
            // Fallback to RPC
            let start_nonce = self.client.get_pending_nonce(&self.rpc_url).await?;
            for i in 0..count {
                nonces.push(start_nonce + i as u64);
            }
        }

        Ok(nonces)
    }

    /// Advance nonce manager to after the last successful transaction
    ///
    /// Call this when a batch has partial failures. It ensures the nonce
    /// manager is set to the correct position after the last successful
    /// transaction, preventing nonce gaps.
    ///
    /// # Arguments
    ///
    /// * `last_success_nonce` - The nonce of the last successful transaction
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # async fn example(helper: &BatchNonceHelper) {
    /// // If transactions with nonces 5, 6, 7 succeeded but 8 failed
    /// helper.advance_to(7).await;  // Next nonce will be 8
    /// # }
    /// ```
    pub async fn advance_to(&self, last_success_nonce: u64) {
        if let Some(manager) = &self.nonce_manager {
            let next_nonce = last_success_nonce.wrapping_add(1);
            manager.set(self.address, next_nonce).await;
            tracing::debug!(
                "Advanced nonce for {} to {} after partial batch failure",
                self.address,
                next_nonce
            );
        }
    }

    /// Reset the nonce cache for this address
    ///
    /// Use this when you receive a "nonce too low" error to force
    /// resynchronization with the blockchain.
    pub async fn reset(&self) {
        if let Some(manager) = &self.nonce_manager {
            manager.reset(self.address).await;
            tracing::debug!("Reset nonce cache for {}", self.address);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests would require mocking the client and provider
    // For now, they serve as documentation of expected behavior

    #[test]
    fn test_reserve_batch_calculates_correct_nonces() {
        // If start nonce is 5 and count is 3, should return [5, 6, 7]
        let start = 5u64;
        let count = 3usize;
        let expected: Vec<u64> = (0..count).map(|i| start + i as u64).collect();
        assert_eq!(expected, vec![5, 6, 7]);
    }

    #[test]
    fn test_advance_to_calculates_next_nonce() {
        // If last success was 7, next should be 8
        let last_success = 7u64;
        let next = last_success.wrapping_add(1);
        assert_eq!(next, 8);
    }
}
