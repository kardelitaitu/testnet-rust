use crate::utils::nonce_manager_2d::TempoNonceManager;
use crate::utils::tempo_tx::{TempoCall, TempoTransaction};
use anyhow::{Context, Result};
use ethers::prelude::*;
use std::sync::Arc;

const DEFAULT_GAS_LIMIT: u64 = 500_000;
const DEFAULT_MAX_FEE_PER_GAS: u128 = 150_000_000_000;
const DEFAULT_PRIORITY_FEE: u128 = 1_500_000_000;
const DEFAULT_CHAIN_ID: u64 = 42431;

pub struct TempoTxSender<M: Middleware + 'static> {
    provider: Arc<M>,
    wallet: LocalWallet,
    chain_id: u64,
}

impl<M: Middleware + 'static> TempoTxSender<M> {
    pub fn new(provider: Arc<M>, wallet: LocalWallet) -> Self {
        Self {
            provider,
            wallet,
            chain_id: DEFAULT_CHAIN_ID,
        }
    }

    pub fn with_chain_id(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self
    }

    pub fn wallet_address(&self) -> Address {
        self.wallet.address()
    }

    pub async fn get_gas_price(&self) -> Result<(u128, u128)> {
        let gas_price_u256 = self
            .provider
            .get_gas_price()
            .await
            .context("Failed to get gas price")?;
        let gas_price: u128 = gas_price_u256.as_u128();
        let priority = std::cmp::min(gas_price, DEFAULT_PRIORITY_FEE);
        Ok((gas_price, priority))
    }

    pub fn build_transaction(
        &self,
        calls: Vec<TempoCall>,
        nonce_key: U256,
        nonce: u64,
        fee_token: Option<Address>,
    ) -> TempoTransaction {
        let (gas_price, priority) = futures::executor::block_on(self.get_gas_price())
            .unwrap_or((DEFAULT_MAX_FEE_PER_GAS, DEFAULT_PRIORITY_FEE));

        let max_fee = gas_price.max(DEFAULT_MAX_FEE_PER_GAS);

        TempoTransaction {
            chain_id: self.chain_id,
            max_priority_fee_per_gas: priority,
            max_fee_per_gas: max_fee,
            gas_limit: DEFAULT_GAS_LIMIT,
            calls,
            access_list: Vec::new(),
            nonce_key,
            nonce,
            valid_before: None,
            valid_after: None,
            fee_token,
            tempo_authorization_list: Vec::new(),
            key_authorization: None,
        }
    }

    pub async fn send(&self, tx: &mut TempoTransaction) -> Result<TransactionReceipt> {
        if tx.calls.is_empty() {
            anyhow::bail!("Cannot send empty TempoTransaction");
        }

        let sighash = tx.signature_hash();
        let signature = self
            .wallet
            .sign_hash(sighash)
            .context("Failed to sign transaction")?;

        let raw_tx = tx.rlp_signed(&signature);
        let pending = self
            .provider
            .send_raw_transaction(raw_tx)
            .await
            .context("Failed to send raw transaction")?;

        let receipt = pending
            .await
            .context("Transaction failed")?
            .ok_or_else(|| anyhow::anyhow!("No receipt received"))?;

        Ok(receipt)
    }

    pub async fn send_with_nonce(
        &self,
        calls: Vec<TempoCall>,
        nonce_key: U256,
        nonce: u64,
        fee_token: Option<Address>,
    ) -> Result<TransactionReceipt> {
        let mut tx = self.build_transaction(calls, nonce_key, nonce, fee_token);
        self.send(&mut tx).await
    }

    pub async fn send_parallel(
        &self,
        transactions: Vec<TempoTransaction>,
    ) -> Vec<Result<TransactionReceipt>> {
        let mut handles = Vec::new();

        for tx in transactions {
            let provider = self.provider.clone();
            let wallet = self.wallet.clone();

            let handle = tokio::spawn(async move {
                let sighash = tx.signature_hash();
                let signature = match wallet.sign_hash(sighash) {
                    Ok(sig) => sig,
                    Err(e) => return Err(anyhow::anyhow!("Signing failed: {}", e)),
                };

                let raw_tx = tx.rlp_signed(&signature);
                let pending = match provider.send_raw_transaction(raw_tx).await {
                    Ok(p) => p,
                    Err(e) => return Err(anyhow::anyhow!("Send failed: {}", e)),
                };

                match pending.await {
                    Ok(Some(r)) => Ok(r),
                    Ok(None) => Err(anyhow::anyhow!("No receipt")),
                    Err(e) => Err(anyhow::anyhow!("Confirmation failed: {}", e)),
                }
            });

            handles.push(handle);
        }

        let results = futures::future::join_all(handles).await;
        results
            .into_iter()
            .map(|r| match r {
                Ok(inner) => inner,
                Err(join_err) => Err(anyhow::anyhow!("Task join failed: {}", join_err)),
            })
            .collect()
    }
}

