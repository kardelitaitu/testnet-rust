use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_multiple_random_addresses;
use crate::utils::gas_manager::GasManager;
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IMemeMintBatch,
    r#"[
        function mint(address to, uint256 amount)
    ]"#
);

pub struct BatchMintMemeTask;

#[async_trait]
impl TempoTask for BatchMintMemeTask {
    fn name(&self) -> &str {
        "44_batch_mint_meme"
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
                message: "No memes found for batch minting.".to_string(),
                tx_hash: None,
            });
        }

        let token_addr = Address::from_str(&assets[0])?;
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let meme = IMemeMintBatch::new(token_addr, client.clone());

        let count = rand::thread_rng().gen_range(3..7);
        let recipients = get_multiple_random_addresses(count)?;

        println!(
            "Batch minting meme tokens {:?} to {} recipients...",
            token_addr, count
        );

        let mut last_hash = String::new();
        for recipient in recipients {
            let amount = U256::from(100);

            let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
            let bumped_gas_price = GasManager::bump_fees(gas_price);

            if let Ok(pending) = meme
                .mint(recipient, amount)
                .gas_price(bumped_gas_price)
                .send()
                .await
            {
                if let Ok(Some(receipt)) = pending.await {
                    last_hash = format!("{:?}", receipt.transaction_hash);
                }
            }
        }

        Ok(TaskResult {
            success: true,
            message: format!("Batch minted meme tokens to {} recipients.", count),
            tx_hash: if last_hash.is_empty() {
                None
            } else {
                Some(last_hash)
            },
        })
    }
}
