use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_multiple_random_addresses;
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IERC20Batch,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function balanceOf(address owner) view returns (uint256)
    ]"#
);

pub struct BatchStableTokenTask;

#[async_trait]
impl TempoTask for BatchStableTokenTask {
    fn name(&self) -> &str {
        "26_batch_stable_token"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let stable_addr = Address::from_str("0x20c0000000000000000000000000000000000000")?; // PathUSD

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let token = IERC20Batch::new(stable_addr, client.clone());

        println!("Executing Batch Stable Token Transfers...");

        let count = rand::thread_rng().gen_range(2..5);
        let recipients = get_multiple_random_addresses(count)?;
        let mut last_hash = String::new();

        for recipient in recipients {
            let amount = U256::from(100);

            let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
            let bumped_gas_price = GasManager::bump_fees(gas_price);

            let tx = token
                .transfer(recipient, amount)
                .gas_price(bumped_gas_price);
            let pending = tx.send().await?;
            let receipt = pending.await?.context("Transfer failed")?;
            last_hash = format!("{:?}", receipt.transaction_hash);
        }

        Ok(TaskResult {
            success: true,
            message: format!("Executed batch of {} stable token transfers.", count),
            tx_hash: Some(last_hash),
        })
    }
}
