use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::str::FromStr;

ethers::contract::abigen!(
    IViralFaucet,
    r#"[
        function claim(address token, uint256 amount)
        function getBalance(address token) view returns (uint256)
        function fund(address token, uint256 amount)
    ]"#
);

pub struct ClaimViralFaucetTask;

#[async_trait]
impl TempoTask for ClaimViralFaucetTask {
    fn name(&self) -> &str {
        "46_claim_viral_faucet"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet_addr = ctx.wallet.address();

        let assets = if let Some(db) = ctx.db.as_ref() {
            db.get_assets_by_type(&format!("{:?}", wallet_addr), "viral_faucet")
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };

        if assets.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No viral faucets found in DB to claim from.".to_string(),
                tx_hash: None,
            });
        }

        let contract_addr = Address::from_str(&assets[0])?;
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let faucet = IViralFaucet::new(contract_addr, client.clone());

        let path_usd = Address::from_str("0x20c0000000000000000000000000000000000000")?;
        let claim_amount = 1000000u128;

        println!("Claiming from Viral Faucet at {:?}...", contract_addr);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        let tx = faucet
            .claim(path_usd, U256::from(claim_amount))
            .gas_price(bumped_gas_price);
        let receipt = tx.send().await?.await?.context("Claim failed")?;

        Ok(TaskResult {
            success: true,
            message: "Claimed from Viral Faucet.".to_string(),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
