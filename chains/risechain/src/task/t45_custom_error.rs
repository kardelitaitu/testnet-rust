use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct CustomErrorTestTask;

impl CustomErrorTestTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for CustomErrorTestTask {
    fn name(&self) -> &str {
        "45_customErrorTest"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let error_abi_json = r#"[
            {"type":"function","name":"testError(bool)","stateMutability":"nonpayable","inputs":[{"name":"shouldFail","type":"bool"}],"outputs":[]},
            {"type":"function","name":"getData","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;

        let error_bytecode = "0x6080604052348015600e575f5ffd5b506101b58061001c5f395ff3fe608060405234801561000f575f5ffd5b506004361061003f575f3560e01c80633bc5de301461004357806373d4a13a146100615780639467c6ed1461007f575b5f5ffd5b61004b61009b565b6040516100589190610102565b60405180910390f35b6100696100a3565b6040516100769190610102565b60405180910390f35b61009960048036038101906100949190610154565b6100a8565b005b5f5f54905090565b5f5481565b80156100e0576040517f4e7254d600000000000000000000000000000000000000000000000000000000815260040160405180910390fd5b602d5f8190555050565b5f819050919050565b6100fc816100ea565b82525050565b5f6020820190506101155f8301846100f3565b92915050565b5f5ffd5b5f8115159050919050565b6101338161011f565b811461013d575f5ffd5b50565b5f8135905061014e8161012a565b92915050565b5f602082840312156101695761016861011b565b5b5f61017684828501610140565b9150509291505056fea26469706673582212201dbaaf7a107333b9b5895808c42d4a4c6a234ffd99ca2388d43581460d29550764736f6c63430008210033";

        let deploy_data = hex::decode(&error_bytecode.trim_start_matches("0x")).unwrap();
        let deploy_data_bytes = Bytes::from(deploy_data);

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

        let deploy_pending = client.send_transaction(deploy_tx, None).await?;
        let deploy_receipt = deploy_pending
            .await?
            .context("Failed to get deploy receipt")?;

        let contract_address = deploy_receipt
            .contract_address
            .context("No contract address")?;

        let error_abi: abi::Abi = serde_json::from_str(error_abi_json)?;
        let error_contract = Contract::new(contract_address, error_abi, Arc::new(provider.clone()));

        let data: U256 = error_contract
            .method("getData", ())?
            .call()
            .await
            .context("Failed to get data")?;

        Ok(TaskResult {
            success: deploy_receipt.status == Some(U64::from(1)),
            message: format!(
                "Custom Error Test: Deployed {:?}. Initial data value: {}",
                contract_address, data
            ),
            tx_hash: Some(format!("{:?}", deploy_receipt.transaction_hash)),
        })
    }
}
