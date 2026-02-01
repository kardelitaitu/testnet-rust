use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;

pub struct LargeEventDataTask;

impl LargeEventDataTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for LargeEventDataTask {
    fn name(&self) -> &str {
        "50_largeEventData"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let large_event_abi_json = r#"[
            {"type":"function","name":"emitLargeData(bytes)","stateMutability":"nonpayable","inputs":[{"name":"data","type":"bytes"}],"outputs":[]},
            {"type":"event","name":"LargeData(bytes)","inputs":[{"name":"data","type":"bytes"}],"anonymous":false}
        ]"#;

        let large_event_bytecode = "0x6080604052348015600e575f5ffd5b506102cd8061001c5f395ff3fe608060405234801561000f575f5ffd5b5060043610610029575f3560e01c806396dba1241461002d575b5f5ffd5b610047600480360381019061004291906101d0565b610049565b005b7f178db412f5c2aa6788c65368d58c78f81681c56c2f6c8001a8ecb108e72a02ae816040516100789190610277565b60405180910390a150565b5f604051905090565b5f5ffd5b5f5ffd5b5f5ffd5b5f5ffd5b5f601f19601f8301169050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52604160045260245ffd5b6100e28261009c565b810181811067ffffffffffffffff82111715610101576101006100ac565b5b80604052505050565b5f610113610083565b905061011f82826100d9565b919050565b5f67ffffffffffffffff82111561013e5761013d6100ac565b5b6101478261009c565b9050602081019050919050565b828183375f83830152505050565b5f61017461016f84610124565b61010a565b9050828152602081018484840111156101905761018f610098565b5b61019b848285610154565b509392505050565b5f82601f8301126101b7576101b6610094565b5b81356101c7848260208601610162565b91505092915050565b5f602082840312156101e5576101e461008c565b5b5f82013567ffffffffffffffff81111561020257610201610090565b5b61020e848285016101a3565b91505092915050565b5f81519050919050565b5f82825260208201905092915050565b8281835e5f83830152505050565b5f61024982610217565b6102538185610221565b9350610263818560208601610231565b61026c8161009c565b840191505092915050565b5f6020820190508181035f83015261028f818461023f565b90509291505056fea2646970667358221220d3b71faf2fbcd6961456b6c2b210bf931f00c4082092d2ae59358d22de765ceb64736f6c63430008210033";

        let deploy_data = hex::decode(&large_event_bytecode.trim_start_matches("0x")).unwrap();
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

        let large_event_abi: abi::Abi = serde_json::from_str(large_event_abi_json)?;
        let large_event_contract = Contract::new(
            contract_address,
            large_event_abi,
            Arc::new(provider.clone()),
        );

        let mut rng = OsRng;
        let mut large_data = vec![0u8; 256];
        for byte in large_data.iter_mut() {
            *byte = rng.gen();
        }

        let emit_data =
            large_event_contract.encode("emitLargeData", Bytes::from(large_data.clone()))?;

        let emit_tx = Eip1559TransactionRequest::new()
            .to(contract_address)
            .data(emit_data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let emit_pending = client.send_transaction(emit_tx, None).await?;
        let emit_receipt = emit_pending.await?.context("Failed to get emit receipt")?;

        let event_data_size = emit_receipt
            .logs
            .first()
            .map(|log| log.data.len())
            .unwrap_or(0);

        Ok(TaskResult {
            success: emit_receipt.status == Some(U64::from(1)),
            message: format!(
                "Large Event Data: Deployed {:?}. Event data size: {} bytes",
                contract_address, event_data_size
            ),
            tx_hash: Some(format!("{:?}", emit_receipt.transaction_hash)),
        })
    }
}
