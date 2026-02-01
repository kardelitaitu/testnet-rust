use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use std::sync::Arc;
use tracing::info;

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

        // 2. Prepare Deployment Transaction
        let abi: abi::Abi = serde_json::from_str(MEME_TOKEN_ABI)?;
        let bytecode_vector = ethers::utils::hex::decode(MEME_TOKEN_BYTECODE)?;
        let bytecode = Bytes::from(bytecode_vector);

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        // Balance check
        let balance = provider.get_balance(address, None).await?;
        let required = gas_limit * max_fee;

        if balance < required {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient funds: need {} Wei, have {} Wei",
                    required, balance
                ),
                tx_hash: None,
            });
        }

        // Encode constructor arguments
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

        let tx = Eip1559TransactionRequest::new()
            .from(address)
            .data(Bytes::from(input))
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .gas(gas_limit);

        // 3. Send Transaction
        let client = Arc::new(ethers::middleware::SignerMiddleware::new(
            provider.clone(),
            wallet.clone(),
        ));
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        if receipt.status != Some(1.into()) {
            return Ok(TaskResult {
                success: false,
                message: format!("Deployment failed with status {:?}", receipt.status),
                tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
            });
        }

        let token_address = receipt
            .contract_address
            .context("No contract address in receipt")?;

        // 4. Log to Database
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

        info!(
            "Created Meme Token: {} ({}) at {:?}",
            name, symbol, token_address
        );

        Ok(TaskResult {
            success: true,
            message: format!("Created {} ({}) at {:?}", name, symbol, token_address),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
