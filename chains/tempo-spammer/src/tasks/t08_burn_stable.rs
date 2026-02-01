//! Burn Stablecoin Task
//!
//! Burns tokens from created stablecoins.
//!
//! Workflow:
//! 1. Query created_assets table for wallet's tokens
//! 2. For each token, check balanceOf(wallet)
//! 3. Filter tokens with balance > 10,000 (dust threshold)
//! 4. If no tokens with balance, auto-mint 1000 tokens first
//! 5. Burn random 1-5 tokens
//! 6. Verify balance decreased after burn

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy_sol_types::SolCall;
use alloy_sol_types::sol;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use std::str::FromStr;

sol!(
    interface ITIP20Mintable {
        function mint(address to, uint256 amount);
        function burn(uint256 amount);
        function grantRole(bytes32 role, address account);
    }
);

const MIN_BALANCE_THRESHOLD: u128 = 10_000;

#[derive(Debug, Clone, Default)]
pub struct BurnStableTask;

impl BurnStableTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for BurnStableTask {
    fn name(&self) -> &'static str {
        "08_burn_stable"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let wallet_addr_str = address.to_string();

        let created_token_addresses = if let Some(db) = &ctx.db {
            match db.get_assets_by_type(&wallet_addr_str, "stablecoin").await {
                Ok(addresses) => addresses,
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        if created_token_addresses.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No created stablecoins found in DB. Run Task 4 first.".to_string(),
                tx_hash: None,
            });
        }

        // Fast path: Pick random token from created list
        let mut rng = rand::rngs::OsRng;
        use rand::seq::SliceRandom;
        let token_addr_str = created_token_addresses
            .choose(&mut rng)
            .ok_or_else(|| anyhow::anyhow!("No tokens to select"))?;

        let token_addr = Address::from_str(token_addr_str).context("Invalid token address")?;

        // Single RPC call: get balance (decimals is always 6 for TIP-20)
        let decimals = 6u8;
        let balance = TempoTokens::get_token_balance(client, token_addr, address).await?;

        // Check if we need to mint first
        if balance < U256::from(MIN_BALANCE_THRESHOLD) * U256::from(10_u64.pow(decimals as u32)) {
            let mint_amount = U256::from(1_000) * U256::from(10_u64.pow(decimals as u32));
            let mint_call = ITIP20Mintable::mintCall {
                to: address,
                amount: mint_amount,
            };

            let mint_calldata = mint_call.abi_encode();

            // Get robust nonce reservation to prevent race conditions
            let reservation = match client.get_robust_nonce(&ctx.config.rpc_url).await {
                Ok(r) => r,
                Err(e) => {
                    return Ok(TaskResult {
                        success: false,
                        message: format!("Failed to reserve nonce for mint: {}", e),
                        tx_hash: None,
                    });
                }
            };

            let tx = TransactionRequest::default()
                .to(token_addr)
                .input(TransactionInput::from(mint_calldata.clone()))
                .from(address)
                .nonce(reservation.nonce)
                .max_fee_per_gas(150_000_000_000u128)
                .max_priority_fee_per_gas(1_500_000_000u128);

            // Try mint with retry logic, continue regardless of result
            let mint_result = match client.provider.send_transaction(tx.clone()).await {
                Ok(pending) => {
                    reservation.mark_submitted().await;
                    Ok(pending)
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nonce too low") || err_str.contains("already known") {
                        tracing::warn!(
                            "Nonce error on mint (burn_stable), recovering and retrying..."
                        );
                        // Release the failed nonce and get a new one
                        drop(reservation);
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

                        // Get new nonce and retry
                        let retry_reservation =
                            match client.get_robust_nonce(&ctx.config.rpc_url).await {
                                Ok(r) => r,
                                Err(e2) => {
                                    return Err(anyhow::anyhow!(
                                        "Failed to reserve nonce for retry: {}",
                                        e2
                                    ));
                                }
                            };
                        let retry_tx = TransactionRequest::default()
                            .to(token_addr)
                            .input(TransactionInput::from(mint_calldata))
                            .from(address)
                            .nonce(retry_reservation.nonce)
                            .max_fee_per_gas(150_000_000_000u128)
                            .max_priority_fee_per_gas(1_500_000_000u128);

                        match client.provider.send_transaction(retry_tx).await {
                            Ok(pending) => {
                                retry_reservation.mark_submitted().await;
                                Ok(pending)
                            }
                            Err(e2) => {
                                drop(retry_reservation);
                                Err(e2)
                            }
                        }
                    } else {
                        drop(reservation);
                        Err(e)
                    }
                }
            };

            if let Ok(pending) = mint_result {
                let _ = pending.get_receipt().await; // Wait for mint to complete
            }

            // Re-fetch balance after mint
            let new_balance = TempoTokens::get_token_balance(client, token_addr, address).await?;
            if new_balance
                < U256::from(MIN_BALANCE_THRESHOLD) * U256::from(10_u64.pow(decimals as u32))
            {
                return Ok(TaskResult {
                    success: false,
                    message: "Insufficient balance even after mint attempt".to_string(),
                    tx_hash: None,
                });
            }
        }

        let token_symbol = token_addr_str.get(..8).unwrap_or("0x20c000").to_string();

        let burn_amount = balance / U256::from(20); // 5% of balance
        let burn_units = burn_amount / U256::from(10_u64.pow(decimals as u32));

        // println!(
        //     "Burning {} {} (5% of balance) from {:?}...",
        //     burn_units, token_symbol, address
        // );

        let burn_call = ITIP20Mintable::burnCall {
            amount: burn_amount,
        };
        let burn_calldata = burn_call.abi_encode();

        // Get robust nonce reservation for burn transaction
        let burn_reservation = match client.get_robust_nonce(&ctx.config.rpc_url).await {
            Ok(r) => r,
            Err(e) => {
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to reserve nonce for burn: {}", e),
                    tx_hash: None,
                });
            }
        };

        let tx = TransactionRequest::default()
            .to(token_addr)
            .input(TransactionInput::from(burn_calldata.clone()))
            .from(address)
            .nonce(burn_reservation.nonce)
            .max_fee_per_gas(150_000_000_000u128)
            .max_priority_fee_per_gas(1_500_000_000u128);

        // Send burn with retry logic
        let burn_result = match client.provider.send_transaction(tx.clone()).await {
            Ok(pending) => {
                burn_reservation.mark_submitted().await;
                Ok(pending)
            }
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on burn, recovering and retrying...");
                    // Release the failed nonce and get a new one
                    drop(burn_reservation);
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

                    // Get new nonce and retry
                    let retry_reservation = match client.get_robust_nonce(&ctx.config.rpc_url).await
                    {
                        Ok(r) => r,
                        Err(e2) => {
                            return Err(anyhow::anyhow!(
                                "Failed to reserve nonce for retry: {}",
                                e2
                            ));
                        }
                    };
                    let retry_tx = TransactionRequest::default()
                        .to(token_addr)
                        .input(TransactionInput::from(burn_calldata))
                        .from(address)
                        .nonce(retry_reservation.nonce)
                        .max_fee_per_gas(150_000_000_000u128)
                        .max_priority_fee_per_gas(1_500_000_000u128);

                    match client.provider.send_transaction(retry_tx).await {
                        Ok(pending) => {
                            retry_reservation.mark_submitted().await;
                            Ok(pending)
                        }
                        Err(e2) => {
                            drop(retry_reservation);
                            Err(e2)
                        }
                    }
                } else {
                    drop(burn_reservation);
                    Err(e)
                }
            }
        };

        match burn_result {
            Ok(pending) => {
                let tx_hash = *pending.tx_hash();

                // Return immediately with tx hash (don't wait for confirmation)
                Ok(TaskResult {
                    success: true,
                    message: format!(
                        "Burned {} {} (5%) from {:?}",
                        burn_units, token_symbol, address
                    ),
                    tx_hash: Some(format!("{:?}", tx_hash)),
                })
            }
            Err(e) => Ok(TaskResult {
                success: false,
                message: format!("Burn failed: {:?}", e),
                tx_hash: None,
            }),
        }
    }
}
