use crate::task::{Task, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

pub struct SelfTransferTask;

#[async_trait]
impl Task<TaskContext> for SelfTransferTask {
    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_TRANSFER;
        let estimated_gas = gas_limit * gas_price;

        // 1. Balance check
        let balance = provider.get_balance(address, None).await?;
        if balance < estimated_gas {
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

        // 3. Send 0 TXENE to self (fire-and-forget)
        let tx = TransactionRequest::new()
            .to(address)
            .value(0)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .from(address);

        use ethers::middleware::SignerMiddleware;
        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!(
                    "Self-transfer 0 TXENE submitted (tx: {:?})",
                    pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("SelfTransfer tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit self-transfer tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }

    fn name(&self) -> &str {
        "05_selfTransferNative"
    }
}
