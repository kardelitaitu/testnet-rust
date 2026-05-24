use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct SimpleEthTransferTask;

impl SimpleEthTransferTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for SimpleEthTransferTask {
    fn name(&self) -> &str {
        "02_simpleNativeTransfer"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = U256::from(21_000u64);
        let estimated_gas = gas_limit * gas_price;

        // 1. Balance check
        let balance = provider.get_balance(address, None).await?;
        if balance <= estimated_gas {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient TXENE for gas: need {} Wei, have {} Wei",
                    estimated_gas, balance
                ),
                tx_hash: None,
            });
        }

        // 2. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );
        let nonce = nonce_manager.next().await?;

        // 3. Transfer 1.00%-2.00% of balance (fire-and-forget)
        let recipient = AddressCache::get_random().context("Failed to get random address")?;

        let mut rng = OsRng;
        let pct_basis_points: u64 = rng.gen_range(100u64..=200u64);
        let available = balance - estimated_gas;
        let amount_wei = available * U256::from(pct_basis_points) / U256::from(10_000u64);

        let tx = TransactionRequest::new()
            .to(recipient)
            .value(amount_wei)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .from(address);

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => {
                let amount_native = ethers::utils::format_units(amount_wei, "ether")
                    .unwrap_or_else(|_| amount_wei.to_string());
                let amount_pct = pct_basis_points as f64 / 100.0;
                Ok(TaskResult {
                    success: true,
                    message: format!(
                        "Sent {} TXENE ({:.2}%) to {:?} (tx: {:?})",
                        amount_native,
                        amount_pct,
                        recipient,
                        pending.tx_hash()
                    ),
                    tx_hash: Some(format!("{:?}", pending.tx_hash())),
                })
            }
            Err(e) => {
                debug!("SimpleEthTransfer tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit native transfer tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