pub struct TempoBatchSender<M: Middleware + 'static> {
    sender: TempoTxSender<M>,
}

impl<M: Middleware + 'static> TempoBatchSender<M> {
    pub fn new(provider: Arc<M>, wallet: LocalWallet) -> Self {
        Self {
            sender: TempoTxSender::new(provider, wallet),
        }
    }

    pub async fn send_batch(
        &self,
        calls: Vec<TempoCall>,
        nonce: u64,
    ) -> Result<TransactionReceipt> {
        self.sender
            .send_with_nonce(calls, U256::zero(), nonce, None)
            .await
    }

    pub async fn send_parallel_batches(
        &self,
        batches: Vec<(Vec<TempoCall>, u64)>,
    ) -> Vec<Result<TransactionReceipt>> {
        let transactions: Vec<TempoTransaction> = batches
            .into_iter()
            .map(|(calls, nonce)| {
                self.sender
                    .build_transaction(calls, U256::zero(), nonce, None)
            })
            .collect();

        self.sender.send_parallel(transactions).await
    }
}

pub struct ParallelNonceSender<M: Middleware + 'static> {
    sender: TempoTxSender<M>,
    nonce_manager: TempoNonceManager<M>,
}

impl<M: Middleware + 'static> ParallelNonceSender<M> {
    pub fn new(provider: Arc<M>, wallet: LocalWallet) -> Self {
        Self {
            sender: TempoTxSender::new(provider.clone(), wallet.clone()),
            nonce_manager: TempoNonceManager::new(provider),
        }
    }

    pub async fn send_parallel_with_keys(
        &self,
        calls_list: Vec<Vec<TempoCall>>,
        nonce_keys: Vec<u64>,
    ) -> Vec<Result<TransactionReceipt>> {
        assert_eq!(calls_list.len(), nonce_keys.len());

        let wallet_addr = self.sender.wallet_address();

        let mut nonce_values = Vec::new();
        for key in &nonce_keys {
            match self.nonce_manager.get_next_nonce(wallet_addr, *key).await {
                Ok(nonce) => nonce_values.push(nonce),
                Err(e) => {
                    return vec![Err(anyhow::anyhow!(
                        "Failed to get nonce for key {}: {}",
                        key,
                        e
                    ))]
                }
            }
        }

        let transactions: Vec<TempoTransaction> = calls_list
            .into_iter()
            .zip(nonce_values.into_iter())
            .map(|(calls, nonce)| {
                self.sender
                    .build_transaction(calls, U256::zero(), nonce, None)
            })
            .collect();

        self.sender.send_parallel(transactions).await
    }

    pub async fn send_parallel_protocol_nonce(
        &self,
        calls_list: Vec<Vec<TempoCall>>,
    ) -> Vec<Result<TransactionReceipt>> {
        let wallet_addr = self.sender.wallet_address();

        let mut nonce_values = Vec::new();
        for _ in &calls_list {
            match self
                .nonce_manager
                .get_next_protocol_nonce(wallet_addr)
                .await
            {
                Ok(nonce) => nonce_values.push(nonce),
                Err(e) => return vec![Err(anyhow::anyhow!("Failed to get protocol nonce: {}", e))],
            }
        }

        let nonce_keys: Vec<u64> = nonce_values.iter().map(|_| 0).collect();
        self.send_parallel_with_keys(calls_list, nonce_keys).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tempo_tx_sender_creation() {
        let provider = Arc::new(Provider::<Http>::new(
            Http::from_str("http://localhost").unwrap(),
        ));
        let wallet = LocalWallet::new(&mut rand::thread_rng());

        let sender = TempoTxSender::new(provider, wallet);
        assert_eq!(sender.chain_id, DEFAULT_CHAIN_ID);
    }

    #[test]
    fn test_build_transaction() {
        let provider = Arc::new(Provider::<Http>::new(
            Http::from_str("http://localhost").unwrap(),
        ));
        let wallet = LocalWallet::new(&mut rand::thread_rng());
        let sender = TempoTxSender::new(provider, wallet);

        let calls = vec![
            TempoCall::new(Address::random(), Bytes::from(vec![0x01])),
            TempoCall::new(Address::random(), Bytes::from(vec![0x02])),
        ];

        let tx = sender.build_transaction(calls, U256::from(1), 5, None);

        assert_eq!(tx.chain_id, DEFAULT_CHAIN_ID);
        assert_eq!(tx.nonce_key, U256::from(1));
        assert_eq!(tx.nonce, 5);
        assert_eq!(tx.len(), 2);
    }

    #[test]
    fn test_parallel_nonce_sender() {
        let provider = Arc::new(Provider::<Http>::new(
            Http::from_str("http://localhost").unwrap(),
        ));
        let wallet = LocalWallet::new(&mut rand::thread_rng());

        let sender = ParallelNonceSender::new(provider, wallet);
        assert!(sender.sender.wallet_address() != Address::zero());
    }
}
