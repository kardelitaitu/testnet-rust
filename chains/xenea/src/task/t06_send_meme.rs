use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;

use crate::contracts::MEME_TOKEN_ABI;
use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use tracing::debug;

pub struct SendMemeTokenTask;

#[async_trait]
impl Task<TaskContext> for SendMemeTokenTask {
    fn name(&self) -> &str {
        "06_sendMemeToken"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();
        let wallet_str = format!("{:?}", address);

        // 1. Pick Random Recipient from address cache
        let recipient = AddressCache::get_random().context("Failed to get random address")?;

        // 2. Get Meme Tokens from DB
        let db = ctx.db.as_ref().context("Database not initialized")?;
        let token_addr_str = db
            .get_latest_asset_by_type(&wallet_str, "MEME")
            .await?
            .context(format!(
                "No meme tokens found in DB for wallet {}. Run createMeme first.",
                wallet_str
            ))?;

        if token_addr_str.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "No meme tokens found in DB for wallet {}. Run createMeme first.",
                    wallet_str
                ),
                tx_hash: None,
            });
        }

        let token_address: Address = token_addr_str
            .parse()
            .context(format!("Invalid token address in DB: {}", token_addr_str))?;

        // 3. Setup Contract
        let abi: abi::Abi = serde_json::from_str(MEME_TOKEN_ABI)?;
        let contract = Contract::new(token_address, abi, Arc::new(provider.clone()));

        // 4. Fetch Balance
        let balance: U256 = contract
            .method::<_, U256>("balanceOf", address)?
            .call()
            .await
            .context("Contract call 'balanceOf' failed")?;

        if balance.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!("Wallet has 0 balance of token at {:?}", token_address),
                tx_hash: None,
            });
        }

        // 5. Calculate a random 0.50% to 1.50%
        let mut rng = OsRng;
        let pct_basis = rng.gen_range(50..=150);
        let amount = balance * U256::from(pct_basis) / U256::from(10_000u64);
        if amount.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Balance too low to send {}.% (balance: {})",
                    pct_basis as f64 / 100.0,
                    balance
                ),
                tx_hash: None,
            });
        }

        // 6. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(Arc::new(provider.clone()), address);
        let nonce = nonce_manager.next().await?;

        // 6b. Check native balance for gas
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

        // 7. Transfer (fire-and-forget)

        let data = contract.encode("transfer", (recipient, amount))?;

        let tx = TransactionRequest::new()
            .to(token_address)
            .data(data)
            .gas_price(gas_price)
            .gas(gas_limit)
            .nonce(nonce)
            .from(address);

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!(
                    "Sent {} tokens to {:?} from {:?} ({:.2}% of balance) (tx: {:?})",
                    ethers::utils::format_units(amount, 18).unwrap_or_else(|_| amount.to_string()),
                    recipient,
                    token_address,
                    pct_basis as f64 / 100.0,
                    pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("SendMemeToken tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit MEME transfer tx: {}", e),
                    tx_hash: None,
                })
            },
        }
    }
}
