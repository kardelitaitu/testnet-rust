use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::{seq::SliceRandom, Rng};
use std::sync::Arc;

use crate::contracts::MEME_TOKEN_ABI;
use crate::task::{Task, TaskContext, TaskResult};
use tracing::debug;

pub struct MintMemeTask;

#[async_trait]
impl Task<TaskContext> for MintMemeTask {
    fn name(&self) -> &str {
        "61_mintMeme"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let db = ctx.db.as_ref().context("Database not initialized")?;
        let token_addresses = db.get_all_assets_by_type("MEME").await?;
        if token_addresses.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No MEME contracts found in DB. Run createMeme first.".to_string(),
                tx_hash: None,
            });
        }

        let token_addr_str = token_addresses.choose(&mut OsRng).unwrap();
        let token_address: Address = token_addr_str
            .parse()
            .context(format!("Invalid token address in DB: {}", token_addr_str))?;

        let mut rng = OsRng;
        let amount_whole = rng.gen_range(1..=100) * 1_000_000u64;
        let amount: U256 = ethers::utils::parse_units(amount_whole.to_string(), 18)?.into();

        let abi: abi::Abi = serde_json::from_str(MEME_TOKEN_ABI)?;
        let contract = Contract::new(token_address, abi, Arc::new(provider.clone()));
        let data = contract.encode("mint", (address, amount))?;

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;
        let estimated_gas = gas_limit * gas_price;

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
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(Arc::new(provider.clone()), address);
        let nonce = nonce_manager.next().await?;

        let tx = TransactionRequest::new()
            .to(token_address)
            .data(data)
            .gas_price(gas_price)
            .gas(gas_limit)
            .nonce(nonce)
            .from(address);

        // 3. Send (fire-and-forget)
        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => {
                let amount_display = ethers::utils::format_units(amount, 18).unwrap_or_else(|_| amount.to_string());
                Ok(TaskResult {
                    success: true,
                    message: format!(
                        "Minted {} MEME on {:?} to {:?} (tx: {:?})",
                        amount_display,
                        token_address,
                        address,
                        pending.tx_hash()
                    ),
                    tx_hash: Some(format!("{:?}", pending.tx_hash())),
                })
            },
            Err(e) => {
                debug!("MintMeme tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit MEME mint tx: {}", e),
                    tx_hash: None,
                })
            },
        }
    }
}
