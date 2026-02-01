//! Multi-Send Concurrent Stable Task
//!
//! Launches concurrent transfers of PathUSD or AlphaUSD.
//!
//! Workflow:
//! 1. Generate random recipients
//! 2. Send concurrent stable token transfers
//! 3. Collect results

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
pub struct MultiSendConcurrentStableTask;

impl MultiSendConcurrentStableTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for MultiSendConcurrentStableTask {
    fn name(&self) -> &'static str {
        "32_multi_send_concurrent_stable"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let wallet_addr_str = address.to_string();

        // 1. Select Stable Token
        let stable_tokens = if let Some(db) = &ctx.db {
            match db.get_assets_by_type(&wallet_addr_str, "stablecoin").await {
                Ok(addresses) => addresses,
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        let mut rng = rand::rngs::OsRng;

        // Pick from DB or fallback to random system token
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

        tracing::debug!(
            "Selected Stable Token for Concurrent: {} ({})",
            symbol,
            token_addr
        );

        let count = 2;
        let decimals = TempoTokens::get_token_decimals(client, token_addr).await?;
        let mut balance = TempoTokens::get_token_balance(client, token_addr, address).await?;

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
                Err(e) => {} // println!("Minting failed: {:?}", e),
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

        // println!(
        //     "Executing {} Concurrent {} Transfers (3% total)...",
        //     count, symbol
        // );

        let base_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;

        // Executing sequentially to avoid nonce collisions
        let mut futures = Vec::new();
        for i in 0..count {
            let recipient = get_random_address()?;
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

        for future in futures {
            match future.await {
                Ok(pending) => {
                    let tx_hash = *pending.tx_hash();
                    if let Ok(receipt) = pending.get_receipt().await {
                        if receipt.inner.status() {
                            success_count += 1;
                            last_hash = format!("{:?}", tx_hash);
                        }
                    }
                }
                Err(_) => {}
            }
        }

        // Update Nonce Manager
        if let Some(manager) = &client.nonce_manager {
            manager.set(address, base_nonce + count as u64).await;
        }

        // Return result immediately, removing the old futures loop
        return Ok(TaskResult {
            success: success_count > 0,
            message: format!(
                "Completed {}/{} sequential {} transfers.",
                success_count, count, symbol
            ),
            tx_hash: if last_hash.is_empty() {
                None
            } else {
                Some(last_hash)
            },
        });
    }
}
