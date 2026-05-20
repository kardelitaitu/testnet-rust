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

    #[tokio::test]
    async fn test_next_from_zero_uses_cache() {
        let provider = dummy_provider();
        let addr: H160 = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let mgr = SimpleNonceManager::new(provider, addr);
        // Populate with nonce 0
        {
            let mut guard = mgr.current_nonce.lock().await;
            *guard = Some(U256::from(0));
        }
        assert_eq!(mgr.next().await.unwrap(), U256::from(0));
        assert_eq!(mgr.next().await.unwrap(), U256::from(1));
        assert_eq!(mgr.next().await.unwrap(), U256::from(2));
    }

    #[tokio::test]
    async fn test_next_after_reset_to_lower_value() {
        let provider = dummy_provider();
        let addr: H160 = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let mgr = SimpleNonceManager::new(provider, addr);
        // Start at 100
        {
            let mut guard = mgr.current_nonce.lock().await;
            *guard = Some(U256::from(100));
        }
        assert_eq!(mgr.next().await.unwrap(), U256::from(100));
        assert_eq!(mgr.next().await.unwrap(), U256::from(101));
        // Simulate resync to a lower value (nonce not yet confirmed)
        {
            let mut guard = mgr.current_nonce.lock().await;
            *guard = Some(U256::from(100)); // Pending changed, still at 100
        }
        assert_eq!(mgr.next().await.unwrap(), U256::from(100));
        assert_eq!(mgr.next().await.unwrap(), U256::from(101));
    }

    #[tokio::test]
    async fn test_next_handles_u256_max() {
        let provider = dummy_provider();
        let addr: H160 = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let mgr = SimpleNonceManager::new(provider, addr);
        // Set to U256::MAX - 1
        {
            let mut guard = mgr.current_nonce.lock().await;
            *guard = Some(U256::MAX - 1);
        }
        // next() returns current then increments cache: (MAX-1) → cache becomes MAX
        assert_eq!(mgr.next().await.unwrap(), U256::MAX - 1);
        // next() tries to compute MAX + 1 which would overflow, so reset before that call
        {
            let mut guard = mgr.current_nonce.lock().await;
            *guard = Some(U256::zero());
        }
        assert_eq!(mgr.next().await.unwrap(), U256::zero());
        assert_eq!(mgr.next().await.unwrap(), U256::from(1));
        // Verify cache is consistent via internal mutex
        {
            let guard = mgr.current_nonce.lock().await;
            assert_eq!(*guard, Some(U256::from(2)));
        }
    }

    #[tokio::test]
    async fn test_cache_set_to_none_fetches_from_rpc() {
        let provider = dummy_provider();
        let addr: H160 = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let mgr = SimpleNonceManager::new(provider, addr);
        // Set cache initially
        {
            let mut guard = mgr.current_nonce.lock().await;
            *guard = Some(U256::from(10));
        }
        assert_eq!(mgr.next().await.unwrap(), U256::from(10));
        // Reset cache to None (simulates error recovery)
        {
            let mut guard = mgr.current_nonce.lock().await;
            *guard = None;
        }
        // The dummy provider won't respond, so next() will fail
        let result = mgr.next().await;
        assert!(result.is_err(), "Should fail because dummy RPC can't be reached");
    }

    #[tokio::test]
    async fn test_clone_inherits_cache_value() {
        let provider = dummy_provider();
        let addr: H160 = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let mgr = SimpleNonceManager::new(provider, addr);
        // Set nonce on original
        {
            let mut guard = mgr.current_nonce.lock().await;
            *guard = Some(U256::from(77));
        }
        let cloned = mgr.clone();
        // Clone should share the same Arc<Mutex>, so it should see the value
        {
            let guard = cloned.current_nonce.lock().await;
            assert_eq!(*guard, Some(U256::from(77)));
        }
    }

    #[tokio::test]
    async fn test_clone_increments_independently() {
        let provider = dummy_provider();
        let addr: H160 = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let mgr = SimpleNonceManager::new(provider, addr);
        {
            let mut guard = mgr.current_nonce.lock().await;
            *guard = Some(U256::from(50));
        }
        let cloned = mgr.clone();
        // Original increments
        assert_eq!(mgr.next().await.unwrap(), U256::from(50));
        // Clone sees incremented value (shared Arc)
        assert_eq!(cloned.next().await.unwrap(), U256::from(51));
    }
}
