use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct AssertFailTask;

impl AssertFailTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for AssertFailTask {
    fn name(&self) -> &str {
        "47_assertFail"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let assert_abi_json = r#"[
            {"type":"function","name":"assertCheck(uint256)","stateMutability":"nonpayable","inputs":[{"name":"value","type":"uint256"}],"outputs":[]},
            {"type":"function","name":"requireCheck(uint256)","stateMutability":"nonpayable","inputs":[{"name":"value","type":"uint256"}],"outputs":[]},
            {"type":"function","name":"getValue","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;

        let assert_bytecode = "0x6080604052348015600e575f5ffd5b506101e98061001c5f395ff3fe608060405234801561000f575f5ffd5b506004361061004a575f3560e01c8063209652551461004e5780633fa4f2451461006c578063761da2d51461008a578063c4c0c46f146100a6575b5f5ffd5b6100566100c2565b6040516100639190610114565b60405180910390f35b6100746100ca565b6040516100819190610114565b60405180910390f35b6100a4600480360381019061009f919061015b565b6100cf565b005b6100c060048036038101906100bb919061015b565b6100e3565b005b5f5f54905090565b5f5481565b805f819055505f81036100e0575f5ffd5b50565b805f819055505f81036100f9576100f8610186565b5b50565b5f819050919050565b61010e816100fc565b82525050565b5f6020820190506101275f830184610105565b92915050565b5f5ffd5b61013a816100fc565b8114610144575f5ffd5b50565b5f8135905061015581610131565b92915050565b5f602082840312156101705761016f61012d565b5b5f61017d84828501610147565b91505092915050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52600160045260245ffdfea2646970667358221220c515b3db4a3c6aa8e2664f670421c17f1f8ba475385be1010c22390555cfac6064736f6c63430008210033";

        let deploy_data = hex::decode(&assert_bytecode.trim_start_matches("0x")).unwrap();
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

        let assert_abi: abi::Abi = serde_json::from_str(assert_abi_json)?;
        let assert_contract =
            Contract::new(contract_address, assert_abi, Arc::new(provider.clone()));

        let value: U256 = assert_contract
            .method("getValue", ())?
            .call()
            .await
            .context("Failed to get value")?;

        Ok(TaskResult {
            success: deploy_receipt.status == Some(U64::from(1)),
            message: format!(
                "Assert Fail Test: Deployed {:?}. Value: {} (testing assert/require patterns)",
                contract_address, value
            ),
            tx_hash: Some(format!("{:?}", deploy_receipt.transaction_hash)),
        })
    }
}
