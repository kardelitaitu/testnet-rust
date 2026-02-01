use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

pub struct TimeBombTask;

#[async_trait]
impl TempoTask for TimeBombTask {
    fn name(&self) -> &str {
        "49_time_bomb"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        let delay = rand::thread_rng().gen_range(5..10); // Simulation

        println!("Setting up Time Bomb (Fuse: {}s)...", delay);

        // Simulation: On Tempo this uses validAfter. In Rust context, we'll simulate with a delayed broadcast.
        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // Deploy minimal contract as "impact"
        let bytecode = Bytes::from_str("0x60008060093d393df3")?;
        let factory = ContractFactory::new(abi::Abi::default(), bytecode, client.clone());

        let mut deployer = factory.deploy(())?;
        deployer.tx.set_gas_price(bumped_gas_price);
        let contract = deployer.send().await?;
        let contract_addr = contract.address();

        Ok(TaskResult {
            success: true,
            message: format!(
                "Detonated Time Bomb (Armed for {}s). Result: {:?}",
                delay, contract_addr
            ),
            tx_hash: Some(format!("{:?}", contract_addr)),
        })
    }
}
