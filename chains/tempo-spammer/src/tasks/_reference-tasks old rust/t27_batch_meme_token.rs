use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_multiple_random_addresses;
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IMemeBatchTransfer,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function balanceOf(address owner) view returns (uint256)
    ]"#
);

pub struct BatchMemeTokenTask;

#[async_trait]
impl TempoTask for BatchMemeTokenTask {
    fn name(&self) -> &str {
        "27_batch_meme_token"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet_addr = ctx.wallet.address();

        // 1. Get Meme Asset from DB
        let assets = if let Some(db) = ctx.db.as_ref() {
            db.get_assets_by_type(&format!("{:?}", wallet_addr), "meme")
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };

        if assets.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No memes found in DB for batch transfer.".to_string(),
                tx_hash: None,
            });
        }

        let asset_addr_str = assets[0].clone();
        let token_address = Address::from_str(&asset_addr_str)?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let meme = IMemeBatchTransfer::new(token_address, client.clone());

        println!(
            "Executing Batch Meme Token Transfers for {:?}...",
            token_address
        );

        let count = rand::thread_rng().gen_range(2..5);
        let recipients = get_multiple_random_addresses(count)?;
        let mut last_hash = String::new();

        for recipient in recipients {
            let amount = U256::from(10);

            let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
            let bumped_gas_price = GasManager::bump_fees(gas_price);

            let tx = meme.transfer(recipient, amount).gas_price(bumped_gas_price);
            let pending = tx.send().await?;
            let receipt = pending.await?.context("Transfer failed")?;
            last_hash = format!("{:?}", receipt.transaction_hash);
        }

        Ok(TaskResult {
            success: true,
            message: format!("Executed batch of {} meme token transfers.", count),
            tx_hash: Some(last_hash),
        })
    }
}
