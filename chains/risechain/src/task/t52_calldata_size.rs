use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;

pub struct CalldataSizeTask;

impl CalldataSizeTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for CalldataSizeTask {
    fn name(&self) -> &str {
        "52_calldataSize"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let calldata_abi_json = r#"[
            {"type":"function","name":"storeData(bytes)","stateMutability":"nonpayable","inputs":[{"name":"data","type":"bytes"}],"outputs":[]},
            {"type":"function","name":"getDataHash()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"bytes32"}]}
        ]"#;

        let calldata_bytecode = "0x6080604052348015600e575f5ffd5b506102b38061001c5f395ff3fe608060405234801561000f575f5ffd5b506004361061003f575f3560e01c80631b3012a314610043578063a4da229014610061578063ac5c85351461007f575b5f5ffd5b61004b61009b565b60405161005891906100d0565b60405180910390f35b6100696100a0565b60405161007691906100d0565b60405180910390f35b61009960048036038101906100949190610236565b6100a8565b005b5f5481565b5f5f54905090565b80805190602001205f8190555050565b5f819050919050565b6100ca816100b8565b82525050565b5f6020820190506100e35f8301846100c1565b92915050565b5f604051905090565b5f5ffd5b5f5ffd5b5f5ffd5b5f5ffd5b5f601f19601f8301169050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52604160045260245ffd5b61014882610102565b810181811067ffffffffffffffff8211171561016757610166610112565b5b80604052505050565b5f6101796100e9565b9050610185828261013f565b919050565b5f67ffffffffffffffff8211156101a4576101a3610112565b5b6101ad82610102565b9050602081019050919050565b828183375f83830152505050565b5f6101da6101d58461018a565b610170565b9050828152602081018484840111156101f6576101f56100fe565b5b6102018482856101ba565b509392505050565b5f82601f83011261021d5761021c6100fa565b5b813561022d8482602086016101c8565b91505092915050565b5f6020828403121561024b5761024a6100f2565b5b5f82013567ffffffffffffffff811115610268576102676100f6565b5b61027484828501610209565b9150509291505056fea26469706673582212201c68fa332f46ef2db292cf454bef9dab2cfaddf76d5c7f33bd22fcafe397b1c264736f6c63430008210033";

        let deploy_data = hex::decode(&calldata_bytecode.trim_start_matches("0x")).unwrap();
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

        let calldata_abi: abi::Abi = serde_json::from_str(calldata_abi_json)?;
        let calldata_contract =
            Contract::new(contract_address, calldata_abi, Arc::new(provider.clone()));

        let mut rng = OsRng;
        let mut large_calldata = vec![0u8; 512];
        for byte in large_calldata.iter_mut() {
            *byte = rng.gen();
        }

        let store_data =
            calldata_contract.encode("storeData", Bytes::from(large_calldata.clone()))?;

        let store_tx = Eip1559TransactionRequest::new()
            .to(contract_address)
            .data(store_data.clone())
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let store_pending = client.send_transaction(store_tx, None).await?;
        let store_receipt = store_pending
            .await?
            .context("Failed to get store receipt")?;

        let calldata_size = store_data.len();

        Ok(TaskResult {
            success: store_receipt.status == Some(U64::from(1)),
            message: format!(
                "Calldata Size: Deployed {:?}. Sent calldata with {} bytes",
                contract_address, calldata_size
            ),
            tx_hash: Some(format!("{:?}", store_receipt.transaction_hash)),
        })
    }
}
