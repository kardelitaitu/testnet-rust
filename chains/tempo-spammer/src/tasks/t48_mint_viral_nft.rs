//! Mint Viral NFT Task
//!
//! Mints an NFT from a ViralNFT collection.
//! Scans DB for known collections, checks balance, and mints if eligible.

use crate::TempoClient;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::TransactionRequest;
use alloy::sol;
use alloy::sol_types::SolCall;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::seq::SliceRandom;
use std::str::FromStr;

// Subset of ViralNFT interface needed for minting
sol!(
    #[sol(rpc)]
    contract ViralNFT {
        function claim() external;
        function balanceOf(address owner) external view returns (uint256);
    }
);

#[derive(Debug, Clone, Default)]
pub struct MintViralNftTask;

impl MintViralNftTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for MintViralNftTask {
    fn name(&self) -> &'static str {
        "48_mint_viral_nft"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let wallet_addr_str = format!("{:?}", address);

        // 1. Load NFTs from DB
        let nfts = if let Some(db) = &ctx.db {
            // Use get_all_assets_by_type to find NFTs created by ANYONE
            match db.get_all_assets_by_type("viral_nft").await {
                Ok(addresses) => addresses,
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        if nfts.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No viral NFTs found in DB to mint.".to_string(),
                tx_hash: None,
            });
        }

        // Shuffle
        let mut rng = rand::rngs::OsRng;
        let mut nfts = nfts;
        nfts.shuffle(&mut rng);

        for nft_addr_str in nfts {
            let nft_addr = if let Ok(addr) = Address::from_str(&nft_addr_str) {
                addr
            } else {
                continue;
            };

            // 2. Check Balance
            let balance_call = ViralNFT::balanceOfCall { owner: address };
            let balance_tx = TransactionRequest::default()
                .to(nft_addr)
                .input(balance_call.abi_encode().into());

            let balance = if let Ok(data) = client.provider.call(balance_tx).await {
                let res = ViralNFT::balanceOfCall::abi_decode_returns(&data);
                match res {
                    Ok(r) => r,
                    Err(_) => U256::ZERO,
                }
            } else {
                // If call fails, contract might not exist or other issue. Skip.
                continue;
            };

            if balance == U256::ZERO {
                // 3. Claim with retry logic for nonce errors
                tracing::debug!("Minting Viral NFT at {:?}...", nft_addr);

                let mut last_error = None;
                let max_retries = 3;

                for attempt in 0..max_retries {
                    let claim_call = ViralNFT::claimCall {};

                    // Reset nonce cache on retry to avoid "nonce too low" errors
                    if attempt > 0 {
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }

                    let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
                    let claim_tx = TransactionRequest::default()
                        .to(nft_addr)
                        .input(claim_call.abi_encode().into())
                        .from(address)
                        .nonce(nonce)
                        .max_fee_per_gas(150_000_000_000u128)
                        .max_priority_fee_per_gas(1_500_000_000u128);

                    match client.provider.send_transaction(claim_tx).await {
                        Ok(pending) => {
                            let tx_hash = *pending.tx_hash();
                            let receipt = pending
                                .get_receipt()
                                .await
                                .context("Failed to get receipt")?;

                            if receipt.inner.status() {
                                return Ok(TaskResult {
                                    success: true,
                                    message: format!("Minted Viral NFT at {:?}", nft_addr),
                                    tx_hash: Some(format!("{:?}", tx_hash)),
                                });
                            } else {
                                tracing::debug!("Mint failed (reverted), trying next NFT...");
                                break; // Try next NFT
                            }
                        }
                        Err(e) => {
                            let err_str = e.to_string().to_lowercase();
                            if err_str.contains("nonce too low")
                                || err_str.contains("already known")
                            {
                                tracing::warn!(
                                    "Nonce error on attempt {}, will retry...",
                                    attempt + 1
                                );
                                last_error = Some(e);
                                continue; // Retry with fresh nonce
                            }
                            if err_str.contains("execution reverted") {
                                // Check for known error selector 0xaa4bc69a (Likely AlreadyClaimed or SoldOut)
                                if err_str.contains("aa4bc69a") {
                                    tracing::debug!(
                                        "NFT sold out or already claimed, trying next..."
                                    );
                                    break; // Try next NFT
                                }
                                // Generic revert - try next NFT
                                tracing::debug!("Mint reverted: {}, trying next NFT...", err_str);
                                break;
                            }
                            return Err(e).context("Mint failed");
                        }
                    }
                }

                if let Some(e) = last_error {
                    tracing::warn!("All retry attempts failed for {:?}: {:?}", nft_addr, e);
                }
            } else {
                tracing::debug!("Already own {:?} (Bal: {}), skipping...", nft_addr, balance);
            }
        }

        Ok(TaskResult {
            success: false,
            message: "Found NFTs but already owned or mint failed.".to_string(),
            tx_hash: None,
        })
    }
}
