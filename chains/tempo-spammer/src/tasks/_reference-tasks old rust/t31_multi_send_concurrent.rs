use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_multiple_random_addresses;
use crate::utils::gas_manager::GasManager;
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;

pub struct MultiSendConcurrentTask;

#[async_trait]
impl TempoTask for MultiSendConcurrentTask {
    fn name(&self) -> &str {
        "31_multi_send_concurrent"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        let count = rand::thread_rng().gen_range(5..10);
        let recipients = get_multiple_random_addresses(count)?;

        println!("Executing {} Concurrent Transfers...", count);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        let mut futures = vec![];
        for recipient in recipients {
            let amount = U256::from(100);

            let tx = TransactionRequest::new()
                .to(recipient)
                .value(amount)
                .gas_price(bumped_gas_price)
                .from(ctx.wallet.address());

            // Sending them sequentially-async to avoid nonce collision in simple providers
            // In a real high-throughput scenario we'd use parallel nonces (Tempo specifics)
            futures.push(client.send_transaction(tx, None));
        }

        let mut success_count = 0;
        let mut last_hash = String::new();

        for pending in futures {
            match pending.await {
                Ok(p) => {
                    if let Ok(Some(receipt)) = p.await {
                        success_count += 1;
                        last_hash = format!("{:?}", receipt.transaction_hash);
                    }
                }
                Err(e) => println!("Tx failed: {:?}", e),
            }
        }

        Ok(TaskResult {
            success: success_count > 0,
            message: format!(
                "Completed {}/{} concurrent transfers.",
                success_count, count
            ),
            tx_hash: if last_hash.is_empty() {
                None
            } else {
                Some(last_hash)
            },
        })
    }
}
