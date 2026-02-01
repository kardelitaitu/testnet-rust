//! Claim Viral Faucet Task
//!
//! Interacts with a deployed ViralFaucet to claim tokens.
//! Scans known faucets for balances and claims supported tokens.

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy::sol;
use alloy::sol_types::SolCall;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::seq::SliceRandom;
use std::str::FromStr;

// Reuse interface
sol!(
    #[sol(rpc)]
    contract ViralFaucet {
        function claim(address token, uint256 amount) external;
        function getBalance(address token) external view returns (uint256);
    }
);

#[derive(Debug, Clone, Default)]
pub struct ClaimViralFaucetTask;

impl ClaimViralFaucetTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for ClaimViralFaucetTask {
    fn name(&self) -> &'static str {
        "46_claim_viral_faucet"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let wallet_addr_str = format!("{:?}", address);

        // 1. Load Faucets from DB
        let faucets = if let Some(db) = &ctx.db {
            match db
                // Use get_all_assets_by_type to find faucets created by ANYONE
                .get_all_assets_by_type("viral_faucet")
                .await
            {
                Ok(addresses) => addresses,
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        if faucets.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No viral faucets found in DB to claim from.".to_string(),
                tx_hash: None,
            });
        }

        // Shuffle faucets
        let mut rng = rand::rngs::OsRng;
        let mut faucets = faucets;
        faucets.shuffle(&mut rng);

        // 2. Scan for claimable tokens
        let system_tokens = TempoTokens::get_system_tokens();

        for faucet_addr_str in faucets {
            let faucet_addr = if let Ok(addr) = Address::from_str(&faucet_addr_str) {
                addr
            } else {
                continue;
            };

            tracing::debug!("Checking faucet {:?}...", faucet_addr);

            for token in &system_tokens {
                // Check balance
                let balance_call = ViralFaucet::getBalanceCall {
                    token: token.address,
                };
                let balance_tx = TransactionRequest::default()
                    .to(faucet_addr)
                    .input(balance_call.abi_encode().into());

                let balance = if let Ok(data) = client.provider.call(balance_tx).await {
                    let res = ViralFaucet::getBalanceCall::abi_decode_returns(&data);
                    match res {
                        Ok(r) => r,
                        Err(_) => U256::ZERO,
                    }
                } else {
                    U256::ZERO
                };

                if balance > U256::ZERO {
                    // Try to claim 1 unit (1 * 10^decimals) OR just 1 unit?
                    // The deploy script funded with ~10-100 units (atomic?).
                    // T45 funded with `fund_amount` (large number).
                    // T46 should claim a small amount.
                    // Let's claim 1 whole token (10^decimals).
                    let decimals = TempoTokens::get_token_decimals(client, token.address)
                        .await
                        .unwrap_or(18);
                    let claim_amount = U256::from(1) * U256::from(10_u64.pow(decimals as u32));

                    if balance >= claim_amount {
                        // println!("Claiming {} {} from {:?}", TempoTokens::format_amount(claim_amount, decimals), token.symbol, faucet_addr);

                        let claim_call = ViralFaucet::claimCall {
                            token: token.address,
                            amount: claim_amount,
                        };
                        let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
                        let claim_tx = TransactionRequest::default()
                            .to(faucet_addr)
                            .input(claim_call.abi_encode().into())
                            .from(address)
                            .nonce(nonce)
                            .max_fee_per_gas(150_000_000_000u128)
                            .max_priority_fee_per_gas(1_500_000_000u128);

                        // Send with retry logic for nonce errors (1 retry)
                        let pending = match client.provider.send_transaction(claim_tx.clone()).await
                        {
                            Ok(p) => p,
                            Err(e) => {
                                let err_str = e.to_string().to_lowercase();
                                if err_str.contains("nonce too low")
                                    || err_str.contains("already known")
                                {
                                    tracing::warn!(
                                        "Nonce error on claim_viral_faucet, resetting cache and retrying..."
                                    );
                                    client.reset_nonce_cache().await;
                                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                                    // Rebuild with fresh nonce
                                    let fresh_nonce =
                                        client.get_pending_nonce(&ctx.config.rpc_url).await?;
                                    let retry_tx = TransactionRequest::default()
                                        .to(faucet_addr)
                                        .input(claim_call.abi_encode().into())
                                        .from(address)
                                        .nonce(fresh_nonce)
                                        .max_fee_per_gas(150_000_000_000u128)
                                        .max_priority_fee_per_gas(1_500_000_000u128);
                                    client
                                        .provider
                                        .send_transaction(retry_tx)
                                        .await
                                        .context("Claim failed")?
                                } else {
                                    return Err(e).context("Claim failed");
                                }
                            }
                        };
                        let tx_hash = *pending.tx_hash();
                        let receipt = pending
                            .get_receipt()
                            .await
                            .context("Failed to get receipt")?;

                        if receipt.inner.status() {
                            return Ok(TaskResult {
                                success: true,
                                message: format!(
                                    "Claimed {} {} from Viral Faucet {:?}",
                                    TempoTokens::format_amount(claim_amount, decimals),
                                    token.symbol,
                                    faucet_addr
                                ),
                                tx_hash: Some(format!("{:?}", tx_hash)),
                            });
                        } else {
                            // Continue to next token/faucet if failed (maybe cooldown)
                            // println!("Claim failed (reverted). Checking next...");
                        }
                    }
                }
            }
        }

        Ok(TaskResult {
            success: false,
            message: "Found faucets but no claimable balance/successful claim.".to_string(),
            tx_hash: None,
        })
    }
}
