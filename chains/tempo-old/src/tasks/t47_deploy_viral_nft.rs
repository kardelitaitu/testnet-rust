use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::contract_compiler::ContractCompiler;
use crate::utils::gas_manager::GasManager;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::path::Path;
use std::str::FromStr;
use tracing::info;

pub struct DeployViralNftTask;

#[async_trait]
impl TempoTask for DeployViralNftTask {
    fn name(&self) -> &str {
        "47_deploy_viral_nft"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        info!("Running Deploy Viral NFT Task...");

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        let contract_path = Path::new("chains/tempo/contracts/ViralNFT.sol");
        let (abi_str, bin_str) = ContractCompiler::compile(contract_path)
            .map_err(|e| anyhow!("Failed to compile ViralNFT.sol: {}", e))?;

        info!("ViralNFT compiled successfully.");

        let bytecode = Bytes::from_str(&bin_str)?;

        let abi: ethers::abi::Abi =
            serde_json::from_str(&abi_str).map_err(|e| anyhow!("Failed to parse ABI: {}", e))?;

        let factory = ContractFactory::new(abi, bytecode, client.clone());

        let current_gas = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas = GasManager::bump_fees(current_gas);

        let mut deployer = factory.deploy(("Viral NFT".to_string(), "VNFT".to_string()))?;
        deployer.tx.set_gas_price(bumped_gas);

        info!("Deploying Viral NFT with constructor args (name='Viral NFT', symbol='VNFT')...");
        let contract = deployer.send().await?;
        let contract_addr = contract.address();

        info!("ViralNFT deployed successfully at {:?}", contract_addr);

        if let Some(db) = ctx.db.as_ref() {
            db.log_asset_creation(
                &format!("{:?}", ctx.wallet.address()),
                &format!("{:?}", contract_addr),
                "viral_nft",
                "Viral NFT",
                "VNFT",
            )
            .await?;
        }

        Ok(TaskResult {
            success: true,
            message: format!("Deployed Viral NFT at {:?}", contract_addr),
            tx_hash: Some(format!("{:?}", contract_addr)),
        })
    }
}
