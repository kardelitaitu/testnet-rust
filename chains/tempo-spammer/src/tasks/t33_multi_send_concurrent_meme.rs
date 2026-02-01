//! Multi-Send Concurrent Meme Task
//!
//! Launches concurrent transfers of meme tokens.
//!
//! Workflow:
//! 1. Query meme token from DB
//! 2. Ensure sufficient balance (mint if needed)
//! 3. Send concurrent meme token transfers
//! 4. Collect results

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask, get_random_address};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::TransactionRequest;
use alloy_sol_types::{SolCall, sol};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::seq::SliceRandom;
use std::str::FromStr;

sol!(
    interface IERC20Mintable {
        function transfer(address recipient, uint256 amount) external returns (bool);
        function mint(address to, uint256 amount) external;
    }
);

#[derive(Debug, Clone, Default)]
pub struct MultiSendConcurrentMemeTask;

impl MultiSendConcurrentMemeTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for MultiSendConcurrentMemeTask {
    fn name(&self) -> &'static str {
        "33_multi_send_concurrent_meme"
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
                message: "No created meme tokens found in DB for concurrent transfer.".to_string(),
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
        let token_addr =
            Address::from_str(token_addr_str).context("Invalid token address from DB")?;
        let symbol = token_addr_str.get(..8).unwrap_or("MEME").to_string();

        let count = 2; // Fixed to 2 recipients
        let decimals = TempoTokens::get_token_decimals(client, token_addr).await?;
        let mut balance = TempoTokens::get_token_balance(client, token_addr, address).await?;

        // Amount: 3% of balance total (1.5% per recipient)
        let mut total_impact = balance * U256::from(3) / U256::from(100);
        let mut amount_per_recipient = total_impact / U256::from(count);

        // 2. Mint if needed
        if balance.is_zero() || amount_per_recipient.is_zero() {
            // println!("Low balance for {}. Minting more...", symbol);
            let mint_amount = U256::from(2000) * U256::from(10_u64.pow(decimals as u32));
            let mint_call = IERC20Mintable::mintCall {
                to: address,
                amount: mint_amount,
            };

            let mint_tx = TransactionRequest::default()
                .to(token_addr)
                .input(mint_call.abi_encode().into())
                .from(address)
                .max_fee_per_gas(150_000_000_000u128)
                .max_priority_fee_per_gas(1_500_000_000u128);

            match client.provider.send_transaction(mint_tx).await {
                Ok(pending) => {
                    let _ = pending.get_receipt().await;
                    balance = TempoTokens::get_token_balance(client, token_addr, address).await?;
                    total_impact = balance * U256::from(3) / U256::from(100);
                    amount_per_recipient = total_impact / U256::from(count);
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Minting failed for {}: {:?}", symbol, e));
                }
            }
        }

        if amount_per_recipient.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient balance for {} even after mint attempt",
                    symbol
                ),
                tx_hash: None,
            });
        }

        tracing::debug!(
            "Executing {} Concurrent {} Transfers (3% total)...",
            count,
            symbol
        );

        let mut futures = Vec::new();
        let mut recipients = Vec::new();

        let base_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;

        for i in 0..count {
            let recipient = get_random_address()?;
            recipients.push(recipient);

            let transfer_call = IERC20Mintable::transferCall {
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

        let mut first_error = None;

        for (i, future) in futures.into_iter().enumerate() {
            match future.await {
                Ok(pending) => {
                    let tx_hash = *pending.tx_hash();
                    match pending.get_receipt().await {
                        Ok(receipt) => {
                            if receipt.inner.status() {
                                success_count += 1;
                                last_hash = format!("{:?}", tx_hash);
                            } else {
                                tracing::warn!("  [{}] Failed: transaction reverted", i + 1);
                            }
                        }
                        Err(e) => {
                            tracing::warn!("  [{}] Failed to get receipt: {:?}", i + 1, e);
                            if first_error.is_none() {
                                first_error = Some(anyhow::anyhow!(e));
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("  [{}] Tx failed: {:?}", i + 1, e);
                    if first_error.is_none() {
                        first_error = Some(anyhow::anyhow!(e));
                    }
                }
            }
        }

        if success_count == 0 && first_error.is_some() {
            return Err(first_error
                .unwrap()
                .context("All concurrent transfers failed"));
        }

        // Update Nonce Manager
        if let Some(manager) = &client.nonce_manager {
            manager.set(address, base_nonce + count as u64).await;
        }

        Ok(TaskResult {
            success: success_count > 0,
            message: format!(
                "Completed {}/{} concurrent meme transfers.",
                success_count, count
            ),
            tx_hash: if last_hash.is_empty() {
                None
            } else {
                Some(last_hash)
            },
        })
    }
}
