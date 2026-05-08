use super::{DaChainTask, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;

pub struct DaChainCheckBalanceTask;

#[async_trait]
impl DaChainTask for DaChainCheckBalanceTask {
    fn name(&self) -> &str {
        "01_checkBalance"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let address = ctx.wallet.address();
        let balance = ctx.provider.get_balance(address, None).await?;
        
        Ok(TaskResult {
            success: true,
            message: format!("Balance: {} DACC", ethers::utils::format_ether(balance)),
        })
    }
}
