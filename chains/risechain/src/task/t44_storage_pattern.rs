use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

pub struct StoragePatternTask;

impl StoragePatternTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for StoragePatternTask {
    fn name(&self) -> &str {
        "44_storagePattern"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let storage_abi_json = r#"[
            {"type":"constructor","stateMutability":"nonpayable","inputs":[{"name":"_packed","type":"uint256"}]},
            {"type":"function","name":"getPacked","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]},
            {"type":"function","name":"setValues(uint128,uint128)","stateMutability":"nonpayable","inputs":[{"name":"a","type":"uint128"},{"name":"b","type":"uint128"}],"outputs":[]}
        ]"#;

        let storage_bytecode = "0x6080604052348015600e575f5ffd5b505f5f819055506101ca806100225f395ff3fe608060405234801561000f575f5ffd5b506004361061003f575f3560e01c80638f23d5f614610043578063f1435cd314610061578063ffbfc1d41461007d575b5f5ffd5b61004b61009b565b60405161005891906100f4565b60405180910390f35b61007b60048036038101906100769190610156565b6100a0565b005b6100856100d4565b60405161009291906100f4565b60405180910390f35b5f5481565b806fffffffffffffffffffffffffffffffff166080836fffffffffffffffffffffffffffffffff16901b175f819055505050565b5f5f54905090565b5f819050919050565b6100ee816100dc565b82525050565b5f6020820190506101075f8301846100e5565b92915050565b5f5ffd5b5f6fffffffffffffffffffffffffffffffff82169050919050565b61013581610111565b811461013f575f5ffd5b50565b5f813590506101508161012c565b92915050565b5f5f6040838503121561016c5761016b61010d565b5b5f61017985828601610142565b925050602061018a85828601610142565b915050925092905056fea26469706673582212202f18e5aa758480b2d317d31bee28a4d99989417749aca2f451dd01d8a0589c0964736f6c63430008210033";

        let mut rng = OsRng;
        let value_a: u128 = rng.gen();
        let value_b: u128 = rng.gen();
        let packed = (U256::from(value_a) << 128) | U256::from(value_b);

        let deploy_data = hex::decode(&storage_bytecode.trim_start_matches("0x")).unwrap();
        let deploy_data_bytes = Bytes::from(deploy_data);

        debug!(
            "Deploying StoragePattern with {} bytes...",
            deploy_data_bytes.len()
        );

        let client = std::sync::Arc::new(SignerMiddleware::new(
            std::sync::Arc::new(provider.clone()),
            wallet.clone(),
        ));

        let deploy_tx = Eip1559TransactionRequest::new()
            .data(deploy_data_bytes.clone())
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        debug!(
            "Deploying StoragePattern with {} bytes...",
            deploy_data_bytes.len()
        );
        let deploy_pending = client.send_transaction(deploy_tx, None).await?;
        let deploy_receipt = deploy_pending
            .await?
            .context("Failed to get deploy receipt")?;

        let contract_address = deploy_receipt
            .contract_address
            .context("No contract address")?;

        debug!("Deployed at: {:?}", contract_address);

        // Verify code
        let code = provider.get_code(contract_address, None).await?;
        debug!("Code length at deployment: {}", code.len());
        if code.len() == 0 {
            return Err(anyhow::Error::msg("Deployed contract has no code!"));
        }

        let storage_abi: abi::Abi = serde_json::from_str(storage_abi_json)?;
        let storage = Contract::new(contract_address, storage_abi, Arc::new(provider.clone()));

        debug!("Calling getPacked (initial)...");
        let _packed_before: U256 = storage
            .method("getPacked", ())?
            .call()
            .await
            .context("Failed to get packed value")?;
        debug!("Initial packed value: {}", _packed_before);

        let set_data = storage.encode("setValues", (value_a, value_b))?;
        debug!("Setting values: {} and {}", value_a, value_b);
        let set_tx = Eip1559TransactionRequest::new()
            .to(contract_address)
            .data(set_data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let set_pending = client.send_transaction(set_tx, None).await?;
        let set_receipt = set_pending.await?.context("Failed to get set receipt")?;
        debug!("SetValues receipt status: {:?}", set_receipt.status);

        debug!("Calling getPacked (final)...");
        let packed_after: U256 = storage
            .method("getPacked", ())?
            .call()
            .await
            .context("Failed to get packed after")?;
        debug!("Final packed value: {}", packed_after);

        Ok(TaskResult {
            success: set_receipt.status == Some(U64::from(1)) && packed_after == packed,
            message: format!(
                "Storage Pattern: Deployed {:?}. Packed storage: {} = {} (<<128) + {}",
                contract_address, packed_after, value_a, value_b
            ),
            tx_hash: Some(format!("{:?}", set_receipt.transaction_hash)),
        })
    }
}
