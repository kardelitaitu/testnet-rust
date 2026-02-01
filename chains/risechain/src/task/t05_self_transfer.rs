use crate::task::{Task, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;

pub struct SelfTransferTask;

#[async_trait]
impl Task<TaskContext> for SelfTransferTask {
    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        // Send 0 ETH to self
        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_TRANSFER;

        let tx = Eip1559TransactionRequest::new()
            .to(ctx.wallet.address())
            .value(0)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(ctx.wallet.address());

        use ethers::middleware::SignerMiddleware;
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await?;

        let receipt = pending_tx.await?;

        match receipt {
            Some(r) => Ok(TaskResult {
                success: r.status == Some(U64::from(1)),
                message: "Self-transfer 0 ETH".into(),
                tx_hash: Some(format!("{:?}", r.transaction_hash)),
            }),
            None => Ok(TaskResult {
                success: false,
                message: "Transaction dropped".into(),
                tx_hash: None,
            }),
        }
    }

    fn name(&self) -> &str {
        "05_selfTransfer"
    }
}
