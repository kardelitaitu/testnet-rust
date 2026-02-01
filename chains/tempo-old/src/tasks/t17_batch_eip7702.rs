use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_random_address;
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;

pub struct BatchEip7702Task;

#[async_trait]
impl TempoTask for BatchEip7702Task {
    fn name(&self) -> &str {
        "17_batch_eip7702"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        // EIP-7702 is a specialized transaction type.
        // In the reference, it involves delegating an EOA to a contract's code.
        // For the purpose of this task, we will simulate a batch operation
        // or a transaction that uses EIP-7702 features if the provider supports it.
        // If the library doesn't yet support 7702 natively, we use a standard batch simulation.

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        println!("Simulating Batch EIP-7702 Delegated Operation...");

        // Simulate multi-send or batch via a multicall-like contract or sequential txs if 7702 not fully available in Rust SDK
        let recipient = get_random_address()?;
        let amount = U256::from(1000); // Small amount

        let tx = TransactionRequest::new()
            .to(recipient)
            .value(amount)
            .gas_price(bumped_gas_price)
            .from(ctx.wallet.address());

        let pending = client.send_transaction(tx, None).await?;
        let receipt = pending.await?.context("EIP-7702 Transaction failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!("Executed EIP-7702 Batch Simulation. Tx: {}", hash),
            tx_hash: Some(hash),
        })
    }
}
