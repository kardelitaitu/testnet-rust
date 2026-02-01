use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_multiple_random_addresses;
use crate::utils::gas_manager::GasManager;
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;

pub struct BatchSendTransactionTask;

#[async_trait]
impl TempoTask for BatchSendTransactionTask {
    fn name(&self) -> &str {
        "34_batch_send_transaction"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        let count = rand::thread_rng().gen_range(5..10);
        let recipients = get_multiple_random_addresses(count)?;

        println!("Executing Batch of {} System Transfers...", count);

        let mut success_count = 0;
        let mut last_hash = String::new();

        for recipient in recipients {
            let amount = U256::from(100);

            let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
            let bumped_gas_price = GasManager::bump_fees(gas_price);

            let tx = TransactionRequest::new()
                .to(recipient)
                .value(amount)
                .gas_price(bumped_gas_price)
                .from(ctx.wallet.address());

            if let Ok(pending) = client.send_transaction(tx, None).await {
                if let Ok(Some(receipt)) = pending.await {
                    success_count += 1;
                    last_hash = format!("{:?}", receipt.transaction_hash);
                }
            }
        }

        Ok(TaskResult {
            success: success_count > 0,
            message: format!("Completed batch of {}/{} transfers.", success_count, count),
            tx_hash: if last_hash.is_empty() {
                None
            } else {
                Some(last_hash)
            },
        })
    }
}
