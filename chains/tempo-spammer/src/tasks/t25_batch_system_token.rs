//! Batch System Token Task
//!
//! Executes batch transfers of system tokens (PathUSD, AlphaUSD, etc.) to multiple recipients.
//!
//! Workflow:
//! 1. Select random system token
//! 2. Generate 2-5 random addresses
//! 3. Send token amounts to each

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask, get_random_address};
use alloy::primitives::U256;
use alloy::rpc::types::TransactionRequest;
use alloy_sol_types::{SolCall, sol};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;

sol!(
    interface IERC20 {
        function transfer(address recipient, uint256 amount) external returns (bool);
    }
);

#[derive(Debug, Clone, Default)]
pub struct BatchSystemTokenTask;

impl BatchSystemTokenTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for BatchSystemTokenTask {
    fn name(&self) -> &'static str {
        "25_batch_system_token"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        // Select random system token
        let token_info = TempoTokens::get_random_system_token();
        let token_addr = token_info.address;

        let count = 2; // Fixed to 2 recipients
        let decimals = TempoTokens::get_token_decimals(client, token_addr).await?;
        let balance = TempoTokens::get_token_balance(client, token_addr, address).await?;

        // Amount: 2% of balance
        let amount_per_recipient = balance * U256::from(2) / U256::from(100) / U256::from(count);

        if amount_per_recipient.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient {} balance to send 2% (balance: {})",
                    token_info.symbol, balance
                ),
                tx_hash: None,
            });
        }

        tracing::debug!(
            "Executing Batch of {} {} Transfers (2% of balance)...",
            count,
            token_info.symbol
        );

        let mut last_hash = String::new();
        let mut success_count = 0;

        for i in 0..count {
            let recipient = get_random_address()?;
            let transfer_call = IERC20::transferCall {
                recipient,
                amount: amount_per_recipient,
            };

            // Get robust nonce reservation for each transfer in the batch
            let reservation = match client.get_robust_nonce(&ctx.config.rpc_url).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(
                        "Failed to reserve nonce for batch transfer {}/{}: {}",
                        i + 1,
                        count,
                        e
                    );
                    continue;
                }
            };

            let tx = TransactionRequest::default()
                .to(token_addr)
                .input(transfer_call.abi_encode().into())
                .from(address)
                .nonce(reservation.nonce)
                .max_fee_per_gas(150_000_000_000u128)
                .max_priority_fee_per_gas(1_500_000_000u128);

            match client.provider.send_transaction(tx.clone()).await {
                Ok(pending) => {
                    reservation.mark_submitted().await;
                    let tx_hash = *pending.tx_hash();
                    match pending.get_receipt().await {
                        Ok(receipt) => {
                            if receipt.inner.status() {
                                success_count += 1;
                                last_hash = format!("{:?}", tx_hash);
                            }
                        }
                        Err(_e) => {}
                    }
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nonce too low") || err_str.contains("already known") {
                        tracing::warn!(
                            "Nonce error on batch system transfer, recovering and retrying..."
                        );
                        // Release the failed nonce
                        drop(reservation);
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

                        // Get new nonce for retry
                        let retry_reservation =
                            match client.get_robust_nonce(&ctx.config.rpc_url).await {
                                Ok(r) => r,
                                Err(_) => continue,
                            };

                        let retry_tx = TransactionRequest::default()
                            .to(token_addr)
                            .input(transfer_call.abi_encode().into())
                            .from(address)
                            .nonce(retry_reservation.nonce)
                            .max_fee_per_gas(150_000_000_000u128)
                            .max_priority_fee_per_gas(1_500_000_000u128);

                        if let Ok(pending) = client.provider.send_transaction(retry_tx).await {
                            retry_reservation.mark_submitted().await;
                            let tx_hash = *pending.tx_hash();
                            if let Ok(receipt) = pending.get_receipt().await {
                                if receipt.inner.status() {
                                    success_count += 1;
                                    last_hash = format!("{:?}", tx_hash);
                                }
                            }
                        } else {
                            drop(retry_reservation);
                        }
                    } else {
                        drop(reservation);
                    }
                }
            }
        }

        Ok(TaskResult {
            success: success_count > 0,
            message: format!(
                "Executed {}/{} {} transfers in batch.",
                success_count, count, token_info.symbol
            ),
            tx_hash: if last_hash.is_empty() {
                None
            } else {
                Some(last_hash)
            },
        })
    }
}
