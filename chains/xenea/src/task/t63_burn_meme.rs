use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use std::sync::Arc;

use crate::contracts::MEME_TOKEN_ABI;
use crate::task::{Task, TaskContext, TaskResult};
use tracing::debug;

pub struct BurnMemeTask;

#[async_trait]
impl Task<TaskContext> for BurnMemeTask {
    fn name(&self) -> &str {
        "63_burnMeme"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // 1. Get random MEME token from DB
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

        // 2. Check balance
        let abi: abi::Abi = serde_json::from_str(MEME_TOKEN_ABI)?;
        let contract = Contract::new(token_address, abi.clone(), Arc::new(provider.clone()));

        let balance: U256 = contract
            .method::<_, U256>("balanceOf", address)?
            .call()
            .await
            .context("Failed to get balance")?;

        if balance.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!("No balance of MEME token at {:?}", token_address),
                tx_hash: None,
            });
        }

        // 3. Calculate 1% to burn
        let burn_amount = balance / U256::from(100u64);
        if burn_amount.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: "Balance too low to burn 1%".to_string(),
                tx_hash: None,
            });
        }

        // 4. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(Arc::new(provider.clone()), address);
        let nonce = nonce_manager.next().await?;

        // 4b. Check native balance for gas
        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;
        let estimated_gas = gas_limit * gas_price;

        let native_balance = provider.get_balance(address, None).await?;
        if native_balance < estimated_gas {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient TXENE for gas: need {} Wei, have {} Wei",
                    estimated_gas, native_balance
                ),
                tx_hash: None,
            });
        }

        // 5. Burn (fire-and-forget)

        let data = match contract.encode("burn", (burn_amount,)) {
            Ok(d) => d,
            Err(_e) => {
                return Ok(TaskResult {
                    success: false,
                    message: format!(
                        "MEME token at {:?} does not support burn(). Run createMeme to deploy a new token with burn support.",
                        token_address
                    ),
                    tx_hash: None,
                });
            },
        };

        let tx = TransactionRequest::new()
            .to(token_address)
            .data(data)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .from(address);

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => {
                let burn_display =
                    ethers::utils::format_units(burn_amount, 18).unwrap_or_else(|_| burn_amount.to_string());
                Ok(TaskResult {
                    success: true,
                    message: format!(
                        "Burned {} MEME from {:?} (tx: {:?})",
                        burn_display,
                        token_address,
                        pending.tx_hash()
                    ),
                    tx_hash: Some(format!("{:?}", pending.tx_hash())),
                })
            },
            Err(e) => {
                debug!("BurnMeme tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit MEME burn tx: {}", e),
                    tx_hash: None,
                })
            },
        }
    }
}
