use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct BatchTransferTask;

impl BatchTransferTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for BatchTransferTask {
    fn name(&self) -> &str {
        "11_batchNativeSend"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // Get random recipients from address cache
        let mut rng = OsRng;
        let num_transfers: usize = rng.gen_range(5..=20);
        let recipients = AddressCache::get_random_many(num_transfers)?;

        // Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_TRANSFER;
        let gas_cost_per_tx = gas_limit * gas_price;

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());

        let mut tx_hashes = Vec::new();
        let mut success_count = 0;
        let mut total_sent = U256::zero();

        for (i, recipient) in recipients.iter().enumerate() {
            // Check remaining balance before each transfer
            let remaining_balance = provider.get_balance(address, None).await?;
            if remaining_balance <= gas_cost_per_tx {
                break; // Not enough for another transfer
            }

            // Calculate amount from REMAINING balance (1%-2%)
            let pct_basis: u64 = rng.gen_range(100..=200);
            let available = remaining_balance - gas_cost_per_tx;
            let amount = available * U256::from(pct_basis) / U256::from(10_000u64);

            if amount.is_zero() {
                break;
            }

            let nonce = nonce_manager.next().await?;

            let tx = TransactionRequest::new()
                .to(*recipient)
                .value(amount)
                .gas(gas_limit)
                .gas_price(gas_price)
                .nonce(nonce)
                .from(address);

            let pending_tx = client.send_transaction(tx, None).await;

            match pending_tx {
                Ok(pending) => {
                    let tx_hash = format!("{:?}", pending.tx_hash());
                    tx_hashes.push(tx_hash.clone());
                    total_sent += amount;
                    debug!(
                        "Transfer {}/{} sent: {} to {:?} (tx: {})",
                        i + 1,
                        num_transfers,
                        amount,
                        recipient,
                        tx_hash
                    );
                    success_count += 1;
                }
                Err(e) => {
                    debug!("Transfer {}/{} failed: {}", i + 1, num_transfers, e);
                    tx_hashes.push("failed".to_string());
                    let _ = nonce_manager.resync().await;
                }
            }
        }

        if success_count == 0 {
            return Ok(TaskResult {
                success: false,
                message: "No transfers submitted — insufficient balance".to_string(),
                tx_hash: None,
            });
        }

        let total_sent_eth = ethers::utils::format_units(total_sent, "ether")
            .unwrap_or_else(|_| total_sent.to_string());

        Ok(TaskResult {
            success: true,
            message: format!(
                "Batch sent {} TXENE across {} transfers to {} recipients",
                total_sent_eth, success_count, num_transfers
            ),
            tx_hash: Some(tx_hashes.join(",")),
        })
    }
}
