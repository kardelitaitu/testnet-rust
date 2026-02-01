use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_multiple_random_addresses;
use crate::utils::gas_manager::GasManager;
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IMemeConcurrent,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function balanceOf(address owner) view returns (uint256)
    ]"#
);

pub struct MultiSendConcurrentMemeTask;

#[async_trait]
impl TempoTask for MultiSendConcurrentMemeTask {
    fn name(&self) -> &str {
        "33_multi_send_concurrent_meme"
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
                message: "No memes found in DB for concurrent transfer.".to_string(),
                tx_hash: None,
            });
        }

        let asset_addr_str = assets[0].clone();
        let token_address = Address::from_str(&asset_addr_str)?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let meme = IMemeConcurrent::new(token_address, client.clone());

        let count = rand::thread_rng().gen_range(5..10);
        let recipients = get_multiple_random_addresses(count)?;

        println!(
            "Executing {} Concurrent Meme Transfers for {:?}...",
            count, token_address
        );

        let mut last_hash = String::new();
        let mut success_count = 0;

        for recipient in recipients {
            let amount = U256::from(10);

            let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
            let bumped_gas_price = GasManager::bump_fees(gas_price);

            if let Ok(pending) = meme
                .transfer(recipient, amount)
                .gas_price(bumped_gas_price)
                .send()
                .await
            {
                if let Ok(Some(receipt)) = pending.await {
                    success_count += 1;
                    last_hash = format!("{:?}", receipt.transaction_hash);
                }
            }
        }

        Ok(TaskResult {
            success: success_count > 0,
            message: format!(
                "Completed {}/{} concurrent meme transfers.",
                success_count, count
            ),
            tx_hash: if last_hash.is_empty() {
                None
            } else {
                Some(last_hash)
            },
        })
    }
}
