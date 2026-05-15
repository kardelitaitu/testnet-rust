use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::{Rng, seq::SliceRandom};
use std::sync::Arc;

use crate::contracts::MEME_TOKEN_ABI;
use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use tracing::debug;

pub struct BatchSendCreatedMemeTask;

#[async_trait]
impl Task<TaskContext> for BatchSendCreatedMemeTask {
    fn name(&self) -> &str {
        "62_batchSendCreatedMeme"
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

        // 3. Pick 5-10 random recipients
        let mut rng = OsRng;
        let recipient_count = rng.gen_range(5..=10);
        let all_recipients = AddressCache::all().context("Failed to get address cache")?;
        let recipients: Vec<Address> = all_recipients
            .choose_multiple(&mut rng, recipient_count)
            .cloned()
            .collect();

        // 4. Calculate 1% per recipient
        let amount_per = balance / U256::from(100u64);
        if amount_per.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: "Balance too low to send 1% per recipient".to_string(),
                tx_hash: None,
            });
        }

        let total_needed = amount_per * U256::from(recipients.len() as u64);
        if total_needed > balance {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient balance: need {} for {} recipients, have {}",
                    total_needed, recipients.len(), balance
                ),
                tx_hash: None,
            });
        }

        // 5. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;

        // 5b. Check native balance for gas
        let native_balance = provider.get_balance(address, None).await?;
        let estimated_gas = gas_limit * gas_price * U256::from(recipients.len() as u64);
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

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());

        // 6. Send transfers (fire-and-forget)
        let mut tx_hashes = Vec::new();
        let mut failed = 0;

        for recipient in &recipients {
            let nonce = match nonce_manager.next().await {
                Ok(n) => n,
                Err(e) => {
                    debug!("BatchSendCreatedMeme nonce error: {}", e);
                    let _ = nonce_manager.resync().await;
                    failed += 1;
                    continue;
                }
            };

            let data = contract.encode("transfer", (*recipient, amount_per))?;

            let tx = TransactionRequest::new()
                .to(token_address)
                .data(data)
                .gas(gas_limit)
                .gas_price(gas_price)
                .nonce(nonce)
                .from(address);

            match client.send_transaction(tx, None).await {
                Ok(pending) => {
                    tx_hashes.push(format!("{:?}", pending.tx_hash()));
                    debug!("BatchSendCreatedMeme transfer submitted: {:?}", pending.tx_hash());
                }
                Err(e) => {
                    debug!("BatchSendCreatedMeme transfer submit failed: {}", e);
                    let _ = nonce_manager.resync().await;
                    failed += 1;
                }
            }
        }

        let amount_display = ethers::utils::format_units(amount_per, 18)
            .unwrap_or_else(|_| amount_per.to_string());

        if tx_hashes.is_empty() {
            Ok(TaskResult {
                success: false,
                message: format!("All {} transfers failed to submit", recipients.len()),
                tx_hash: None,
            })
        } else {
            Ok(TaskResult {
                success: true,
                message: format!(
                    "Batch sent {} MEME to {} recipients ({}/{} submitted, {} failed)",
                    amount_display,
                    recipients.len(),
                    tx_hashes.len(),
                    recipients.len(),
                    failed
                ),
                tx_hash: Some(tx_hashes.join(",")),
            })
        }
    }
}
