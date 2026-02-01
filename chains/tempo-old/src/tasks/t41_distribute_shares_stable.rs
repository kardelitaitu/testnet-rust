use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::str::FromStr;

ethers::contract::abigen!(
    ISplitterStable,
    r#"[
        function distribute(address token)
    ]"#
);

ethers::contract::abigen!(
    IERC20SplitStable,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function balanceOf(address owner) view returns (uint256)
    ]"#
);

pub struct DistributeSharesStableTask;

#[async_trait]
impl TempoTask for DistributeSharesStableTask {
    fn name(&self) -> &str {
        "41_distribute_shares_stable"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let splitter_addr = Address::from_str("0x30c0000000000000000000000000000000000000")?;
        let token_addr = Address::from_str("0x20c0000000000000000000000000000000000001")?; // AlphaUSD

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let splitter = ISplitterStable::new(splitter_addr, client.clone());
        let token = IERC20SplitStable::new(token_addr, client.clone());

        println!("Distributing stable shares...");

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        let amount = U256::from(500);
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
            message: "Distributed stable shares.".to_string(),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
