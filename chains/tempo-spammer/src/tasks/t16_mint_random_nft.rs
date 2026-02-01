//! Mint Random NFT Task
//!
//! Randomly selects an NFT from wallet's collection and mints 1-5 units to the same wallet.
//!
//! Workflow:
//! 1. Query database for NFT collections owned by wallet
//! 2. Randomly select one NFT collection
//! 3. Mint 1-5 random NFTs to the wallet from that collection
//! 4. Log results and return count

use crate::TempoClient;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy::sol;
use alloy_sol_types::SolCall;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use rand::seq::SliceRandom;
use std::str::FromStr;

// Minimal NFT interface for minting
sol! {
    interface IMinimalNFT {
        function mint(address to) external;
        function balanceOf(address owner) external view returns (uint256);
    }
}

#[derive(Debug, Clone, Default)]
pub struct MintRandomNftTask;

impl MintRandomNftTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for MintRandomNftTask {
    fn name(&self) -> &'static str {
        "16_mint_random_nft"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let wallet_address = address.to_string();

        let mut rng = rand::rngs::OsRng;

        // Step 1: Query database for NFT collections
        let available_collections = if let Some(db) = &ctx.db {
            // println!("🔍 Querying database for NFT collections...");

            match db.get_assets_by_type(&wallet_address, "nft").await {
                Ok(collections) => {
                    // println!(
                    //     "📊 Found {} NFT collection(s) in database",
                    //     collections.len()
                    // );
                    collections
                }
                Err(_e) => {
                    // println!("❌ Failed to query database: {:?}", _e);
                    Vec::new()
                }
            }
        } else {
            // println!("⚠️ No database available - no NFTs to mint");
            return Ok(TaskResult {
                success: false,
                message: "No NFT collections available to mint from.".to_string(),
                tx_hash: None,
            });
        };

        if available_collections.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No NFT collections found for this wallet.".to_string(),
                tx_hash: None,
            });
        }

        // Step 2: Randomly select one NFT collection
        let selected_collection = match available_collections.choose(&mut rng) {
            Some(collection) => {
                // println!("🎯 Selected NFT collection: {}", collection);
                collection.to_string()
            }
            None => {
                return Ok(TaskResult {
                    success: false,
                    message: "Failed to select random NFT collection.".to_string(),
                    tx_hash: None,
                });
            }
        };

        // Step 3: Parse contract address
        let contract_address = match Address::from_str(&selected_collection) {
            Ok(addr) => {
                // println!("✅ Parsed collection address: {:?}", addr);
                addr
            }
            Err(e) => {
                return Ok(TaskResult {
                    success: false,
                    message: format!(
                        "Invalid collection address {}: {:?}",
                        selected_collection, e
                    ),
                    tx_hash: None,
                });
            }
        };

        // Step 4: Generate random number of NFTs to mint (1-5)
        let nfts_to_mint = rng.gen_range(1..=5);
        // println!(
        //     "🎲 Will mint {} NFT(s) from collection {}",
        //     nfts_to_mint, selected_collection
        // );

        // Step 5: Mint NFTs
        let mut successful_mints = 0;
        let mut minted_token_ids = Vec::new();

        for _i in 0..nfts_to_mint {
            // println!("🪙 Minting NFT {}/{}...", _i + 1, nfts_to_mint);

            let mint_call = IMinimalNFT::mintCall { to: address };
            let mint_input = mint_call.abi_encode();

            // let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;

            let mint_tx = TransactionRequest::default()
                .to(contract_address)
                .input(TransactionInput::from(mint_input))
                .from(address)
                .gas_limit(5_000_000)
                .max_fee_per_gas(150_000_000_000u128)
                .max_priority_fee_per_gas(1_500_000_000u128);

            match client.provider.send_transaction(mint_tx.clone()).await {
                Ok(mint_pending) => {
                    let mint_hash = *mint_pending.tx_hash();
                    successful_mints += 1;
                    minted_token_ids.push(format!("{:?}", mint_hash));
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nonce too low") || err_str.contains("already known") {
                        tracing::warn!("Nonce error on NFT mint, resetting cache and retrying...");
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        if let Ok(pending) = client.provider.send_transaction(mint_tx).await {
                            let mint_hash = *pending.tx_hash();
                            successful_mints += 1;
                            minted_token_ids.push(format!("{:?}", mint_hash));
                        }
                    } else {
                        // println!("❌ Mint {} failed to send: {:?}", _i + 1, e);
                    }
                }
            }

            // Minimal delay just to not flood too hard if many items
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            // Small delay between mints to avoid nonce issues
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // Step 6: Return results
        Ok(TaskResult {
            success: true,
            message: format!(
                "Minted {}/{} NFT(s) from collection {}. Token IDs: [{}]",
                successful_mints,
                nfts_to_mint,
                selected_collection,
                minted_token_ids.join(", ")
            ),
            tx_hash: if minted_token_ids.is_empty() {
                None
            } else {
                Some(minted_token_ids.first().cloned().unwrap_or_default())
            },
        })
    }
}
