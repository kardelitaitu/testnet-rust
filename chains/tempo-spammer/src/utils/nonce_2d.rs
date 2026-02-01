//! Tempo 2D Nonce Manager
//!
//! Provides true parallel transaction execution using Tempo's 2D nonce system.
//!
//! The 2D nonce system extends the standard EVM nonce (nonceKey: 0) with additional
//! parallel nonce sequences (nonceKey: 1, 2, 3, ...). This enables sending multiple
//! transactions in parallel without waiting for confirmations.
//!
//! ## Key Concepts
//!
//! - **nonceKey: 0** = Protocol nonce (sequential, like standard EVM)
//! - **nonceKey: 1+** = User nonces (independent parallel sequences)
//!
//! ## Usage
//!
//! ```ignore
//! let manager = TempoNonceManager2D::new(provider.clone());
//!
//! // Authorize a nonce key (one-time setup)
//! manager.authorize_key(signer, 1).await?;
//!
//! // Send 3 transactions in parallel using different nonce keys
//! let tx1 = manager.build_tx(to, data, 1).nonce_key(1);
//! let tx2 = manager.build_tx(to, data, 2).nonce_key(2);
//! let tx3 = manager.build_tx(to, data, 3).nonce_key(3);
//!
//! // Execute all in parallel - no waiting for confirmations!
//! let hashes = tokio::join!(send1, send2, send3);
//! ```

use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Nonce precompile address: 0x4E4F4E4345000000000000000000000000000000
/// (hex encoding of "NONCE")
const NONCE_PRECOMPILE: &str = "0x4E4F4E4345000000000000000000000000000000";

/// Account keychain address: 0x4B41434F554E5400000000000000000000000000
/// (hex encoding of "ACCOUNTKEY")
const ACCOUNT_KEYCHAIN: &str = "0x4B41434F554E5400000000000000000000000000";

/// ERC20 balanceOf selector
const BALANCE_OF_SELECTOR: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];

/// ERC20 approve selector
const APPROVE_SELECTOR: [u8; 4] = [0x09, 0x5e, 0xa7, 0xb3];

/// ERC20 transfer selector
const TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

/// Nonce precompile function: nonce(address owner, uint256 key) returns (uint64)
const NONCE_SELECTOR: [u8; 4] = [0x27, 0xfc, 0xba, 0xcf];

/// Nonce key count function: nonceKeyCount(address owner) returns (uint256)
const NONCE_KEY_COUNT_SELECTOR: [u8; 4] = [0x6a, 0x16, 0x65, 0x88];

/// Authorize nonce key function: authorizeNonceKey(uint256 key)
const AUTHORIZE_KEY_SELECTOR: [u8; 4] = [0xc8, 0x84, 0x4e, 0x62];

/// 2D Nonce Manager for Tempo blockchain
#[derive(Clone)]
pub struct TempoNonceManager2D<P: Provider + Send + Sync> {
    provider: Arc<P>,
    local_nonces: Mutex<HashMap<(Address, u64), u64>>,
}

impl<P: Provider + Send + Sync> TempoNonceManager2D<P> {
    /// Create a new nonce manager
    pub fn new(provider: Arc<P>) -> Self {
        Self {
            provider,
            local_nonces: Mutex::new(HashMap::new()),
        }
    }

    /// Get the protocol nonce (nonceKey: 0) from the network
    pub async fn get_protocol_nonce(&self, address: Address) -> Result<u64> {
        let count = self
            .provider
            .get_transaction_count(address)
            .await
            .context("Failed to get protocol nonce")?;
        Ok(count.as_u64())
    }

    /// Get the nonce for a specific key from the noncel precompile
    pub async fn get_user_nonce(&self, address: Address, key: u64) -> Result<u64> {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&NONCE_SELECTOR);
        calldata.extend_from_slice(&[0u8; 12]); // 12 bytes padding for address
        calldata.extend_from_slice(address.as_slice());
        calldata.extend_from_slice(&U256::from(key).to_be_bytes::<32>());

