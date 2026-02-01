use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_multiple_random_addresses;
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;

pub struct BatchSystemTokenTask;

#[async_trait]
impl TempoTask for BatchSystemTokenTask {
    fn name(&self) -> &str {
        "25_batch_system_token"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        println!("Executing Batch System Token Transfers...");

        let count = rand::thread_rng().gen_range(2..5);
        let recipients = get_multiple_random_addresses(count)?;
        let mut last_hash = String::new();

        for recipient in recipients {
            let amount = U256::from(100); // 100 Wei or smallest unit

            let tx = TransactionRequest::new()
                .to(recipient)
                .value(amount)
                .gas_price(bumped_gas_price)
                .from(ctx.wallet.address());

            let pending = client.send_transaction(tx, None).await?;
            let receipt = pending.await?.context("Transfer failed")?;
            last_hash = format!("{:?}", receipt.transaction_hash);
        }

        Ok(TaskResult {
            success: true,
            message: format!("Executed batch of {} system token transfers.", count),
            tx_hash: Some(last_hash),
        })
    }
}
