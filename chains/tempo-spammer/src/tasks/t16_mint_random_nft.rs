//! Mint Random NFT Task
//!
//! Randomly selects an NFT from wallet's collection and mints 1-5 units to the same wallet.
//! If no NFT collections exist, deploys a new one first.
//!
//! Workflow:
//! 1. Query database for NFT collections owned by wallet
//! 2. If none exist, deploy a new NFT collection
//! 3. Randomly select one NFT collection (existing or newly created)
//! 4. Mint 1-5 random NFTs to the wallet from that collection
//! 5. Log results and return count

use crate::TempoClient;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, TxKind, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy::sol;
use alloy_sol_types::{SolCall, SolEvent};
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
        function grantRole(address minter) external;
        event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);
    }
}

// Load NFT bytecode from compiled contract file at compile time
fn load_nft_bytecode() -> Result<Vec<u8>> {
    let bytecode_hex = include_str!("../contracts/MinimalNFT.bin");
    hex::decode(bytecode_hex.trim()).context("Loading NFT contract bytecode")
}

// Error message constants
const ERR_NONCE_TOO_LOW: &str = "nonce too low";
const ERR_ALREADY_KNOWN: &str = "already known";

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
            match db.get_assets_by_type(&wallet_address, "nft").await {
                Ok(collections) => collections,
                Err(_e) => Vec::new(),
            }
        } else {
            return Ok(TaskResult {
                success: false,
                message: "No NFT collections available to mint from.".to_string(),
                tx_hash: None,
            });
        };

        // Step 2: If no collections exist, deploy a new one
        let selected_collection = if available_collections.is_empty() {
            tracing::info!(
                "No NFT collections found for wallet {}, deploying new collection...",
                wallet_address
            );

            // Deploy new NFT collection
            match deploy_nft_collection(client, address, ctx).await {
                Ok(contract_address) => {
                    tracing::info!(
                        "Successfully deployed NFT collection at {}",
                        contract_address
                    );
                    contract_address
                }
                Err(e) => {
                    return Ok(TaskResult {
                        success: false,
                        message: format!(
                            "No NFT collections found and failed to deploy new one: {}",
                            e
                        ),
                        tx_hash: None,
                    });
                }
            }
        } else {
            // Randomly select one NFT collection
            match available_collections.choose(&mut rng) {
                Some(collection) => {
                    tracing::debug!("Selected NFT collection: {}", collection);
                    collection.to_string()
                }
                None => {
                    return Ok(TaskResult {
                        success: false,
                        message: "Failed to select random NFT collection.".to_string(),
                        tx_hash: None,
                    });
                }
            }
        };

        // Step 3: Parse contract address
        let contract_address = match Address::from_str(&selected_collection) {
            Ok(addr) => addr,
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

        // Step 5: Mint NFTs
        let mut successful_mints = 0;
        let mut minted_token_ids = Vec::new();

        for _i in 0..nfts_to_mint {
            let mint_call = IMinimalNFT::mintCall { to: address };
            let mint_input = mint_call.abi_encode();

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
                    let nonce_error =
                        err_str.contains(ERR_NONCE_TOO_LOW) || err_str.contains(ERR_ALREADY_KNOWN);
                    if nonce_error {
                        tracing::warn!("Nonce error on NFT mint, resetting cache and retrying...");
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        if let Ok(pending) = client.provider.send_transaction(mint_tx).await {
                            let mint_hash = *pending.tx_hash();
                            successful_mints += 1;
                            minted_token_ids.push(format!("{:?}", mint_hash));
                        }
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

/// Deploy a new NFT collection for the wallet
async fn deploy_nft_collection(
    client: &TempoClient,
    address: Address,
    ctx: &TaskContext,
) -> Result<String> {
    let bytecode = load_nft_bytecode()?;

    let mut deploy_tx = TransactionRequest::default()
        .input(TransactionInput::from(bytecode))
        .from(address)
        .max_fee_per_gas(150_000_000_000u128)
        .max_priority_fee_per_gas(1_500_000_000u128);

    deploy_tx.to = Some(TxKind::Create);

    // Send deploy with retry logic
    let pending = match client.provider.send_transaction(deploy_tx.clone()).await {
        Ok(p) => p,
        Err(e) => {
            let err_str = e.to_string().to_lowercase();
            let nonce_error =
                err_str.contains(ERR_NONCE_TOO_LOW) || err_str.contains(ERR_ALREADY_KNOWN);
            if nonce_error {
                tracing::warn!("Nonce error on NFT deploy, resetting cache and retrying...");
                client.reset_nonce_cache().await;
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                client
                    .provider
                    .send_transaction(deploy_tx)
                    .await
                    .context("Sending deployment transaction")?
            } else {
                return Err(e).context("Creating NFT contract");
            }
        }
    };

    let tx_hash = *pending.tx_hash();
    let receipt = pending
        .get_receipt()
        .await
        .context("Getting transaction confirmation")?;

    if !receipt.inner.status() {
        anyhow::bail!("NFT deployment transaction failed. Tx: {:?}", tx_hash);
    }

    let contract_address = receipt
        .contract_address
        .ok_or_else(|| anyhow::anyhow!("Missing contract address in transaction result"))?;

    let contract_address_str = format!("{:?}", contract_address);

    // Grant minter role to self
    let grant_call = IMinimalNFT::grantRoleCall { minter: address };
    let grant_input = grant_call.abi_encode();

    let grant_tx = TransactionRequest::default()
        .to(contract_address)
        .input(TransactionInput::from(grant_input))
        .from(address)
        .max_fee_per_gas(150_000_000_000u128)
        .max_priority_fee_per_gas(1_500_000_000u128);

    match client.provider.send_transaction(grant_tx).await {
        Ok(grant_pending) => {
            let _ = grant_pending.get_receipt().await;
            tracing::debug!(
                "Granted minter role for NFT collection {}",
                contract_address_str
            );
        }
        Err(e) => {
            tracing::warn!("Failed to grant minter role (continuing anyway): {}", e);
        }
    }

    // Log to database if available
    if let Some(db) = &ctx.db {
        if let Err(e) = db
            .log_asset_creation(
                &format!("{:?}", address),
                &contract_address_str,
                "nft",
                "TempoNFT",
                "TNF",
            )
            .await
        {
            tracing::warn!("Failed to log NFT creation to database: {}", e);
        }
    }

    Ok(contract_address_str)
}
