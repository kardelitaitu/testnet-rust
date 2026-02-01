use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use std::fs;
use std::sync::Arc;

use crate::contracts::MEME_TOKEN_ABI;
use crate::task::{Task, TaskContext, TaskResult};

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

        // 1. Pick Random Recipient from address.txt
        let recipients = fs::read_to_string("address.txt").context("Failed to read address.txt")?;
        let recipient_list: Vec<&str> = recipients
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();
        let recipient_str = recipient_list
            .choose(&mut OsRng)
            .context("address.txt is empty")?;
        let recipient: Address = recipient_str
            .trim()
            .parse()
            .context(format!("Invalid address in address.txt: {}", recipient_str))?;

        // 2. Get Meme Tokens from DB
        let db = ctx.db.as_ref().context("Database not initialized")?;
        let token_addresses = db.get_assets_by_type(&wallet_str, "MEME").await?;

        if token_addresses.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "No meme tokens found in DB for wallet {}. Run createMeme first.",
                    wallet_str
                ),
                tx_hash: None,
            });
        }

        let token_addr_str = token_addresses.choose(&mut OsRng).unwrap();
        let token_address: Address = token_addr_str
            .parse()
            .context(format!("Invalid token address in DB: {}", token_addr_str))?;

        // 3. Setup Contract
        let abi: abi::Abi = serde_json::from_str(MEME_TOKEN_ABI)?;
        let contract = Contract::new(token_address, abi, Arc::new(ctx.provider.clone()));

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

        // 5. Calculate 1%
        let amount = balance / 100;
        if amount.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!("Balance too low to send 1% (balance: {})", balance),
                tx_hash: None,
            });
        }

        // 6. Transfer
        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;

        let data = contract.encode("transfer", (recipient, amount))?;

        let tx = Eip1559TransactionRequest::new()
            .to(token_address)
            .data(data)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .gas(gas_limit);

        let client = Arc::new(ethers::middleware::SignerMiddleware::new(
            provider.clone(),
            wallet.clone(),
        ));
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        Ok(TaskResult {
            success: true,
            message: format!(
                "Sent {} tokens to {:?} from {:?} (1% of balance)",
                ethers::utils::format_units(amount, 18).unwrap_or_else(|_| amount.to_string()),
                recipient,
                token_address
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
