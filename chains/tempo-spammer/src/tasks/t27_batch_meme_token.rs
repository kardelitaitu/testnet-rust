//! Batch Meme Token Task
//!
//! Performs multiple meme token transfers in a loop.
//!
//! Workflow:
//! 1.// query contentokens from DB
//! 2. Select random token
//! 3. Perform multiple transfers to random addresses

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask, get_random_address};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::TransactionRequest;
use alloy_sol_types::{SolCall, sol};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use std::str::FromStr;

sol!(
    interface IERC20Mintable {
        function transfer(address recipient, uint256 amount) external returns (bool);
        function mint(address to, uint256 amount) external;
    }
);

#[derive(Debug, Clone, Default)]
pub struct BatchMemeTokenTask;

impl BatchMemeTokenTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for BatchMemeTokenTask {
    fn name(&self) -> &'static str {
        "27_batch_meme_token"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let wallet_addr_str = address.to_string();

        let mut meme_tokens = if let Some(db) = &ctx.db {
            match db.get_assets_by_type(&wallet_addr_str, "meme").await {
                Ok(addresses) => addresses,
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        // If no meme tokens exist, create one first
        if meme_tokens.is_empty() {
            tracing::info!("No meme tokens found for wallet. Creating new meme token first...");

            // Import the create_meme task
            use crate::tasks::t21_create_meme::CreateMemeTask;

            let create_task = CreateMemeTask::new();
            let create_result = create_task.run(ctx).await?;

            if !create_result.success {
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to create meme token: {}", create_result.message),
                    tx_hash: create_result.tx_hash,
                });
            }

            // Re-query the database for the newly created token
            if let Some(db) = &ctx.db {
                match db.get_assets_by_type(&wallet_addr_str, "meme").await {
                    Ok(addresses) if !addresses.is_empty() => {
                        meme_tokens = addresses;
                        tracing::info!(
                            "Successfully created and found {} meme token(s)",
                            meme_tokens.len()
                        );
                    }
                    _ => {
                        return Ok(TaskResult {
                            success: false,
                            message: "Created meme token but could not find it in database"
                                .to_string(),
                            tx_hash: None,
                        });
                    }
                }
            } else {
                return Ok(TaskResult {
                    success: false,
                    message: "Cannot create meme token without database".to_string(),
                    tx_hash: None,
                });
            }
        }

        let mut rng = rand::rngs::OsRng;
        let token_addr_str = meme_tokens[rng.gen_range(0..meme_tokens.len())].clone();
        let token_addr = Address::from_str(&token_addr_str).context("Invalid token address")?;

        let symbol = token_addr_str.get(..8).unwrap_or("MEME").to_string();
        let decimals = TempoTokens::get_token_decimals(client, token_addr).await?;

        let count = 2; // Fixed to 2 recipients
        let mut balance = TempoTokens::get_token_balance(client, token_addr, address).await?;

        // Amount: 2% of balance, divided by count for each transfer
        let mut amount_wei = balance * U256::from(2) / U256::from(100) / U256::from(count);

        // 1. Check if Mint needed (if balance is zero or 2% is zero)
        if balance.is_zero() || amount_wei.is_zero() {
            tracing::debug!("Low balance for {}. Minting more...", symbol);
            let mint_amount = U256::from(1000) * U256::from(10_u64.pow(decimals as u32));
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

            match client.provider.send_transaction(mint_tx.clone()).await {
                Ok(pending) => {
                    let _ = pending.get_receipt().await;
                    balance = TempoTokens::get_token_balance(client, token_addr, address).await?;
                    amount_wei = balance * U256::from(2) / U256::from(100) / U256::from(count);
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nonce too low") || err_str.contains("already known") {
                        tracing::warn!(
                            "Nonce error on meme mint (t27), resetting cache and retrying..."
                        );
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        if let Ok(pending) = client.provider.send_transaction(mint_tx).await {
                            let _ = pending.get_receipt().await;
                            balance = TempoTokens::get_token_balance(client, token_addr, address)
                                .await
                                .unwrap_or(U256::ZERO);
                            amount_wei =
                                balance * U256::from(2) / U256::from(100) / U256::from(count);
                        }
                    } else if err_str.contains("aa4bc69a") {
                        tracing::warn!(
                            "Minting skipped: Likely Sold Out or Already Claimed (0xaa4bc69a)"
                        );
                    } else {
                        tracing::error!("Minting failed: {:?}", e);
                    }
                }
            }
        }

        if amount_wei.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient balance for {} even after mint attempt",
                    symbol
                ),
                tx_hash: None,
            });
        }

        // 2. Transfers
        let mut last_hash = String::new();
        let mut success_count = 0;

        tracing::debug!(
            "Executing Batch of {} {} Transfers (2% of balance)...",
            count,
            symbol
        );

        for i in 0..count {
            let recipient = get_random_address()?;
            let transfer_call = IERC20Mintable::transferCall {
                recipient,
                amount: amount_wei,
            };

            let tx = TransactionRequest::default()
                .to(token_addr)
                .input(transfer_call.abi_encode().into())
                .from(address)
                .max_fee_per_gas(150_000_000_000u128)
                .max_priority_fee_per_gas(1_500_000_000u128);

            match client.provider.send_transaction(tx.clone()).await {
                Ok(pending) => {
                    let tx_hash = *pending.tx_hash();
                    match pending.get_receipt().await {
                        Ok(receipt) => {
                            if receipt.inner.status() {
                                success_count += 1;
                                last_hash = format!("{:?}", tx_hash);
                                tracing::debug!(
                                    "✅ {} Transfer {}/{} success: {:?}",
                                    symbol,
                                    i + 1,
                                    count,
                                    tx_hash
                                );
                                // Wait for nonce propagation
                                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                            } else {
                                tracing::error!(
                                    "❌ {} Transfer {}/{} reverted: {:?}",
                                    symbol,
                                    i + 1,
                                    count,
                                    tx_hash
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!("Error getting receipt for transfer {}: {:?}", i + 1, e)
                        }
                    }
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nonce too low") || err_str.contains("already known") {
                        tracing::warn!(
                            "Nonce error on meme transfer (t27 loop), resetting cache and retrying..."
                        );
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

                        if let Ok(pending) = client.provider.send_transaction(tx).await {
                            let tx_hash = *pending.tx_hash();
                            if let Ok(receipt) = pending.get_receipt().await {
                                if receipt.inner.status() {
                                    success_count += 1;
                                    last_hash = format!("{:?}", tx_hash);
                                }
                            }
                        }
                    } else {
                        tracing::error!("{} Transfer {}/{} failed: {:?}", symbol, i + 1, count, e);
                    }
                }
            }
        }

        Ok(TaskResult {
            success: success_count > 0,
            message: format!(
                "Executed {}/{} meme token transfers for {}.",
                success_count, count, symbol
            ),
            tx_hash: if last_hash.is_empty() {
                None
            } else {
                Some(last_hash)
            },
        })
    }
}