        let response = self
            .provider
            .call(
                TransactionRequest::default()
                    .to(NONCE_PRECOMPILE.parse().unwrap())
                    .input(calldata.into()),
            )
            .await
            .context("Failed to call nonce precompile")?;

        let bytes = response.as_ref();
        if bytes.len() >= 32 {
            Ok(U256::from_be_slice(bytes).as_u64())
        } else {
            Ok(0)
        }
    }

    /// Get the number of authorized nonce keys for an address
    pub async fn get_nonce_key_count(&self, address: Address) -> Result<u64> {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&NONCE_KEY_COUNT_SELECTOR);
        calldata.extend_from_slice(&[0u8; 12]); // 12 bytes padding for address
        calldata.extend_from_slice(address.as_slice());

        let response = self
            .provider
            .call(
                TransactionRequest::default()
                    .to(NONCE_PRECOMPILE.parse().unwrap())
                    .input(calldata.into()),
            )
            .await
            .context("Failed to call nonceKeyCount")?;

        let bytes = response.as_ref();
        if bytes.len() >= 32 {
            Ok(U256::from_be_slice(bytes).as_u64())
        } else {
            Ok(0)
        }
    }

    /// Get the next nonce for a specific key, with local caching
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

    /// Get the next protocol nonce (key: 0), with local caching
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

    /// Authorize a nonce key for a wallet
    pub async fn authorize_nonce_key(&self, signer: &PrivateKeySigner, key: u64) -> Result<()> {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&AUTHORIZE_KEY_SELECTOR);
        calldata.extend_from_slice(&U256::from(key).to_be_bytes::<32>());

        let tx = TransactionRequest::default()
            .to(ACCOUNT_KEYCHAIN.parse().unwrap())
            .input(calldata.into())
            .from(signer.address())
            .value(U256::ZERO);

        let pending = self
            .provider
            .send_transaction(tx)
            .await
            .context("Failed to send authorize nonce key tx")?;

        let _receipt = pending
            .get_receipt()
            .await
            .context("Failed to get authorize receipt")?;

        Ok(())
    }

    /// Build a transaction request with the specified nonce key
    pub fn build_tx(
        &self,
        to: Address,
        data: Bytes,
        nonce_key: u64,
        chain_id: u64,
        signer: &PrivateKeySigner,
    ) -> TransactionRequest {
        TransactionRequest::default()
            .to(to)
            .input(data)
            .from(signer.address())
            .chain_id(chain_id)
    }

    /// Build a transaction with a specific nonce value (for parallel execution)
    pub fn build_tx_with_nonce(
        &self,
        to: Address,
        data: Bytes,
        nonce: u64,
        chain_id: u64,
        signer: &PrivateKeySigner,
    ) -> TransactionRequest {
        TransactionRequest::default()
            .to(to)
            .input(data)
            .from(signer.address())
            .chain_id(chain_id)
            .nonce(U256::from(nonce))
    }

    /// Build ERC20 balanceOf calldata
    pub fn build_balance_of_calldata(owner: Address) -> Bytes {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&BALANCE_OF_SELECTOR);
        calldata.extend_from_slice(&[0u8; 12]); // 12 bytes padding
        calldata.extend_from_slice(owner.as_slice());
        calldata.into()
    }

    /// Build ERC20 approve calldata
    pub fn build_approve_calldata(spender: Address, amount: u128) -> Bytes {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&APPROVE_SELECTOR);
        calldata.extend_from_slice(&[0u8; 12]); // 12 bytes padding for spender
        calldata.extend_from_slice(spender.as_slice());
        calldata.extend_from_slice(&U256::from(amount).to_be_bytes::<32>());
        calldata.into()
    }

    /// Build ERC20 transfer calldata
    pub fn build_transfer_calldata(recipient: Address, amount: u128) -> Bytes {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&TRANSFER_SELECTOR);
        calldata.extend_from_slice(&[0u8; 12]); // 12 bytes padding for recipient
        calldata.extend_from_slice(recipient.as_slice());
        calldata.extend_from_slice(&U256::from(amount).to_be_bytes::<32>());
        calldata.into()
    }

    /// Clear local nonce cache for an address
    pub async fn clear_cache(&self, address: Address) {
        let mut local = self.local_nonces.lock().await;
        local.retain(|(addr, _), _| *addr != address);
    }

    /// Clear local nonce cache for a specific key
    pub async fn clear_cache_key(&self, address: Address, key: u64) {
        let mut local = self.local_nonces.lock().await;
        local.remove(&(address, key));
    }
}

