use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_multiple_random_addresses;
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IMulticallMeme,
    r#"[
        struct CallMeme { address target; bytes callData; }
        function aggregate(CallMeme[] calls) external payable returns (uint256 blockNumber, bytes[] returnData)
    ]"#
);

ethers::contract::abigen!(
    IERC20MemeDisp,
    r#"[
        function approve(address spender, uint256 amount) returns (bool)
        function transferFrom(address from, address to, uint256 amount) returns (bool)
    ]"#
);

pub struct MultiSendDisperseMemeTask;

#[async_trait]
impl TempoTask for MultiSendDisperseMemeTask {
    fn name(&self) -> &str {
        "30_multi_send_disperse_meme"
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
                message: "No memes found in DB for dispersion.".to_string(),
                tx_hash: None,
            });
        }

        let asset_addr_str = assets[0].clone();
        let token_address = Address::from_str(&asset_addr_str)?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        let multicall_addr = Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11")?;
        let multicall = IMulticallMeme::new(multicall_addr, client.clone());
        let token = IERC20MemeDisp::new(token_address, client.clone());

        println!("Dispersing Meme Token {:?} to recipients...", token_address);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // Approve
        token
            .approve(multicall_addr, U256::max_value())
            .gas_price(bumped_gas_price)
            .send()
            .await?
            .await?;

        // Prepare Calls
        let target_count = rand::thread_rng().gen_range(5..10);
        let recipients = get_multiple_random_addresses(target_count)?;
        let mut calls = vec![];
        for recipient in recipients {
            let amount = U256::from(10);
            let data = token.encode("transferFrom", (wallet_addr, recipient, amount))?;
            calls.push(CallMeme {
                target: token_address,
                call_data: data,
            });
        }

        // Execute
        let tx = multicall.aggregate(calls).gas_price(bumped_gas_price);
        let pending = tx.send().await?;
        let receipt = pending.await?.context("Multicall failed")?;

        Ok(TaskResult {
            success: true,
            message: format!("Dispersed meme tokens to {} recipients.", target_count),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
