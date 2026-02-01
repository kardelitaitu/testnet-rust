use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_random_address;
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IERC20Later,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function balanceOf(address owner) view returns (uint256)
    ]"#
);

pub struct TransferLaterStableTask;

#[async_trait]
impl TempoTask for TransferLaterStableTask {
    fn name(&self) -> &str {
        "38_transfer_later_stable"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let stable_addr = Address::from_str("0x20c0000000000000000000000000000000000000")?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let token = IERC20Later::new(stable_addr, client.clone());

        let delay = rand::thread_rng().gen_range(2..5);
        let recipient = get_random_address()?;
        let amount = U256::from(100);

        println!(
            "Scheduling stable transfer to {:?} in {} seconds...",
            recipient, delay
        );

        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        let tx = token
            .transfer(recipient, amount)
            .gas_price(bumped_gas_price);
        let pending = tx.send().await?;
        let receipt = pending.await?.context("Transfer failed")?;

        Ok(TaskResult {
            success: true,
            message: format!("Executed scheduled stable transfer (waited {}s).", delay),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
