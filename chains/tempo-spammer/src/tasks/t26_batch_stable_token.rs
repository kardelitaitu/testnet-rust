//! Batch Stable Token Task
//!
//! Executes batch transfers of created stable tokens to multiple recipients.
//!
//! Workflow:
//! 1. Select random stable token from DB
//! 2. Perform multiple transfers to random addresses
//! 3. Mint more if balance is low using Optimistic Pipelining

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask, get_random_address};
use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, U256};
use alloy::rpc::types::TransactionRequest;
use alloy_sol_types::{SolCall, sol};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use rand::seq::SliceRandom;
use std::str::FromStr;

sol!(
    interface IERC20Mintable {
        function transfer(address recipient, uint256 amount) external returns (bool);
        function mint(address to, uint256 amount) external;
    }
);

#[derive(Debug, Clone, Default)]
pub struct BatchStableTokenTask;

impl BatchStableTokenTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for BatchStableTokenTask {
    fn name(&self) -> &'static str {
        "26_batch_stable_token"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        // 1. Select Stable Token
        let stable_tokens = if let Some(db) = &ctx.db {
            db.get_assets_by_type(&address.to_string(), "stablecoin")
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut rng = rand::rngs::OsRng;
        tracing::debug!("🚀 Category 6: Optimistic Pipelining (Stable Batch)");

        // Pick from DB or fallback
        let (token_addr, symbol) = if !stable_tokens.is_empty() {
            let addr_str = stable_tokens.choose(&mut rng).unwrap().clone();
            let addr = Address::from_str(&addr_str).context("Invalid token address from DB")?;
            let system_tokens = TempoTokens::get_system_tokens();
            let sym = system_tokens
                .iter()
                .find(|t| t.address == addr)
                .map(|t| t.symbol.clone())
                .unwrap_or_else(|| format!("Asset_{}", &addr_str[2..8]));
            (addr, sym)
        } else {
            let token_info = TempoTokens::get_random_system_token();
            (token_info.address, token_info.symbol)
        };

        // 2. Settings & Balance Check
        let count = 2; // Kept at 2 as requested
        let decimals = TempoTokens::get_token_decimals(client, token_addr)
            .await
            .unwrap_or(18);
        let balance = TempoTokens::get_token_balance(client, token_addr, address)
            .await
            .unwrap_or(U256::ZERO);

        let amount_per_recipient =
            U256::from(rng.gen_range(100..500)) * U256::from(10_u64.pow(decimals as u32));
        let total_needed = amount_per_recipient * U256::from(count);

        // 3. Prepare Pipeline
        let mut current_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let start_nonce = current_nonce;
        let mut burst_txs = Vec::new();

        // Add Mint if needed
        if balance < total_needed {
            tracing::debug!(
                "Wallet needs {} {}. Adding Mint to pipeline...",
                symbol,
                total_needed
            );
            let mint_call = IERC20Mintable::mintCall {
                to: address,
                amount: total_needed * U256::from(10),
            };
            let mint_tx = TransactionRequest::default()
                .to(token_addr)
                .input(mint_call.abi_encode().into())
                .from(address)
                .nonce(current_nonce)
                .gas_limit(150_000);
            burst_txs.push(mint_tx);
            current_nonce += 1;
        }

        // Add Transfers
        for _ in 0..count {
            let recipient = get_random_address()?;
            let transfer_call = IERC20Mintable::transferCall {
                recipient,
                amount: amount_per_recipient,
            };
            let tx = TransactionRequest::default()
                .to(token_addr)
                .input(transfer_call.abi_encode().into())
                .from(address)
                .nonce(current_nonce)
                .gas_limit(100_000);

            burst_txs.push(tx);
            current_nonce += 1;
        }

        // 4. Burst Submit with Retry Logic for Nonce Errors
        let tx_count = burst_txs.len();
        tracing::info!(
            "Blasting {} Transactions (Nonces {}..{})",
            tx_count,
            start_nonce,
            current_nonce - 1
        );

        let mut last_submitted_nonce = start_nonce.wrapping_sub(1);
        let mut last_hash = String::new();

        for (idx, tx) in burst_txs.iter().enumerate() {
            let tx_nonce = start_nonce + idx as u64;
            let mut attempt = 0;
            let max_attempts = 3;

            loop {
                match client.provider.send_transaction(tx.clone()).await {
                    Ok(pending) => {
                        last_hash = pending.tx_hash().to_string();
                        last_submitted_nonce = tx_nonce;
                        break; // Success, move to next tx
                    }
                    Err(e) => {
                        let err_str = e.to_string().to_lowercase();
                        attempt += 1;

                        // Check if it's a nonce error and we have retries left
                        if (err_str.contains("nonce too low") || err_str.contains("already known"))
                            && attempt < max_attempts
                        {
                            tracing::warn!(
                                "Nonce error at tx {} (nonce {}), attempt {}/{}, resetting cache...",
                                idx,
                                tx_nonce,
                                attempt,
                                max_attempts
                            );

                            // Reset nonce cache and wait briefly
                            client.reset_nonce_cache().await;
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;

                            // Get fresh nonce and update this transaction
                            let fresh_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
                            let mut updated_tx = tx.clone();
                            updated_tx.set_nonce(fresh_nonce);

                            // Try again with updated transaction
                            match client.provider.send_transaction(updated_tx).await {
                                Ok(pending) => {
                                    last_hash = pending.tx_hash().to_string();
                                    last_submitted_nonce = fresh_nonce;
                                    break; // Success!
                                }
                                Err(e2) => {
                                    tracing::warn!("Retry failed: {}, will try once more...", e2);
                                    // Continue to next attempt
                                    continue;
                                }
                            }
                        } else {
                            // Non-nonce error or max retries exceeded
                            tracing::error!("Pipelined Tx at nonce {} failure: {}", tx_nonce, e);
                            break; // Continue with partial success
                        }
                    }
                }
            }
        }

        // 5. Update Nonce Manager with next nonce after last successful submission
        if let Some(manager) = &client.nonce_manager {
            let next_nonce = last_submitted_nonce.wrapping_add(1);
            manager.set(address, next_nonce).await;
        }

        if last_hash.is_empty() {
            anyhow::bail!("Failed to submit any transactions in pipeline.");
        }

        Ok(TaskResult {
            success: true,
            message: format!(
                "Pipelined {} {} transfers via Optimistic Burst.",
                count, symbol
            ),
            tx_hash: Some(last_hash),
        })
    }
}
