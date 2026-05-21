use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::abi::Token;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::{seq::SliceRandom, Rng};
use std::sync::Arc;
use tracing::{debug, info};

pub struct NftMintTask;

impl NftMintTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for NftMintTask {
    fn name(&self) -> &str {
        "12_nftMint"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let abi_path = "chains/xenea/contracts/TestNFT_abi.txt";
        let mnemonic_path = "core-logic/src/utils/mnemonic.txt";

        let recipient = address;
        let mut rng = OsRng;

        // Load ABI
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

        // Calculate gas - cheaper if using existing contract
        let gas_price = U256::from(1_100_000_000u64);
        let deploy_gas = crate::utils::gas::GasManager::LIMIT_DEPLOY * gas_price;
        let mint_gas = U256::from(600_000u64) * gas_price;
        let estimated_gas = if nft_address.is_some() {
            mint_gas
        } else {
            deploy_gas + mint_gas
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
                // Generate random NFT metadata
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
                let nft_name = format!("{} NFT", capitalized_word);
                let nft_symbol = format!("{}NFT", capitalized_word.chars().next().unwrap_or('T'));
                let nft_symbol_clone = nft_symbol.clone();
                debug!("Random NFT Name: '{}' ({})", nft_name, nft_symbol);

                // Load and deploy bytecode
                let bytecode_path = "chains/xenea/contracts/TestNFT_bytecode.txt";
                let bytecode_hex = std::fs::read_to_string(bytecode_path)
                    .with_context(|| format!("Failed to read bytecode from {}", bytecode_path))?;
                let bytecode_raw = ethers::utils::hex::decode(bytecode_hex.trim())?;

                let constructor = abi.constructor().context("ABI missing constructor")?;
                let encoded_args = constructor.encode_input(
                    bytecode_raw,
                    &[
                        Token::String(nft_name.clone()),
                        Token::String(nft_symbol_clone),
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
                                // Save to DB
                                if let Some(db) = &ctx.db {
                                    let _ = db
                                        .log_asset_creation(
                                            &format!("{:?}", address),
                                            &format!("{:?}", addr),
                                            "ERC721",
                                            &nft_name,
                                            &nft_symbol,
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

        // Build metadata
        let token_id: u64 = rng.gen_range(1000000..9999999);
        let r: u8 = rng.gen();
        let g: u8 = rng.gen();
        let b: u8 = rng.gen();
        let color_hex = format!("#{:02X}{:02X}{:02X}", r, g, b);
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="500" height="500" viewBox="0 0 500 500"><rect width="500" height="500" fill="{}"/></svg>"#,
            color_hex
        );

        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        let svg_base64 = BASE64.encode(&svg);
        let image_uri = format!("data:image/svg+xml;base64,{}", svg_base64);

        // Get word for metadata
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

        let metadata_json = serde_json::json!({
            "name": format!("{} NFT", capitalized_word),
            "description": format!("NFT of {}", capitalized_word),
            "image": image_uri,
            "external_url": "https://testnet.riselabs.xyz",
            "attributes": [
                { "trait_type": "Color", "value": color_hex },
                { "trait_type": "Word", "value": capitalized_word }
            ]
        });

        let json_str = serde_json::to_string(&metadata_json)?;
        let token_uri = format!("data:application/json;base64,{}", BASE64.encode(&json_str));

        // Mint (fire-and-forget)
        let mint_nonce = nonce_manager.next().await?;
        let mint_data = contract.encode("mint", (recipient, token_uri.clone()))?;
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
                info!("t12 mint tx hash: {:?}", pending.tx_hash());
                Ok(TaskResult {
                    success: true,
                    message: format!(
                        "Submitted NFT deploy + mint at {:?} (URI ID: {})",
                        nft_address, token_id
                    ),
                    tx_hash: Some(mint_tx_hash),
                })
            }
            Err(e) => {
                debug!("NFT mint submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit NFT mint tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
