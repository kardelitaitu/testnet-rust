use anyhow::{Context, Result};
use ethers::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

const NONCE_PRECOMPILE: &str = "0x4E4F4E4345000000000000000000000000000000";

abigen!(
    INoncePrecompile,
    r#"[
        function nonce(address owner, uint256 key) external view returns (uint64)
        function nonceKeyCount(address owner) external view returns (uint256)
    ]"#
);

abigen!(
    IAccountKeychain,
    r#"[
        function authorizeNonceKey(uint256 key) external
        function isAuthorizedNonceKey(address owner, uint256 key) external view returns (bool)
    ]"#
);

pub struct TempoNonceManager<M: Middleware + 'static> {
    provider: Arc<M>,
    nonce_contract: INoncePrecompile<M>,
    _keychain_contract: IAccountKeychain<M>,
    local_nonces: Mutex<HashMap<(Address, u64), u64>>,
}

impl<M: Middleware + 'static> TempoNonceManager<M> {
    pub fn new(provider: Arc<M>) -> Self {
        let nonce_addr: Address = NONCE_PRECOMPILE.parse().unwrap();
        let keychain_addr: Address = "0x4B41434F554E5400000000000000000000000000"
            .parse()
            .unwrap();

        Self {
            provider: provider.clone(),
            nonce_contract: INoncePrecompile::new(nonce_addr, provider.clone()),
            _keychain_contract: IAccountKeychain::new(keychain_addr, provider.clone()),
            local_nonces: Mutex::new(HashMap::new()),
        }
    }

    pub async fn get_protocol_nonce(&self, address: Address) -> Result<u64> {
        let nonce = self
            .provider
            .get_transaction_count(address, None)
            .await
            .context("Failed to get protocol nonce")?;
        Ok(nonce.as_u64())
    }

    pub async fn get_user_nonce(&self, address: Address, key: u64) -> Result<u64> {
        let nonce = self
            .nonce_contract
            .nonce(address, U256::from(key))
            .call()
            .await
            .context("Failed to get user nonce")?;
        Ok(nonce)
    }

    pub async fn get_next_nonce(&self, address: Address, key: u64) -> Result<u64> {
        let mut local = self.local_nonces.lock().await;
        let nonce_key = (address, key);
        let next = if let Some(cached) = local.get(&nonce_key) {
            *cached
        } else {
            let network_nonce = self.get_user_nonce(address, key).await?;
            local.insert(nonce_key, network_nonce);
            network_nonce
        };
        local.insert(nonce_key, next + 1);
        Ok(next)
    }

    pub async fn get_next_protocol_nonce(&self, address: Address) -> Result<u64> {
        let mut local = self.local_nonces.lock().await;
        let nonce_key = (address, 0u64);
        let next = if let Some(cached) = local.get(&nonce_key) {
            *cached
        } else {
            let network_nonce = self.get_protocol_nonce(address).await?;
            local.insert(nonce_key, network_nonce);
            network_nonce
        };
        local.insert(nonce_key, next + 1);
        Ok(next)
    }

    pub async fn get_nonce_key_count(&self, address: Address) -> Result<u64> {
        let count = self
            .nonce_contract
            .nonce_key_count(address)
            .call()
            .await
            .context("Failed to get nonce key count")?;
        Ok(count.as_u64())
    }

    pub async fn authorize_nonce_key(
        &self,
        wallet: &LocalWallet,
        key: u64,
    ) -> Result<TransactionReceipt> {
        let client = SignerMiddleware::new(self.provider.clone(), wallet.clone());
        let client = Arc::new(client);

        let keychain = IAccountKeychain::new(
            "0x4B41434F554E5400000000000000000000000000"
                .parse::<Address>()
                .unwrap(),
            client,
        );

        let tx = keychain.authorize_nonce_key(U256::from(key));
        let pending = tx.send().await?;
        let receipt = pending.await.context("Failed to authorize nonce key")?;
        receipt.ok_or_else(|| anyhow::anyhow!("Transaction receipt not found"))
    }

    pub fn build_parallel_tx(
        &self,
        wallet: &LocalWallet,
        to: Address,
        data: Bytes,
        nonce: u64,
        chain_id: u64,
    ) -> TransactionRequest {
        TransactionRequest::new()
            .to(to)
            .data(data)
            .nonce(U256::from(nonce))
            .chain_id(chain_id)
            .from(wallet.address())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_nonce_manager_creation() {
        let manager = TempoNonceManager::<Provider<Http>>::new(Arc::new(Provider::new(
            Http::from_str("http://localhost").unwrap(),
        )));
        assert_eq!(manager.local_nonces.lock().await.len(), 0);
    }
}
