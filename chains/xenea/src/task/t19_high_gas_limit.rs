use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct HighGasLimitTask;

impl HighGasLimitTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for HighGasLimitTask {
    fn name(&self) -> &str {
        "19_highGasLimit"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // Get random recipient from address cache
        let recipient = AddressCache::get_random().context("Failed to get random address")?;

        let balance = provider.get_balance(address, None).await?;

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = 1_000_000u64;
        let estimated_gas = U256::from(gas_limit) * gas_price;

        // 1. Balance check
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

        // 2. Send 1% of balance (after reserving gas)
        let available = balance - estimated_gas;
        let amount_wei = available / U256::from(100u64);

        // 3. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );
        let nonce = nonce_manager.next().await?;

        let tx = TransactionRequest::new()
            .to(recipient)
            .value(amount_wei)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .from(address);

        // 4. Send (fire-and-forget)
        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => {
                let amount_eth = ethers::utils::format_units(amount_wei, "ether")
                    .unwrap_or_else(|_| amount_wei.to_string());
                Ok(TaskResult {
                    success: true,
                    message: format!(
                        "High gas limit (1M): sent {} TXENE to {:?}",
                        amount_eth, recipient
                    ),
                    tx_hash: Some(format!("{:?}", pending.tx_hash())),
                })
            }
            Err(e) => {
                debug!("HighGasLimit tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit high gas limit tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
