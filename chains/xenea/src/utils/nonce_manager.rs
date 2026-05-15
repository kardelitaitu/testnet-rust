use anyhow::{Context, Result};
use ethers::prelude::*;
// use std::sync::atomic::{AtomicU64, Ordering}; // Unused
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct SimpleNonceManager {
    provider: Arc<Provider<Http>>,
    address: Address,
    current_nonce: Arc<Mutex<Option<U256>>>,
}

impl SimpleNonceManager {
    pub fn new(provider: Arc<Provider<Http>>, address: Address) -> Self {
        Self {
            provider,
            address,
            current_nonce: Arc::new(Mutex::new(None)),
        }
    }

    /// Get the next nonce to use.
    /// If initialized, returns the local counter and increments it.
    /// If not, fetches from pending state.
    pub async fn next(&self) -> Result<U256> {
        let mut nonce_guard = self.current_nonce.lock().await;

        if let Some(nonce) = *nonce_guard {
            let next = nonce + 1;
            *nonce_guard = Some(next);
            Ok(nonce)
        } else {
            // Fetch from chain
            let nonce = self
                .provider
                .get_transaction_count(self.address, Some(BlockNumber::Pending.into()))
                .await
                .context("Failed to fetch initial nonce")?;

            *nonce_guard = Some(nonce + 1);
            Ok(nonce)
        }
    }

    /// Reset the local nonce to the on-chain value (useful on errors)
    pub async fn resync(&self) -> Result<()> {
        let mut nonce_guard = self.current_nonce.lock().await;
        let nonce = self
            .provider
            .get_transaction_count(self.address, Some(BlockNumber::Pending.into()))
            .await
            .context("Failed to resync nonce")?;
        *nonce_guard = Some(nonce);
        Ok(())
    }
}
