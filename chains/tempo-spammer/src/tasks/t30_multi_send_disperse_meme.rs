//! Multi-Send Disperse Meme Task
//!
//! Sends meme tokens to multiple recipients in a loop.
//! Workflow:
//! 1. Select random meme token from DB
//! 2. Calculate 3% of balance (mint if low)
//! 3. Execute transfers in a loop

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask, get_random_address};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::TransactionRequest;
use alloy_sol_types::{SolCall, SolValue, sol};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::seq::SliceRandom;
use std::str::FromStr;

sol!(
    interface IERC20Mintable {
        function approve(address spender, uint256 amount) external returns (bool);
        function transfer(address recipient, uint256 amount) external returns (bool);
        function mint(address to, uint256 amount) external;
    }
);

#[derive(Debug, Clone, Default)]
pub struct MultiSendDisperseMemeTask;

impl MultiSendDisperseMemeTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for MultiSendDisperseMemeTask {
    fn name(&self) -> &'static str {
        "30_multi_send_disperse_meme"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let wallet_addr_str = address.to_string();

        // 1. Select Meme Token
        let meme_tokens = if let Some(db) = &ctx.db {
            match db.get_assets_by_type(&wallet_addr_str, "meme").await {
                Ok(addresses) => addresses,
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        if meme_tokens.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No meme tokens found in DB.".to_string(),
                tx_hash: None,
            });
        }

        // Filter out system tokens (starting with 0x20c0 or 0x20C0)
        let mut meme_tokens: Vec<String> = meme_tokens
            .into_iter()
            .filter(|addr| !addr.to_lowercase().starts_with("0x20c0"))
            .collect();

        if meme_tokens.is_empty() {
            // Fallback to default, mintable meme token if we filtered everything out
            meme_tokens.push(TempoTokens::FALLBACK_MEME_TOKEN.to_string());
        }

        let mut rng = rand::rngs::OsRng;
        let token_addr_str = meme_tokens.choose(&mut rng).unwrap();
        let token_addr = Address::from_str(token_addr_str)?;

        tracing::debug!("Selected Meme Token: {}", token_addr);

        // 2. Logic
        let decimals = TempoTokens::get_token_decimals(client, token_addr).await?;
        let mut balance = TempoTokens::get_token_balance(client, token_addr, address).await?;

        let percent = U256::from(3);
        let mut total_amount = balance * percent / U256::from(100);

        // Check if Mint needed
        if balance.is_zero() || total_amount.is_zero() {
            let mint_amount = U256::from(1000) * U256::from(10_u64.pow(decimals as u32));
            let mint_call = IERC20Mintable::mintCall {
                to: address,
                amount: mint_amount,
            };

            let tx = TransactionRequest::default()
                .to(token_addr)
                .input(mint_call.abi_encode().into())
                .from(address)
                .max_fee_per_gas(150_000_000_000u128)
                .max_priority_fee_per_gas(1_500_000_000u128);

            // Try to mint, but don't fail if unauthorized (not owner)
            match client.provider.send_transaction(tx).await {
                Ok(pending) => {
                    match pending.get_receipt().await {
                        Ok(_receipt) => {
                            balance =
                                TempoTokens::get_token_balance(client, token_addr, address).await?;
                            total_amount = balance * percent / U256::from(100);
                        }
                        Err(e) => {
                            // Mint failed (likely unauthorized), use existing balance
                            tracing::debug!("Mint failed ({}), using existing balance", e);
                        }
                    }
                }
                Err(e) => {
                    // Unauthorized or other error, skip minting
                    tracing::debug!("Cannot mint token ({}), using existing balance", e);
                }
            }
        }

        if total_amount.is_zero() {
            // Try to use fallback minimum instead (100 tokens)
            total_amount = U256::from(100) * U256::from(10_u64.pow(decimals as u32));

            // If still insufficient, skip gracefully
            if balance < total_amount {
                return Ok(TaskResult {
                    success: false,
                    message: "Insufficient balance for disperse (need 100+ tokens)".to_string(),
                    tx_hash: None,
                });
            }
        }

        if total_amount.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: "Calculated amount is zero".to_string(),
                tx_hash: None,
            });
        }

        let recipient_count = 3;
        let amount_per_recipient = total_amount / U256::from(recipient_count);
        let mut last_tx_hash = None;

        let mut first_error = None;

        for _ in 0..recipient_count {
            if let Ok(recipient) = get_random_address() {
                let transfer_call = IERC20Mintable::transferCall {
                    recipient,
                    amount: amount_per_recipient,
                };

                let tx = TransactionRequest::default()
                    .to(token_addr)
                    .input(transfer_call.abi_encode().into())
                    .from(address)
                    .max_fee_per_gas(150_000_000_000u128)
                    .max_priority_fee_per_gas(1_500_000_000u128);

                match client.provider.send_transaction(tx).await {
                    Ok(pending) => {
                        last_tx_hash = Some(format!("{:?}", *pending.tx_hash()));
                        let _ = pending.get_receipt().await;
                    }
                    Err(e) => {
                        tracing::warn!("Transfer to {} failed: {:?}", recipient, e);
                        if first_error.is_none() {
                            first_error = Some(anyhow::anyhow!(e));
                        }
                    }
                }
            }
        }

        if last_tx_hash.is_none() && first_error.is_some() {
            return Err(first_error.unwrap());
        }

        Ok(TaskResult {
            success: last_tx_hash.is_some(),
            message: format!(
                "Dispersed {} meme token to {} recipients.",
                TempoTokens::format_amount(total_amount, decimals),
                recipient_count
            ),
            tx_hash: last_tx_hash,
        })
    }
}
