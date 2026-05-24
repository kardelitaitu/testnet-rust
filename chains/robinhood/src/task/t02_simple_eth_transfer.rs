use crate::task::{Task, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use tracing::info;

pub struct SimpleEthTransferTask;

#[async_trait]
impl Task<TaskContext> for SimpleEthTransferTask {
    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let address = ctx.wallet.address();
        let provider = &ctx.provider;

        let to = address;
        let amount = parse_ether(0.0001)?;

        info!("Sending 0.0001 ETH from {} to {}", address, to);

        let tx = TransactionRequest::new().to(to).value(amount).from(address);

        let pending_tx = provider.send_transaction(tx, None).await?;
        let tx_hash = pending_tx.tx_hash();

        Ok(TaskResult {
            success: true,
            message: "Transferred 0.0001 ETH to self".to_string(),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }

    fn name(&self) -> &str {
        "02_simpleEthTransfer"
    }
}

fn parse_ether(amount: f64) -> Result<U256> {
    let amount_str = format!("{:.18}", amount);
    Ok(ethers::utils::parse_ether(amount_str)?)
}
