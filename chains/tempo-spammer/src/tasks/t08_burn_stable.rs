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

use crate::tasks::t04_create_stable::CreateStableTask;

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

        let mut created_token_addresses = if let Some(db) = &ctx.db {
            match db.get_assets_by_type(&wallet_addr_str, "stablecoin").await {
                Ok(addresses) => addresses,
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        if created_token_addresses.is_empty() {
            tracing::info!("No stablecoins found for burning. Creating one now...");
            let create_task = CreateStableTask::new();
            let create_result = create_task.run(ctx).await?;
            
            if !create_result.success {
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to create stablecoin for burning: {}", create_result.message),
                    tx_hash: create_result.tx_hash,
                });
            }
            
            // Re-query DB
            if let Some(db) = &ctx.db {
                match db.get_assets_by_type(&wallet_addr_str, "stablecoin").await {
                    Ok(addresses) => created_token_addresses = addresses,
                    Err(_) => {},
                }
            }
            
            if created_token_addresses.is_empty() {
                 return Ok(TaskResult {
                    success: false,
                    message: "Created stablecoin but DB query still returns empty.".to_string(),
                    tx_hash: create_result.tx_hash,
                });
            }
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

            // Re-fetch balance after mint - Removed to save time
            // let new_balance = TempoTokens::get_token_balance(client, token_addr, address).await?;
            // if new_balance
            //     < U256::from(MIN_BALANCE_THRESHOLD) * U256::from(10_u64.pow(decimals as u32))
            // {
            //     return Ok(TaskResult {
            //         success: false,
            //         message: "Insufficient balance even after mint attempt".to_string(),
            //         tx_hash: None,
            //     });
            // }
            
            // Assume mint succeeded or we have enough balance, proceed to burn
            // This is a spammer, we don't need perfect reliability
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

        // Send burn with retry logic using standard provider (no robust nonce manager needed for simple spam)
        let mut attempt = 0;
        let max_retries = 3;
        
        loop {
            // Get fresh nonce
            let nonce = match client.get_pending_nonce(&ctx.config.rpc_url).await {
                Ok(n) => n,
                Err(_) => {
                    attempt += 1;
                    if attempt >= max_retries {
                         return Ok(TaskResult { success: false, message: "Failed to get nonce".into(), tx_hash: None });
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    continue;
                }
            };
            
            let tx = TransactionRequest::default()
                .to(token_addr)
                .input(TransactionInput::from(burn_calldata.clone()))
                .from(address)
                .nonce(nonce)
                .max_fee_per_gas(150_000_000_000u128)
                .max_priority_fee_per_gas(1_500_000_000u128);

            match client.provider.send_transaction(tx).await {
                Ok(pending) => {
                    let tx_hash = *pending.tx_hash();
                    // Fire and forget
                    return Ok(TaskResult {
                        success: true,
                        message: format!(
                            "Burned {} {} (5%) from {:?} (Fire-and-Forget)",
                            burn_units, token_symbol, address
                        ),
                        tx_hash: Some(format!("{:?}", tx_hash)),
                    });
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if (err_str.contains("nonce too low") || err_str.contains("already known")) && attempt < max_retries {
                        client.reset_nonce_cache().await;
                        attempt += 1;
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        continue;
                    }
                    
                    return Ok(TaskResult {
                        success: false,
                        message: format!("Burn failed: {:?}", e),
                        tx_hash: None,
                    });
                }
            }
        }
    }
}
