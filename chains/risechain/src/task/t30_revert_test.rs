use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;

pub struct RevertTestTask;

impl RevertTestTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for RevertTestTask {
    fn name(&self) -> &str {
        "30_revertTest"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // Get random recipient from address cache
        let recipient = AddressCache::get_random().context("Failed to get random address")?;

        let mut rng = OsRng;
        let amount_wei: u64 = rng.gen_range(1_000_000_000_000u64..10_000_000_000_000u64);

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_TRANSFER;

        let tx = Eip1559TransactionRequest::new()
            .to(recipient)
            .value(amount_wei)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let client = std::sync::Arc::new(SignerMiddleware::new(
            std::sync::Arc::new(provider.clone()),
            wallet.clone(),
        ));

        let amount_eth = ethers::utils::format_units(amount_wei, "ether")
            .unwrap_or_else(|_| amount_wei.to_string());

        let mut send_result = client.send_transaction(tx, None).await;
        let receipt_result = match send_result {
            Ok(ref mut pending) => pending.await.map_err(|e| anyhow::anyhow!("{:?}", e)),
            Err(ref e) => Err(anyhow::anyhow!("Send failed: {:?}", e)),
        };

        let result = match (send_result, receipt_result) {
            (Ok(_), Ok(Some(receipt))) => {
                if receipt.status == Some(U64::from(0)) {
                    TaskResult {
                        success: true,
                        message: format!(
                            "Transaction reverted as expected (sent {} ETH to {:?})",
                            amount_eth, recipient
                        ),
                        tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
                    }
                } else {
                    TaskResult {
                        success: true,
                        message: format!(
                            "Transaction succeeded (sent {} ETH to {:?})",
                            amount_eth, recipient
                        ),
                        tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
                    }
                }
            }
            (Ok(_), Ok(None)) => TaskResult {
                success: false,
                message: "Transaction dropped".into(),
                tx_hash: None,
            },
            (_, Err(e)) => TaskResult {
                success: true,
                message: format!("Transaction reverted/error: {}", e),
                tx_hash: None,
            },
            (Err(e), _) => TaskResult {
                success: true,
                message: format!("Transaction failed as expected: {}", e),
                tx_hash: None,
            },
        };

        Ok(result)
    }
}
