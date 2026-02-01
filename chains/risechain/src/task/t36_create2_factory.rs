use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;

pub struct Create2FactoryTask;

impl Create2FactoryTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for Create2FactoryTask {
    fn name(&self) -> &str {
        "36_create2Factory"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // Load factory from config or use fallback
        let factory_address: Address = if let Some(addr) = &ctx.config.create2_factory {
            addr.parse()
                .context("Invalid create2_factory address in config")?
        } else {
            // Fallback to the one we deployed in Phase 1 if config not updated
            "0x8628208543e2b16be283e30abec6fec7b91e5721".parse()?
        };

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let mut rng = OsRng;
        let salt: u64 = rng.gen();
        let salt_hex = format!("0x{:x}", salt);

        // Runtime code (Minimal Proxy logic)
        let runtime_code_hex = "363d3d373d3d3d363d73bebebebebebebebebebebebebebebebebebebebebebebebebebebe5af43d82803e903d91602b57fd5bf3";
        let runtime_code = hex::decode(runtime_code_hex)?;

        // Wrap in init code loader: 3d60<len>80600a3d3981f3
        // len = 0x37 (55 bytes)
        let loader_hex = "3d603780600a3d3981f3";
        let loader = hex::decode(loader_hex)?;

        let mut init_code = loader;
        init_code.extend(runtime_code);
        let init_code_bytes = Bytes::from(init_code);

        // SimpleFactory ABI (deploy(uint256,bytes))
        let factory_abi_json = r#"[
            {"inputs":[{"internalType":"uint256","name":"salt","type":"uint256"},{"internalType":"bytes","name":"bytecode","type":"bytes"}],"name":"deploy","outputs":[{"internalType":"address","name":"addr","type":"address"}],"stateMutability":"nonpayable","type":"function"}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(factory_abi_json)?;
        let factory = Contract::new(factory_address, abi, Arc::new(provider.clone()));

        // Encode call: deploy(uint256 salt, bytes bytecode)
        let deploy_data = factory.encode("deploy", (U256::from(salt), init_code_bytes))?;

        let tx = Eip1559TransactionRequest::new()
            .to(factory_address)
            .data(deploy_data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let client = std::sync::Arc::new(SignerMiddleware::new(
            std::sync::Arc::new(provider.clone()),
            wallet.clone(),
        ));
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        // In SimpleFactory, the deployed address is in the logs (Deployed event)
        let mut contract_address = Address::zero();
        for log in receipt.logs.iter() {
            if log.address == factory_address {
                // Decode log
                // addr is first 32 bytes (padded), salt is second 32 bytes
                if log.data.len() >= 32 {
                    contract_address = Address::from_slice(&log.data[12..32]);
                }
            }
        }

        if contract_address == Address::zero() {
            return Ok(TaskResult {
                success: false,
                message: "Deployed event not found in logs".to_string(),
                tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
            });
        }

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)),
            message: format!(
                "CREATE2 factory deployed contract to {:?} (salt: {})",
                contract_address, salt_hex
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
