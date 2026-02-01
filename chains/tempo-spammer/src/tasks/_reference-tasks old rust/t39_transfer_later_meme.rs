use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_random_address;
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IMemeLater,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function balanceOf(address owner) view returns (uint256)
    ]"#
);

pub struct TransferLaterMemeTask;

#[async_trait]
impl TempoTask for TransferLaterMemeTask {
    fn name(&self) -> &str {
        "39_transfer_later_meme"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet_addr = ctx.wallet.address();

        // Get Meme Asset
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
                message: "No memes found in DB for scheduled transfer.".to_string(),
                tx_hash: None,
            });
        }

        let asset_addr_str = assets[0].clone();
        let token_address = Address::from_str(&asset_addr_str)?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let meme = IMemeLater::new(token_address, client.clone());

        let delay = rand::thread_rng().gen_range(2..5);
        let recipient = get_random_address()?;
        let amount = U256::from(10);

        println!("Scheduling meme transfer in {} seconds...", delay);

        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        let tx = meme.transfer(recipient, amount).gas_price(bumped_gas_price);
        let pending = tx.send().await?;
        let receipt = pending.await?.context("Transfer failed")?;

        Ok(TaskResult {
            success: true,
            message: format!("Executed scheduled meme transfer (waited {}s).", delay),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
