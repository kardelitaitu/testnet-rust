use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::str::FromStr;

ethers::contract::abigen!(
    ISplitterMeme,
    r#"[
        function distribute(address token)
    ]"#
);

ethers::contract::abigen!(
    IERC20SplitMeme,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function balanceOf(address owner) view returns (uint256)
    ]"#
);

pub struct DistributeSharesMemeTask;

#[async_trait]
impl TempoTask for DistributeSharesMemeTask {
    fn name(&self) -> &str {
        "42_distribute_shares_meme"
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
                message: "No memes found for share distribution.".to_string(),
                tx_hash: None,
            });
        }

        let token_addr = Address::from_str(&assets[0])?;
        let splitter_addr = Address::from_str("0x30c0000000000000000000000000000000000000")?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let splitter = ISplitterMeme::new(splitter_addr, client.clone());
        let token = IERC20SplitMeme::new(token_addr, client.clone());

        println!("Distributing meme shares for {:?}...", token_addr);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        let amount = U256::from(10);
        token
            .transfer(splitter_addr, amount)
            .gas_price(bumped_gas_price)
            .send()
            .await?
            .await?;

        let tx = splitter.distribute(token_addr).gas_price(bumped_gas_price);
        let receipt = tx.send().await?.await?.context("Distribution failed")?;

        Ok(TaskResult {
            success: true,
            message: "Distributed meme shares.".to_string(),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
