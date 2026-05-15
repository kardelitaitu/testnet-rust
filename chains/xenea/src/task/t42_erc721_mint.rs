use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::{Rng, seq::SliceRandom};
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tracing::debug;

pub struct ERC721MintTask;

impl ERC721MintTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for ERC721MintTask {
    fn name(&self) -> &str {
        "42_erc721Mint"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let gas_price = U256::from(1_100_000_000u64);
        let deploy_gas_limit = ctx.gas_manager.limit_deploy().as_u64();
        let mint_gas_limit = 1_000_000u64;

        // Check DB for existing ERC721 contract
        let nft_address = if let Some(db) = &ctx.db {
            match db.get_all_assets_by_type("ERC721").await {
                Ok(contracts) if !contracts.is_empty() => {
                    let mut rng = OsRng;
                    let addr_str = contracts.choose(&mut rng).context("Failed to pick contract")?;
                    debug!("Using existing ERC721: {}", addr_str);
                    Some(addr_str.parse::<Address>().context("Invalid address in DB")?)
                }
                _ => None,
            }
        } else {
            None
        };

        let estimated_gas = if nft_address.is_some() {
            U256::from(mint_gas_limit) * gas_price
        } else {
            U256::from(deploy_gas_limit + mint_gas_limit) * gas_price
        };

        // 1. Balance check
        let balance = provider.get_balance(address, None).await?;
        if balance <= estimated_gas {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient TXENE for gas: need {} Wei, have {} Wei",
                    estimated_gas, balance
                ),
                tx_hash: None,
            });
        }

        // 2. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());

        // Load ABI
        let abi_str = include_str!("../../contracts/TestNFT_abi.txt").trim();
        let abi: abi::Abi = serde_json::from_str(abi_str).context("Failed to parse ABI")?;

        // Deploy new if no existing contract
        let nft_address = match nft_address {
            Some(addr) => addr,
            None => {
                let bytecode_str = include_str!("../../contracts/TestNFT_bytecode.txt").trim();
                let mut bytecode = hex::decode(bytecode_str).context("Failed to decode bytecode")?;

                let encoded_args = ethers::abi::encode(&[
                    ethers::abi::Token::String("TestNFT".to_string()),
                    ethers::abi::Token::String("TNFT".to_string()),
                ]);
                bytecode.extend(encoded_args);

                let deploy_nonce = nonce_manager.next().await?;
                let deploy_tx = TransactionRequest::new()
                    .data(Bytes::from(bytecode))
                    .gas(deploy_gas_limit)
                    .gas_price(gas_price)
                    .nonce(deploy_nonce)
                    .from(address);

                let pending_deploy = client.send_transaction(deploy_tx, None).await;
                match pending_deploy {
                    Ok(pending) => {
                        let tx_hash = format!("{:?}", pending.tx_hash());
                        match timeout(Duration::from_secs(90), pending).await {
                            Ok(Ok(Some(receipt))) if receipt.status == Some(U64::from(1)) => {
                                let addr = receipt.contract_address.context("No contract address in receipt")?;
                                if let Some(db) = &ctx.db {
                                    let _ = db.log_asset_creation(
                                        &format!("{:?}", address),
                                        &format!("{:?}", addr),
                                        "ERC721",
                                        "TestNFT",
                                        "TNFT",
                                    ).await;
                                }
                                addr
                            }
                            Ok(Ok(Some(_))) => {
                                let _ = nonce_manager.resync().await;
                                return Ok(TaskResult {
                                    success: false,
                                    message: format!("ERC721 deploy reverted (tx: {})", tx_hash),
                                    tx_hash: Some(tx_hash),
                                });
                            }
                            Ok(Ok(None)) | Err(_) => {
                                let _ = nonce_manager.resync().await;
                                return Ok(TaskResult {
                                    success: false,
                                    message: format!("ERC721 deploy timed out (tx: {})", tx_hash),
                                    tx_hash: Some(tx_hash),
                                });
                            }
                            Ok(Err(e)) => {
                                let _ = nonce_manager.resync().await;
                                return Ok(TaskResult {
                                    success: false,
                                    message: format!("ERC721 deploy receipt failed (tx: {}): {}", tx_hash, e),
                                    tx_hash: Some(tx_hash),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        debug!("ERC721Mint deploy submit failed, resyncing nonce: {}", e);
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("Failed to submit ERC721 deploy tx: {}", e),
                            tx_hash: None,
                        });
                    }
                }
            }
        };

        debug!("Using ERC721 at {:?}", nft_address);
        let contract = Contract::new(nft_address, abi, Arc::new(provider.clone()));

        let total_before: U256 = contract
            .method("totalSupply", ())?
            .call()
            .await
            .context("Failed to get total supply")?;

        // 4. Mint (fire-and-forget)
        let mut rng = OsRng;
        let token_id: u64 = rng.gen();
        let token_uri = format!("https://xenea-testnet.io/metadata/{}", token_id);

        let mint_nonce = nonce_manager.next().await?;
        let mint_data = contract.encode("mint", (address, token_uri.clone()))?;

        let mint_tx = TransactionRequest::new()
            .to(nft_address)
            .data(mint_data)
            .gas(mint_gas_limit)
            .gas_price(gas_price)
            .nonce(mint_nonce)
            .from(address);

        let pending_mint = client.send_transaction(mint_tx, None).await;

        match pending_mint {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!(
                    "ERC721 at {:?}, total supply: {}, mint token #{} (tx: {:?})",
                    nft_address, total_before, token_id, pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("ERC721Mint mint submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit ERC721 mint tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}