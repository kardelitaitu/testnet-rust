use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::str::FromStr;

ethers::contract::abigen!(
    ISplitter,
    r#"[
        function distribute(address token)
    ]"#
);

ethers::contract::abigen!(
    IERC20Split,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function balanceOf(address owner) view returns (uint256)
    ]"#
);

pub struct DistributeSharesTask;

#[async_trait]
impl TempoTask for DistributeSharesTask {
    fn name(&self) -> &str {
        "40_distribute_shares"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let splitter_addr = Address::from_str("0x30c0000000000000000000000000000000000000")?; // Mock Splitter
        let token_addr = Address::from_str("0x20c0000000000000000000000000000000000000")?; // PathUSD

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let splitter = ISplitter::new(splitter_addr, client.clone());
        let token = IERC20Split::new(token_addr, client.clone());

        println!(
            "Distributing shares via Splitter {:?} for token {:?}...",
            splitter_addr, token_addr
        );

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // 1. Fund Splitter
        let amount = U256::from(1000);
        token
            .transfer(splitter_addr, amount)
            .gas_price(bumped_gas_price)
            .send()
            .await?
            .await?;

        // 2. Distribute
        let tx = splitter.distribute(token_addr).gas_price(bumped_gas_price);
        let pending = tx.send().await?;
        let receipt = pending.await?.context("Distribution failed")?;

        Ok(TaskResult {
            success: true,
            message: format!("Distributed shares. Tx: {:?}", receipt.transaction_hash),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
