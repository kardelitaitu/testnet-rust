use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_multiple_random_addresses;
use crate::utils::gas_manager::GasManager;
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IERC20BatchSend,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function balanceOf(address owner) view returns (uint256)
    ]"#
);

pub struct BatchSendTransactionStableTask;

#[async_trait]
impl TempoTask for BatchSendTransactionStableTask {
    fn name(&self) -> &str {
        "35_batch_send_transaction_stable"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let stable_addr = Address::from_str("0x20c0000000000000000000000000000000000000")?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let token = IERC20BatchSend::new(stable_addr, client.clone());

        let count = rand::thread_rng().gen_range(5..10);
        let recipients = get_multiple_random_addresses(count)?;

        println!("Executing Batch of {} Stable Transfers...", count);

        let mut success_count = 0;
        let mut last_hash = String::new();

        for recipient in recipients {
            let amount = U256::from(100);

            let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
            let bumped_gas_price = GasManager::bump_fees(gas_price);

            if let Ok(pending) = token
                .transfer(recipient, amount)
                .gas_price(bumped_gas_price)
                .send()
                .await
            {
                if let Ok(Some(receipt)) = pending.await {
                    success_count += 1;
                    last_hash = format!("{:?}", receipt.transaction_hash);
                }
            }
        }

        Ok(TaskResult {
            success: success_count > 0,
            message: format!(
                "Completed batch of {}/{} stable transfers.",
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
