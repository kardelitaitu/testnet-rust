use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_random_address;
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IMemeTransfer,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function mint(address to, uint256 amount)
        function balanceOf(address owner) view returns (uint256)
        function decimals() view returns (uint8)
        function symbol() view returns (string)
    ]"#
);

pub struct TransferMemeTask;

#[async_trait]
impl TempoTask for TransferMemeTask {
    fn name(&self) -> &str {
        "23_transfer_meme"
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
                message: "No created meme tokens found in DB to transfer.".to_string(),
                tx_hash: None,
            });
        }

        let asset_addr_str = { assets[rand::thread_rng().gen_range(0..assets.len())].clone() };
        let token_address = Address::from_str(&asset_addr_str)?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let meme = IMemeTransfer::new(token_address, client.clone());

        let symbol = meme
            .symbol()
            .call()
            .await
            .unwrap_or_else(|_| "???".to_string());
        let decimals = meme.decimals().call().await.unwrap_or(6);
        let amount_base = rand::thread_rng().gen_range(1..10);
        let amount_wei = U256::from(amount_base) * U256::exp10(decimals as usize);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // 2. Ensuring Balance
        let balance = meme
            .balance_of(wallet_addr)
            .call()
            .await
            .unwrap_or_default();
        if balance < amount_wei {
            println!("Low balance for {}. Minting more...", symbol);
            meme.mint(wallet_addr, amount_wei)
                .gas_price(bumped_gas_price)
                .send()
                .await?
                .await?;
        }

        // 3. Transfer
        let recipient = get_random_address()?;
        println!(
            "Transferring {} {} to {:?}...",
            amount_base, symbol, recipient
        );

        let tx = meme
            .transfer(recipient, amount_wei)
            .gas_price(bumped_gas_price);
        let pending = tx.send().await?;
        let receipt = pending.await?.context("Transfer failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!(
                "Transferred {} {} to {:?}. Tx: {}",
                amount_base, symbol, recipient, hash
            ),
            tx_hash: Some(hash),
        })
    }
}
