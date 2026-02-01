//! Multi-Send Disperse Task (System Token)
//!
//! Sends system tokens to multiple recipients in a loop.
//! Workflow:
//! 1. Select random system token.
//! 2. Calculate 3% of balance.
//! 3. Execute transfers in a loop.

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask, get_random_address};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::TransactionRequest;
use alloy_sol_types::{SolCall, SolValue, sol};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::str::FromStr;

sol!(
    interface IERC20 {
        function transfer(address recipient, uint256 amount) external returns (bool);
    }
);

#[derive(Debug, Clone, Default)]
pub struct MultiSendDisperseTask;

impl MultiSendDisperseTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for MultiSendDisperseTask {
    fn name(&self) -> &'static str {
        "28_multi_send_disperse"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        // 1. Scan for System Token with Balance
        let mut selected_token = None;
        let mut selected_balance = U256::ZERO;
        let mut selected_decimals = 18;

        let mut tokens = TempoTokens::get_system_tokens();
        use rand::seq::SliceRandom;
        let mut rng = rand::rngs::OsRng;
        tokens.shuffle(&mut rng);

        for token in tokens {
            let balance = TempoTokens::get_token_balance(client, token.address, address).await?;
            if !balance.is_zero() {
                selected_decimals = TempoTokens::get_token_decimals(client, token.address)
                    .await
                    .unwrap_or(18);
                selected_token = Some(token);
                selected_balance = balance;
                break;
            }
        }

        if selected_token.is_none() {
            return Ok(TaskResult {
                success: false,
                message: "No system tokens with balance found".to_string(),
                tx_hash: None,
            });
        }

        let token_info = selected_token.unwrap();
        let token_addr = token_info.address;
        let balance = selected_balance;
        let decimals = selected_decimals;

        tracing::debug!(
            "Selected System Token: {} ({}) Bal: {}",
            token_info.symbol,
            token_addr,
            balance
        );

        let percent = U256::from(3); // 3%
        let total_amount = balance * percent / U256::from(100);
        let recipient_count = 3;
        let amount_per_recipient = total_amount / U256::from(recipient_count);

        // println!(
        //     "Balance: {}, Dispersing 3% ({}) to {} recipients ({} each)...",
        //     TempoTokens::format_amount(balance, decimals),
        //     TempoTokens::format_amount(total_amount, decimals),
        //     recipient_count,
        //     TempoTokens::format_amount(amount_per_recipient, decimals)
        // );

        if total_amount.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: "Calculated amount is zero".to_string(),
                tx_hash: None,
            });
        }

        let mut last_tx_hash = None;

        for _ in 0..recipient_count {
            if let Ok(recipient) = get_random_address() {
                let transfer_call = IERC20::transferCall {
                    recipient,
                    amount: amount_per_recipient,
                };

                let tx = TransactionRequest::default()
                    .to(token_addr)
                    .input(transfer_call.abi_encode().into())
                    .from(address)
                    .max_fee_per_gas(150_000_000_000u128)
                    .max_priority_fee_per_gas(1_500_000_000u128);

                match client.provider.send_transaction(tx.clone()).await {
                    Ok(pending) => {
                        last_tx_hash = Some(format!("{:?}", *pending.tx_hash()));
                        let _ = pending.get_receipt().await;
                    }
                    Err(e) => {
                        let err_str = e.to_string().to_lowercase();
                        if err_str.contains("nonce too low") || err_str.contains("already known") {
                            tracing::warn!(
                                "Nonce error on multi_send (t28), resetting cache and retrying..."
                            );
                            client.reset_nonce_cache().await;
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            if let Ok(pending) = client.provider.send_transaction(tx).await {
                                last_tx_hash = Some(format!("{:?}", *pending.tx_hash()));
                                let _ = pending.get_receipt().await;
                            }
                        } else {
                            // println!("Transfer to {} failed: {:?}", recipient, e);
                        }
                    }
                }
            }
        }

        Ok(TaskResult {
            success: last_tx_hash.is_some(),
            message: format!(
                "Dispersed {} of {} to {} recipients.",
                TempoTokens::format_amount(total_amount, decimals),
                token_info.symbol,
                recipient_count
            ),
            tx_hash: last_tx_hash,
        })
    }
}
