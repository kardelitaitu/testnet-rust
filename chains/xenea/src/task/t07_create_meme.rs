use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::{seq::SliceRandom, Rng};
use std::sync::Arc;
use tracing::{debug, info};

use crate::contracts::{MEME_TOKEN_ABI, MEME_TOKEN_BYTECODE};
use crate::task::{Task, TaskContext, TaskResult};

pub struct CreateMemeTask;

#[async_trait]
impl Task<TaskContext> for CreateMemeTask {
    fn name(&self) -> &str {
        "07_createMeme"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // 1. Generate Meme Name and Symbol
        let (name, symbol) = {
            let mut rng = OsRng;
            let prefixes = [
                "Dog", "Cat", "Pepe", "Elon", "Moon", "Safe", "Rich", "Shiba", "Giga", "Turbo",
            ];
            let suffixes = [
                "Inu", "Coin", "Token", "Moon", "Rocket", "Mars", "Alpha", "Chad", "Wif",
            ];

            let prefix = prefixes.choose(&mut rng).unwrap_or(&"Pepe");
            let suffix = suffixes.choose(&mut rng).unwrap_or(&"Coin");

            let name = format!("{} {}", prefix, suffix);
            let symbol = format!(
                "{}{}",
                prefix.chars().next().unwrap_or('P'),
                suffix.chars().next().unwrap_or('C')
            )
            .to_uppercase();
            (name, symbol)
        };

        let mut rng = OsRng;
        let minted_whole = rng.gen_range(1..=100) * 1_000_000u64;
        let minted_amount: U256 = ethers::utils::parse_units(minted_whole.to_string(), 18)?.into();

        // 2. Prepare Deployment
        let abi: abi::Abi = serde_json::from_str(MEME_TOKEN_ABI)?;
        let bytecode_vector = ethers::utils::hex::decode(MEME_TOKEN_BYTECODE)?;
        let bytecode = Bytes::from(bytecode_vector);

        let gas_price = U256::from(1_100_000_000u64);
        let deploy_gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;
        let mint_gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;
        let estimated_gas = U256::from(deploy_gas_limit.as_u64() + mint_gas_limit.as_u64()) * gas_price;

        // 3. Balance check
        let balance = provider.get_balance(address, None).await?;
        if balance < estimated_gas {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient funds: need {} Wei, have {} Wei",
                    estimated_gas, balance
                ),
                tx_hash: None,
            });
        }

        // 4. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());

        // 5. Deploy (wait for address)
        let input = abi
            .constructor()
            .context("No constructor found")?
            .encode_input(
                bytecode.to_vec(),
                &[
                    abi::Token::String(name.clone()),
                    abi::Token::String(symbol.clone()),
                ],
            )?;

        let deploy_nonce = nonce_manager.next().await?;
        let deploy_tx = TransactionRequest::new()
            .from(address)
            .data(Bytes::from(input))
            .gas_price(gas_price)
            .gas(deploy_gas_limit)
            .nonce(deploy_nonce);

        let pending_deploy = client.send_transaction(deploy_tx, None).await;
        let token_address = match pending_deploy {
            Ok(pending) => {
                let tx_hash = format!("{:?}", pending.tx_hash());
                match pending.await {
                    Ok(Some(receipt)) if receipt.status == Some(U64::from(1)) => {
                        receipt.contract_address.context("No contract address")?
                    }
                    _ => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("MEME deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    }
                }
            }
            Err(e) => {
                debug!("CreateMeme deploy submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit MEME deploy tx: {}", e),
                    tx_hash: None,
                });
            }
        };

        debug!("Deployed MEME token at {:?}", token_address);

        // Log to DB
        if let Some(db) = &ctx.db {
            let _ = db
                .log_asset_creation(
                    &format!("{:?}", address),
                    &format!("{:?}", token_address),
                    "MEME",
                    &name,
                    &symbol,
                )
                .await;
        }

        // 6. Mint (fire-and-forget)
        let contract = Contract::new(token_address, abi, Arc::new(provider.clone()));
        let mint_data = contract.encode("mint", (address, minted_amount))?;

        let mint_nonce = nonce_manager.next().await?;
        let mint_tx = TransactionRequest::new()
            .from(address)
            .to(token_address)
            .data(mint_data)
            .gas_price(gas_price)
            .gas(mint_gas_limit)
            .nonce(mint_nonce);

        let pending_mint = client.send_transaction(mint_tx, None).await;

        match pending_mint {
            Ok(pending) => {
                let minted_display = ethers::utils::format_units(minted_amount, 18)
                    .unwrap_or_else(|_| minted_amount.to_string());
                info!(
                    "Created Meme Token: {} ({}) at {:?} and minted {}",
                    name, symbol, token_address, minted_display
                );
                Ok(TaskResult {
                    success: true,
                    message: format!(
                        "Created {} ({}) at {:?}, mint {} submitted (tx: {:?})",
                        name, symbol, token_address, minted_display, pending.tx_hash()
                    ),
                    tx_hash: Some(format!("{:?}", pending.tx_hash())),
                })
            }
            Err(e) => {
                debug!("CreateMeme mint submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit MEME mint tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
