//! Multi-Send Concurrent Task
//!
//! Launches native token transfers concurrently via asynchronous loop.
//!
//! Workflow:
//! 1. Generate random recipients
//! 2. Send transactions concurrently
//! 3. Collect results

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
pub struct MultiSendConcurrentTask;

impl MultiSendConcurrentTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for MultiSendConcurrentTask {
    fn name(&self) -> &'static str {
        "31_multi_send_concurrent"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        // 1. Select random system token
        // 1. Find a system token with balance
        let mut system_tokens = TempoTokens::get_system_tokens();
        use rand::seq::SliceRandom;
        let mut rng = rand::rngs::OsRng;
        system_tokens.shuffle(&mut rng);

        let mut selected_token = None;
        let mut token_balance = U256::ZERO;
        let mut token_decimals = 18;

        for token in system_tokens {
            let bal = TempoTokens::get_token_balance(client, token.address, address).await?;
            if !bal.is_zero() {
                selected_token = Some(token.clone());
                token_balance = bal;
                token_decimals = TempoTokens::get_token_decimals(client, token.address).await?;
                break;
            }
        }

        let (token_info, balance) = match selected_token {
            Some(t) => (t, token_balance),
            None => {
                return Ok(TaskResult {
                    success: false,
                    message: "No system token balance found for concurrent transfer".to_string(),
                    tx_hash: None,
                });
            }
        };
        let decimals = token_decimals;
        let token_addr = token_info.address;

        let count = 2;
        // Amount: 3% of balance total (1.5% per recipient)
        let total_impact = balance * U256::from(3) / U256::from(100);
        let amount_per_recipient = total_impact / U256::from(count);

        if amount_per_recipient.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient {} balance to send 3% (balance: {})",
                    token_info.symbol, balance
                ),
                tx_hash: None,
            });
        }

        tracing::debug!(
            "Executing {} Concurrent {} Transfers (3% total)...",
            count,
            token_info.symbol
        );

        let mut futures = Vec::new();
        let mut recipients = Vec::new();

        let base_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;

        for i in 0..count {
            let recipient = get_random_address()?;
            recipients.push(recipient);

            let transfer_call = IERC20::transferCall {
                recipient,
                amount: amount_per_recipient,
            };

            let tx = TransactionRequest::default()
                .to(token_addr)
                .input(transfer_call.abi_encode().into())
                .from(address)
                .nonce(base_nonce + i as u64)
                .max_fee_per_gas(200_000_000_000u128)
                .max_priority_fee_per_gas(2_000_000_000u128);

            futures.push(client.provider.send_transaction(tx));
        }

        let mut success_count = 0;
        let mut last_hash = String::new();

        for (i, future) in futures.into_iter().enumerate() {
            match future.await {
                Ok(pending) => {
                    let tx_hash = *pending.tx_hash();
                    match pending.get_receipt().await {
                        Ok(receipt) => {
                            if receipt.inner.status() {
                                success_count += 1;
                                last_hash = format!("{:?}", tx_hash);
                                // println!(
                                //     "  [{}] Success: sent {} {} to {:?}",
                                //     i + 1,
                                //     TempoTokens::format_amount(amount_per_recipient, decimals),
                                //     token_info.symbol,
                                //     recipients[i]
                                // );
                            } else {
                                // println!("  [{}] Failed: transaction reverted", i + 1);
                            }
                        }
                        Err(_e) => {
                            // println!("  [{}] Failed to get receipt: {:?}", i + 1, _e);
                        }
                    }
                }
                Err(_e) => {
                    // println!("  [{}] Tx failed: {:?}", i + 1, _e);
                }
            }
        }

        // Update Nonce Manager
        if let Some(manager) = &client.nonce_manager {
            manager.set(address, base_nonce + count as u64).await;
        }

        Ok(TaskResult {
            success: success_count > 0,
            message: format!(
                "Completed {}/{} concurrent {} transfers.",
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
