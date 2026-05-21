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
use futures::future::join_all;
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

        // 2. Mint if needed (Skip for PathUSD/AlphaUSD/BetaUSD/ThetaUSD as we can't mint them)
        let is_system_token = TempoTokens::SYSTEM_TOKENS.iter().any(|(s, _)| *s == symbol);

        if (balance.is_zero() || amount_per_recipient.is_zero()) && !is_system_token {
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
                .max_priority_fee_per_gas(1_500_000_000u128)
                .gas_limit(200_000);

            match client.provider.send_transaction(mint_tx).await {
                Ok(pending) => {
                    let _ = pending.get_receipt().await;
                    balance = TempoTokens::get_token_balance(client, token_addr, address).await?;
                    total_impact = balance * U256::from(3) / U256::from(100);
                    amount_per_recipient = total_impact / U256::from(count);
                }
                Err(e) => {} // println!("Minting failed: {:?}", e),
            }
        } else if is_system_token && (balance.is_zero() || amount_per_recipient.is_zero()) {
            // Try faucet if balance is zero or too low for system token
            // Use Faucet Logic (similar to t02/t17)
            let faucet_address =
                Address::from_str("0x4200000000000000000000000000000000000019").unwrap();
            // Selector 0x4f9828f6 + address padded
            let mut data = hex::decode("4f9828f6000000000000000000000000").unwrap();
            data.extend_from_slice(address.as_slice());

            let faucet_tx = TransactionRequest::default()
                .to(faucet_address)
                .input(data.into())
                .from(address)
                .gas_limit(500_000);

            match client.provider.send_transaction(faucet_tx).await {
                Ok(pending) => {
                    let _ = pending.get_receipt().await;
                    // Refresh balance
                    balance = TempoTokens::get_token_balance(client, token_addr, address).await?;
                    total_impact = balance * U256::from(3) / U256::from(100);
                    amount_per_recipient = total_impact / U256::from(count);
                }
                Err(_) => {} // Ignore faucet error
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

        // Reserve nonces using RobustNonceManager if available, otherwise legacy or RPC
        let mut reservations = Vec::new();
        let mut nonces = Vec::new();

        if let Some(robust_manager) = &client.robust_nonce_manager {
            for _ in 0..count {
                if let Ok(reservation) = client.get_robust_nonce(&ctx.config.rpc_url).await {
                    nonces.push(reservation.nonce);
                    reservations.push(reservation);
                }
            }
        }

        // If robust manager failed or not configured, fallback to legacy/RPC
        if nonces.len() < count {
            // Release any partial reservations
            for r in reservations.drain(..) {
                r.release().await;
            }
            // reservations is now empty
            nonces.clear();

            if let Some(manager) = &client.nonce_manager {
                // Use atomic nonce reservation
                let start_nonce = manager.get_and_increment(address).await.unwrap_or_else(|| {
                    // Fallback: get from RPC and initialize
                    0u64
                });

                // Reserve all nonces upfront
                nonces = (0..count).map(|i| start_nonce + i as u64).collect();

                // Pre-advance the manager to skip all reserved nonces
                manager.set(address, start_nonce + count as u64).await;
            } else {
                // Fallback: get from RPC
                let start_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
                nonces = (0..count).map(|i| start_nonce + i as u64).collect();
            }
        }

        // Send transactions concurrently
        let mut send_futures = Vec::new();
        // We need to match reservations to nonces if we have them
        let has_reservations = !reservations.is_empty();

        for (i, nonce) in nonces.iter().enumerate() {
            let recipient = get_random_address()?;
            let transfer_call = IERC20Mintable::transferCall {
                recipient,
                amount: amount_per_recipient,
            };

            let tx = TransactionRequest::default()
                .to(token_addr)
                .input(transfer_call.abi_encode().into())
                .from(address)
                .nonce(*nonce)
                .max_fee_per_gas(200_000_000_000u128)
                .max_priority_fee_per_gas(2_000_000_000u128)
                .gas_limit(1_000_000);

            let client_clone = client.clone();
            send_futures.push(async move { client_clone.provider.send_transaction(tx).await });
        }

        // Wait for all sends to complete
        let send_results = join_all(send_futures).await;

        let mut success_count = 0;
        let mut last_hash = String::new();
        let mut failed_nonces = Vec::new();
        let mut errors = Vec::new();
        let mut results_status = Vec::new();

        for (i, result) in send_results.into_iter().enumerate() {
            match result {
                Ok(pending) => {
                    let tx_hash = *pending.tx_hash();
                    match pending.get_receipt().await {
                        Ok(receipt) => {
                            if receipt.inner.status() {
                                success_count += 1;
                                last_hash = format!("{:?}", tx_hash);
                                results_status.push(true);
                            } else {
                                results_status.push(false);
                                errors.push(format!(
                                    "transaction reverted {}",
                                    format!("{:?}", tx_hash)
                                ));
                            }
                        }
                        Err(e) => {
                            results_status.push(false);
                            errors.push(e.to_string());
                        }
                    }
                }
                Err(e) => {
                    results_status.push(false);
                    let err_str = e.to_string();
                    errors.push(err_str.clone());
                    let lower_err = err_str.to_lowercase();
                    if lower_err.contains("nonce too low") || lower_err.contains("already known") {
                        failed_nonces.push(nonces[i]);
                    }
                }
            }
        }

        // Handle reservations (submit or release)
        if has_reservations {
            for (i, reservation) in reservations.into_iter().enumerate() {
                if i < results_status.len() && results_status[i] {
                    reservation.mark_submitted().await;
                } else {
                    reservation.release().await;
                }
            }
        }

        // Reset nonce manager if there were failures to resync
        if !failed_nonces.is_empty() && client.nonce_manager.is_some() {
            client.reset_nonce_cache().await;
        }

        if success_count == 0 && !errors.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: format!("Failed all transfers. Error: {}", errors[0]),
                tx_hash: None,
            });
        }

        // Return result immediately
        return Ok(TaskResult {
            success: success_count > 0,
            message: format!(
                "Completed {}/{} concurrent {} transfers.",
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
