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

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::types::H160;

    fn dummy_provider() -> Arc<Provider<Http>> {
        let client = reqwest::Client::new();
        let url = reqwest::Url::parse("http://localhost:8545").unwrap();
        Arc::new(Provider::new(Http::new_with_client(url, client)))
    }

    #[tokio::test]
    async fn test_new_initial_state_none() {
        let provider = dummy_provider();
        let addr: H160 = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let mgr = SimpleNonceManager::new(provider, addr);
        let guard = mgr.current_nonce.lock().await;
        assert!(guard.is_none());
    }

    #[tokio::test]
    async fn test_clone_preserves_state() {
        let provider = dummy_provider();
        let addr: H160 = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let mgr = SimpleNonceManager::new(provider.clone(), addr);
        let cloned = mgr.clone();
        let guard = cloned.current_nonce.lock().await;
        assert!(guard.is_none());
    }

    #[tokio::test]
    async fn test_next_returns_cached_nonce() {
        let provider = dummy_provider();
        let addr: H160 = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let mgr = SimpleNonceManager::new(provider, addr);
        // Pre-populate the cache (simulates having done an initial fetch)
        {
            let mut guard = mgr.current_nonce.lock().await;
            *guard = Some(U256::from(42));
        }
        // next() should return the cached value and increment
        let nonce = mgr.next().await.unwrap();
        assert_eq!(nonce, U256::from(42));
        let nonce2 = mgr.next().await.unwrap();
        assert_eq!(nonce2, U256::from(43));
        let nonce3 = mgr.next().await.unwrap();
        assert_eq!(nonce3, U256::from(44));
    }
}
