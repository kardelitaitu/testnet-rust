use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use rand::Rng;
use std::fs;
use std::sync::Arc;
use tracing::debug;

pub struct BatchTransferTask;

impl BatchTransferTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for BatchTransferTask {
    fn name(&self) -> &str {
        "11_batchTransfer"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let recipients = fs::read_to_string("address.txt").context("Failed to read address.txt")?;
        let recipient_list: Vec<&str> = recipients
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();

        let num_transfers = 5;
        let mut rng = OsRng;
        let amount_wei: u64 = rng.gen_range(10_000_000_000_000u64..100_000_000_000_000u64);
        let amount_eth = ethers::utils::format_units(amount_wei, "ether")
            .unwrap_or_else(|_| amount_wei.to_string());

        let mut tx_hashes = Vec::new();
        let mut success_count = 0;

        // Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_TRANSFER;

        for i in 0..num_transfers {
            let recipient_str = recipient_list
                .choose(&mut OsRng)
                .context("address.txt is empty")?;

            let recipient: Address = recipient_str
                .trim()
                .parse()
                .context(format!("Invalid address in address.txt: {}", recipient_str))?;

            let nonce = nonce_manager.next().await?;

            let tx = Eip1559TransactionRequest::new()
                .to(recipient)
                .value(amount_wei)
                .gas(gas_limit)
                .max_fee_per_gas(max_fee)
                .max_priority_fee_per_gas(priority_fee)
                .nonce(nonce)
                .from(address);

            use ethers::middleware::SignerMiddleware;
            let client = SignerMiddleware::new(provider.clone(), wallet.clone());
            let pending_tx = client.send_transaction(tx, None).await;

            match pending_tx {
                Ok(pending) => {
                    let tx_hash = format!("{:?}", pending.tx_hash());
                    tx_hashes.push(tx_hash.clone());
                    debug!(
                        "🚀 Transfer {}/{} sent: {} (Nonce: {})",
                        i + 1,
                        num_transfers,
                        tx_hash,
                        nonce
                    );
                    success_count += 1;
                    // We DO NOT await the receipt here to keep it fast
                }
                Err(e) => {
                    debug!("❌ Transfer {}/{} failed: {}", i + 1, num_transfers, e);
                    tx_hashes.push("failed".to_string());
                    // If failed, we should probably resync nonce, but for now we just continue
                    let _ = nonce_manager.resync().await;
                }
            }
        }

        Ok(TaskResult {
            success: success_count > 0,
            message: format!(
                "Batch sent {} ETH to {} recipients ({} submitted)",
                amount_eth, num_transfers, success_count
            ),
            tx_hash: Some(tx_hashes.join(",")),
        })
    }
}
