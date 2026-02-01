use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::sync::Arc;

pub struct DelegatecallTask;

impl DelegatecallTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for DelegatecallTask {
    fn name(&self) -> &str {
        "28_delegatecall"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // Load factory from config or use fallback
        let create2_address: Address = if let Some(addr) = &ctx.config.create2_factory {
            addr.parse()
                .context("Invalid create2_factory address in config")?
        } else {
            "0x8628208543e2b16be283e30abec6fec7b91e5721".parse()?
        };

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let counter_abi_str = include_str!("../../contracts/Counter_abi.txt").trim();
        let counter_bytecode_str = include_str!("../../contracts/Counter_bytecode.txt").trim();
        let counter_bytecode_bytes = Bytes::from(
            hex::decode(counter_bytecode_str).context("Failed to decode Counter bytecode")?,
        );

        // Factory ABI
        let factory_abi_json = r#"[
            {"inputs":[{"internalType":"uint256","name":"salt","type":"uint256"},{"internalType":"bytes","name":"bytecode","type":"bytes"}],"name":"deploy","outputs":[{"internalType":"address","name":"addr","type":"address"}],"stateMutability":"nonpayable","type":"function"}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(factory_abi_json)?;
        let factory = Contract::new(create2_address, abi, Arc::new(provider.clone()));

        let mut rng = rand::rngs::OsRng;
        let salt: u64 = rng.gen();
        let deploy_data = factory.encode("deploy", (U256::from(salt), counter_bytecode_bytes))?;

        let tx = Eip1559TransactionRequest::new()
            .to(create2_address)
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

        if receipt.status != Some(U64::from(1)) {
            return Ok(TaskResult {
                success: false,
                message: "Factory deploy transaction failed (reverted)".to_string(),
                tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
            });
        }

        let mut contract_address = Address::zero();
        for log in receipt.logs.iter() {
            if log.address == create2_address {
                // Decode log
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

        let counter_abi: abi::Abi = serde_json::from_str(counter_abi_str)?;
        let counter = Contract::new(contract_address, counter_abi, Arc::new(provider.clone()));

        let initial_value: U256 = counter
            .method("count", ())?
            .call()
            .await
            .context("Failed to get initial value")?;

        let increment_data = counter.encode("increment", ())?;
        let increment_tx = Eip1559TransactionRequest::new()
            .to(contract_address)
            .data(increment_data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let increment_pending = client.send_transaction(increment_tx, None).await?;
        let increment_receipt = increment_pending
            .await?
            .context("Failed to get increment receipt")?;

        let new_value: U256 = counter
            .method("count", ())?
            .call()
            .await
            .context("Failed to get new value")?;

        Ok(TaskResult {
            success: increment_receipt.status == Some(U64::from(1)),
            message: format!(
                "Counter deployed at {:?}, count changed from {} to {}",
                contract_address, initial_value, new_value
            ),
            tx_hash: Some(format!("{:?}", increment_receipt.transaction_hash)),
        })
    }
}
