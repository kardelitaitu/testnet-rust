use crate::contracts::MEME_TOKEN_ABI;
use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct BatchApproveTask;

impl BatchApproveTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for BatchApproveTask {
    fn name(&self) -> &str {
        "33_batchApprove"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();
        let wallet_str = format!("{:?}", address);

        // 1. Get meme tokens from DB
        let db = ctx.db.as_ref().context("Database not initialized")?;
        let token_addresses = db.get_all_assets_by_type("MEME").await?;

        if token_addresses.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "No MEME contracts found in DB for {}. Run createMeme first.",
                    wallet_str
                ),
                tx_hash: None,
            });
        }

        // 2. Get random spender from address cache
        let spender = AddressCache::get_random().context("Failed to get random address")?;

        let amount: u128 = 1_000_000_000_000_000_000_000_000_000u128; // effectively unlimited (1e27)
        let amount_formatted = ethers::utils::format_units(amount, 18u32).unwrap_or_else(|_| amount.to_string());

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;
        let max_tokens = token_addresses.len().min(5); // cap at 5 to avoid excessive gas
        let estimated_gas = (gas_limit * max_tokens as u64) * gas_price;

        // 3. Balance check
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

        // 4. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(Arc::new(provider.clone()), address);

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let mut tx_hashes = Vec::new();
        let mut successes = 0;

        for (i, token_addr_str) in token_addresses.iter().take(max_tokens).enumerate() {
            let token_address: Address = token_addr_str
                .parse()
                .context(format!("Invalid token address in DB: {}", token_addr_str))?;

            let abi: abi::Abi = serde_json::from_str(MEME_TOKEN_ABI)?;
            let contract = Contract::new(token_address, abi, Arc::new(provider.clone()));
            let data = contract.encode("approve", (spender, amount))?;

            let nonce = nonce_manager.next().await?;
            let tx = TransactionRequest::new()
                .to(token_address)
                .data(data)
                .gas(gas_limit)
                .gas_price(gas_price)
                .nonce(nonce)
                .from(address);

            let pending_tx = client.send_transaction(tx, None).await;

            match pending_tx {
                Ok(pending) => {
                    tx_hashes.push(format!("{:?}", pending.tx_hash()));
                    successes += 1;
                    debug!(
                        "BatchApprove {}/{} submitted for {:?}: {:?}",
                        i + 1,
                        max_tokens,
                        token_address,
                        pending.tx_hash()
                    );
                },
                Err(e) => {
                    debug!("BatchApprove token {} submit failed: {}", i + 1, e);
                    tx_hashes.push("failed".to_string());
                    let _ = nonce_manager.resync().await;
                },
            }
        }

        let success_threshold = max_tokens; // ALL must succeed

        Ok(TaskResult {
            success: successes == max_tokens && successes > 0,
            message: format!(
                "Batch approved {} MEME tokens for {:?} ({}/{} submitted, need {} for success)",
                amount_formatted, spender, successes, max_tokens, success_threshold
            ),
            tx_hash: Some(tx_hashes.join(",")),
        })
    }
}
