use crate::task::{Task, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;

pub struct CheckBalanceTask;

#[async_trait]
impl Task<TaskContext> for CheckBalanceTask {
    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let address = ctx.wallet.address();
        let provider = &ctx.provider;

        let balance = provider.get_balance(address, None).await?;
        let balance_eth = ethers::utils::format_units(balance, "ether")?;

        Ok(TaskResult {
            success: true,
            message: format!("Balance: {} ETH", balance_eth),
            tx_hash: None,
        })
    }

    fn name(&self) -> &str {
        "01_checkBalance"
    }
}
