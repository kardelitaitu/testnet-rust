use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::abi::Token;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::{seq::SliceRandom, Rng};
use std::sync::Arc;
use tracing::{debug, info};

#[derive(Default)]
pub struct NftTransferTask;

impl NftTransferTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for NftTransferTask {
    fn name(&self) -> &str {
        "13_nftTransfer"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();
        let recipient = AddressCache::get_random().context("Failed to get random address")?;

        let abi_path = "chains/xenea/contracts/TestNFT_abi.txt";
        let mnemonic_path = "core-logic/src/utils/mnemonic.txt";

        let mut rng = OsRng;
        let abi_json = std::fs::read_to_string(abi_path)
            .with_context(|| format!("Failed to read ABI from {}", abi_path))?;
        let abi: abi::Abi = serde_json::from_str(&abi_json)?;

        // Check DB for existing NFT contract
        let nft_address = if let Some(db) = &ctx.db {
            match db.get_all_assets_by_type("ERC721").await {
                Ok(contracts) if !contracts.is_empty() => {
                    let addr_str = contracts
                        .choose(&mut rng)
                        .context("Failed to pick contract")?;
                    debug!("Using existing NFT: {}", addr_str);
                    Some(
                        addr_str
                            .parse::<Address>()
                            .context("Invalid address in DB")?,
                    )
                }
                _ => None,
            }
        } else {
            None
        };

        // Calculate gas
        let gas_price = U256::from(1_100_000_000u64);
        let deploy_gas = crate::utils::gas::GasManager::LIMIT_DEPLOY * gas_price;
        let mint_gas = U256::from(600_000u64) * gas_price;
        let transfer_gas = crate::utils::gas::GasManager::LIMIT_SEND_MEME * gas_price;
        let estimated_gas = if nft_address.is_some() {
            mint_gas + transfer_gas
        } else {
            deploy_gas + mint_gas + transfer_gas
        };

        let balance = provider.get_balance(address, None).await?;
        if balance < estimated_gas {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient TXENE for gas: need {} Wei, have {} Wei",
                    estimated_gas, balance
                ),
                tx_hash: None,
            });
        }

        // Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());

        // Deploy new if no existing contract
        let nft_address = match nft_address {
            Some(addr) => addr,
            None => {
                let mnemonic_content =
                    std::fs::read_to_string(mnemonic_path).with_context(|| {
                        format!("Failed to read mnemonic file from {}", mnemonic_path)
                    })?;
                let words: Vec<&str> = mnemonic_content
                    .lines()
                    .map(|line| line.trim())
                    .filter(|line| !line.is_empty())
                    .collect();
                let word = words[rng.gen_range(0..words.len())];
                let mut chars = word.chars();
                let capitalized_word = match chars.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                };
                let nft_name = format!("{} Transfer NFT", capitalized_word);
                let nft_symbol = format!("{}TNFT", capitalized_word.chars().next().unwrap_or('T'));
                let nft_symbol_for_abi = nft_symbol.clone();
                let nft_symbol_for_db = nft_symbol.clone();

                let bytecode_path = "chains/xenea/contracts/TestNFT_bytecode.txt";
                let bytecode_hex = std::fs::read_to_string(bytecode_path)
                    .with_context(|| format!("Failed to read bytecode from {}", bytecode_path))?;
                let bytecode_raw = ethers::utils::hex::decode(bytecode_hex.trim())?;

                let constructor = abi.constructor().context("ABI missing constructor")?;
                let encoded_args = constructor.encode_input(
                    bytecode_raw,
                    &[
                        Token::String(nft_name.clone()),
                        Token::String(nft_symbol_for_abi),
                    ],
                )?;

                let deploy_nonce = nonce_manager.next().await?;
                let deploy_tx = TransactionRequest::new()
                    .from(address)
                    .data(Bytes::from(encoded_args))
                    .gas(crate::utils::gas::GasManager::LIMIT_DEPLOY)
                    .gas_price(gas_price)
                    .nonce(deploy_nonce);

                let pending_deploy = client.send_transaction(deploy_tx, None).await;
                match pending_deploy {
                    Ok(pending) => {
                        let tx_hash = format!("{:?}", pending.tx_hash());
                        match pending.await {
                            Ok(Some(receipt)) if receipt.status == Some(U64::from(1)) => {
                                let addr = receipt
                                    .contract_address
                                    .context("No contract address in receipt")?;
                                if let Some(db) = &ctx.db {
                                    let _ = db
                                        .log_asset_creation(
                                            &format!("{:?}", address),
                                            &format!("{:?}", addr),
                                            "ERC721",
                                            &nft_name,
                                            &nft_symbol_for_db,
                                        )
                                        .await;
                                }
                                addr
                            }
                            _ => {
                                let _ = nonce_manager.resync().await;
                                return Ok(TaskResult {
                                    success: false,
                                    message: format!("NFT deploy failed (tx: {})", tx_hash),
                                    tx_hash: Some(tx_hash),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        debug!("NFT deploy submit failed, resyncing nonce: {}", e);
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("Failed to submit NFT deploy tx: {}", e),
                            tx_hash: None,
                        });
                    }
                }
            }
        };

        debug!("Using NFT at {:?}", nft_address);
        let contract = Contract::new(nft_address, abi.clone(), Arc::new(provider.clone()));

        // Mint to self
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        let mnemonic_content = std::fs::read_to_string(mnemonic_path)
            .with_context(|| format!("Failed to read mnemonic file from {}", mnemonic_path))?;
        let words: Vec<&str> = mnemonic_content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect();
        let word = words[rng.gen_range(0..words.len())];
        let mut chars = word.chars();
        let capitalized_word = match chars.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        };

        let metadata_json = format!(
            r#"{{"name":"{}","description":"Transfer Test"}}"#,
            capitalized_word
        );
        let metadata_uri = format!(
            "data:application/json;base64,{}",
            BASE64.encode(metadata_json)
        );

        let mint_nonce = nonce_manager.next().await?;
        let mint_data = contract.encode("mint", (address, metadata_uri))?;
        let mint_tx = TransactionRequest::new()
            .to(nft_address)
            .data(mint_data)
            .gas(U256::from(600_000))
            .gas_price(gas_price)
            .nonce(mint_nonce)
            .from(address);

        let pending_mint = client.send_transaction(mint_tx, None).await;
        match pending_mint {
            Ok(pending) => {
                let mint_tx_hash = format!("{:?}", pending.tx_hash());
                match pending.await {
                    Ok(Some(receipt)) if receipt.status == Some(U64::from(1)) => {
                        info!("t13 mint succeeded");
                    }
                    _ => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("NFT mint failed (tx: {})", mint_tx_hash),
                            tx_hash: Some(mint_tx_hash),
                        });
                    }
                }
            }
            Err(e) => {
                debug!("NFT mint submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit NFT mint tx: {}", e),
                    tx_hash: None,
                });
            }
        }

        // Transfer
        let token_id = U256::one();
        let transfer_data = contract.encode("transferFrom", (address, recipient, token_id))?;
        let transfer_nonce = nonce_manager.next().await?;
        let transfer_tx = TransactionRequest::new()
            .to(nft_address)
            .data(transfer_data)
            .gas(crate::utils::gas::GasManager::LIMIT_SEND_MEME)
            .gas_price(gas_price)
            .nonce(transfer_nonce)
            .from(address);

        let pending_transfer = client.send_transaction(transfer_tx, None).await;

        match pending_transfer {
            Ok(pending) => {
                let transfer_tx_hash = format!("{:?}", pending.tx_hash());
                info!("t13 transfer tx hash: {:?}", pending.tx_hash());
                Ok(TaskResult {
                    success: true,
                    message: format!(
                        "Submitted NFT deploy + mint + transfer at {:?} to {:?}",
                        nft_address, recipient
                    ),
                    tx_hash: Some(transfer_tx_hash),
                })
            }
            Err(e) => {
                debug!("NFT transfer submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit NFT transfer tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
