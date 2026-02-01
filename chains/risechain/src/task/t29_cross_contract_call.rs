use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::sync::Arc;

pub struct CrossContractCallTask;

impl CrossContractCallTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for CrossContractCallTask {
    fn name(&self) -> &str {
        "29_crossContractCall"
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
            "0x8628208543e2b16be283e30abec6fec7b91e5721".parse()?
        };

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let counter_abi_json = r#"[
            {"type":"function","name":"number","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]},
            {"type":"function","name":"increment","stateMutability":"nonpayable","inputs":[],"outputs":[]}
        ]"#;

        // Manual Bytecode for Counter (No PUSH0)
        // number() -> 0x8381f58a
        // increment() -> 0xd09de08a
        // Runtime: 60003560e01c80638381f58a14601e578063d09de08a14602a57600080fd5b60005460005260206000f35b60005460010160005500
        let runtime_hex = "60003560e01c80638381f58a14601e578063d09de08a14602a57600080fd5b60005460005260206000f35b60005460010160005500";
        let runtime_bytes = hex::decode(runtime_hex)?;

        // Loader: 603580600b6000396000f3 (0x35 = 53 bytes length)
        let loader_hex = "603580600b6000396000f3";
        let loader_bytes = hex::decode(loader_hex)?;

        let mut init_code = loader_bytes;
        init_code.extend(runtime_bytes);
        let init_code_bytes = Bytes::from(init_code);

        // SimpleFactory ABI
        let factory_abi_json = r#"[
            {"inputs":[{"internalType":"uint256","name":"salt","type":"uint256"},{"internalType":"bytes","name":"bytecode","type":"bytes"}],"name":"deploy","outputs":[{"internalType":"address","name":"addr","type":"address"}],"stateMutability":"nonpayable","type":"function"}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(factory_abi_json)?;
        let factory = Contract::new(factory_address, abi, Arc::new(provider.clone()));

        let salt: u64 = rand::thread_rng().gen();

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

        let mut target_address = Address::zero();
        for log in receipt.logs.iter() {
            if log.address == factory_address {
                if log.data.len() >= 32 {
                    target_address = Address::from_slice(&log.data[12..32]);
                }
            }
        }

        if target_address == Address::zero() {
            return Ok(TaskResult {
                success: false,
                message: "Deployed event not found in logs".to_string(),
                tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
            });
        }

        let code = provider.get_code(target_address, None).await?;
        if code.len() == 0 {
            return Ok(TaskResult {
                success: false,
                message: format!("Deployed contract has no code at {:?}", target_address),
                tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
            });
        }

        // Interact with deployed contract
        let counter_abi: abi::Abi = serde_json::from_str(counter_abi_json)?;
        let counter_contract =
            Contract::new(target_address, counter_abi, Arc::new(provider.clone()));

        let initial_value: U256 = counter_contract
            .method("number", ())?
            .call()
            .await
            .context("Failed to get initial value")?;

        let increment_data = counter_contract.encode("increment", ())?;
        let increment_tx = Eip1559TransactionRequest::new()
            .to(target_address)
            .data(increment_data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let increment_pending = client.send_transaction(increment_tx, None).await?;
        let increment_receipt = increment_pending
            .await?
            .context("Failed to get increment receipt")?;

        let new_value: U256 = counter_contract
            .method("number", ())?
            .call()
            .await
            .context("Failed to get new value")?;

        Ok(TaskResult {
            success: increment_receipt.status == Some(U64::from(1)),
            message: format!(
                "Cross-contract: called {:?}, value changed from {} to {}",
                target_address, initial_value, new_value
            ),
            tx_hash: Some(format!("{:?}", increment_receipt.transaction_hash)),
        })
    }
}