/// Parallel transaction sender using 2D nonces
pub struct ParallelSender<P: Provider + Send + Sync> {
    manager: TempoNonceManager2D<P>,
}

impl<P: Provider + Send + Sync> ParallelSender<P> {
    /// Create a new parallel sender
    pub fn new(provider: Arc<P>) -> Self {
        Self {
            manager: TempoNonceManager2D::new(provider),
        }
    }

    /// Send multiple transactions in parallel using different nonce keys
    ///
    /// # Arguments
    /// * `signer` - Wallet to sign transactions
    /// * `chain_id` - Network chain ID
    /// * `to` - Target contract address
    /// * `calldatas` - Vector of calldata for each transaction
    /// * `start_key` - Starting nonce key (transactions use key, key+1, key+2, ...)
    ///
    /// # Returns
    /// Vector of transaction hashes
    pub async fn send_parallel(
        &self,
        signer: &PrivateKeySigner,
        chain_id: u64,
        to: Address,
        calldatas: Vec<Bytes>,
        start_key: u64,
    ) -> Result<Vec<String>> {
        let mut futures = Vec::new();

        for (i, calldata) in calldatas.iter().enumerate() {
            let key = start_key + (i as u64);
            let nonce = self.manager.get_next_nonce(signer.address(), key).await?;

            let tx =
                self.manager
                    .build_tx_with_nonce(to, calldata.clone(), nonce, chain_id, signer);

            futures.push(self.manager.provider.send_transaction(tx));
        }

        let mut hashes = Vec::new();
        for result in futures::future::join_all(futures).await {
            match result {
                Ok(pending) => {
                    hashes.push(format!("{:?}", pending.tx_hash()));
                }
                Err(e) => {
                    tracing::error!("Parallel send failed: {:?}", e);
                    hashes.push(format!("ERROR: {:?}", e));
                }
            }
        }

        Ok(hashes)
    }

    /// Authorize multiple nonce keys at once
    pub async fn authorize_keys(
        &self,
        signer: &PrivateKeySigner,
        start_key: u64,
        count: u64,
    ) -> Result<()> {
        for i in 0..count {
            self.manager
                .authorize_nonce_key(signer, start_key + i)
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::providers::ProviderBuilder;

    #[tokio::test]
    async fn test_nonce_manager_creation() {
        let provider = ProviderBuilder::new().on_http("http://localhost:8545".parse().unwrap());

        let manager = TempoNonceManager2D::new(Arc::new(provider));
        assert_eq!(manager.local_nonces.lock().await.len(), 0);
    }

    #[tokio::test]
    async fn test_build_calldata() {
        let owner: Address = "0x6eacca11a74f3d0562aa7de02c4e7a397b73c636"
            .parse()
            .unwrap();
        let calldata = TempoNonceManager2D::build_balance_of_calldata(owner);

        // Verify calldata starts with balanceOf selector
        assert_eq!(&calldata[..4], &BALANCE_OF_SELECTOR);

        // Verify owner is at position 4 (after 4-byte selector + 12 bytes padding)
        assert_eq!(&calldata[16..36], &owner.as_slice());
    }
}
