use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

pub struct DeployStormTask;

#[async_trait]
impl TempoTask for DeployStormTask {
    fn name(&self) -> &str {
        "50_deploy_storm"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        let size = rand::thread_rng().gen_range(3..6);

        println!("Starting DEPLOY STORM (Size: {})...", size);

        let mut success_count = 0;
        let mut last_addr = String::new();

        for i in 0..size {
            let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
            let bumped_gas_price = GasManager::bump_fees(gas_price);

            let bytecode = Bytes::from_str("0x60008060093d393df3")?;
            let factory = ContractFactory::new(abi::Abi::default(), bytecode, client.clone());

            println!("  [{}/{}] Launching deploy...", i + 1, size);
            if let Ok(mut deployer) = factory.deploy(()) {
                deployer.tx.set_gas_price(bumped_gas_price);
                if let Ok(contract) = deployer.send().await {
                    success_count += 1;
                    last_addr = format!("{:?}", contract.address());
                }
            }
        }

        Ok(TaskResult {
            success: success_count > 0,
            message: format!(
                "Deploy Storm completed: {}/{} successful launchers.",
                success_count, size
            ),
            tx_hash: if last_addr.is_empty() {
                None
            } else {
                Some(last_addr)
            },
        })
    }
}
