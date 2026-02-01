use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_random_address;
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;

pub struct TransferLaterTask;

#[async_trait]
impl TempoTask for TransferLaterTask {
    fn name(&self) -> &str {
        "37_transfer_later"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        let delay = rand::thread_rng().gen_range(2..5); // Seconds
        let recipient = get_random_address()?;
        let amount = U256::from(100);

        println!(
            "Scheduling transfer to {:?} in {} seconds...",
            recipient, delay
        );

        // Simulation of "Later" via sleep in the task flow
        // In a real scheduler, this would be a separate process or block-hash based wait
        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        let tx = TransactionRequest::new()
            .to(recipient)
            .value(amount)
            .gas_price(bumped_gas_price)
            .from(ctx.wallet.address());

        let pending = client.send_transaction(tx, None).await?;
        let receipt = pending.await?.context("Transfer failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!(
                "Executed scheduled transfer (waited {}s). Tx: {}",
                delay, hash
            ),
            tx_hash: Some(hash),
        })
    }
}
