use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct AnonymousEventTask;

impl AnonymousEventTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for AnonymousEventTask {
    fn name(&self) -> &str {
        "48_anonymousEvent"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let event_abi_json = r#"[
            {"type":"function","name":"emitAnonymous(uint256)","stateMutability":"nonpayable","inputs":[{"name":"value","type":"uint256"}],"outputs":[]},
            {"type":"function","name":"emitNamed(uint256)","stateMutability":"nonpayable","inputs":[{"name":"value","type":"uint256"}],"outputs":[]}
        ]"#;

        let event_bytecode = "0x6080604052348015600e575f5ffd5b506101838061001c5f395ff3fe608060405234801561000f575f5ffd5b5060043610610034575f3560e01c8063088f5397146100385780638130f89a14610054575b5f5ffd5b610052600480360381019061004d91906100fa565b610070565b005b61006e600480360381019061006991906100fa565b6100aa565b005b7fa7abb6db5a64d6f6d865cdd7cc2e4a0a49d3483ce9ec81b5bde62e1cd80ff0308160405161009f9190610134565b60405180910390a150565b806040516100b89190610134565b60405180910390a050565b5f5ffd5b5f819050919050565b6100d9816100c7565b81146100e3575f5ffd5b50565b5f813590506100f4816100d0565b92915050565b5f6020828403121561010f5761010e6100c3565b5b5f61011c848285016100e6565b91505092915050565b61012e816100c7565b82525050565b5f6020820190506101475f830184610125565b9291505056fea2646970667358221220fceeeebbb2efb92b44d97c67610fc4f5af03c464069f40da1e021cb5f999bbb664736f6c63430008210033";

        let deploy_data = hex::decode(&event_bytecode.trim_start_matches("0x")).unwrap();
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

        let event_abi: abi::Abi = serde_json::from_str(event_abi_json)?;
        let event_contract = Contract::new(contract_address, event_abi, Arc::new(provider.clone()));

        let value: u64 = 42;
        let emit_data = event_contract.encode("emitAnonymous", value)?;

        let emit_tx = Eip1559TransactionRequest::new()
            .to(contract_address)
            .data(emit_data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let emit_pending = client.send_transaction(emit_tx, None).await?;
        let emit_receipt = emit_pending.await?.context("Failed to get emit receipt")?;

        let _anonymous_topics = ethers::utils::keccak256("AnonymousEvent(uint256)");

        let mut anonymous_count = 0;
        for log in &emit_receipt.logs {
            if log.topics.is_empty() {
                anonymous_count += 1;
            }
        }

        Ok(TaskResult {
            success: emit_receipt.status == Some(U64::from(1)),
            message: format!(
                "Anonymous Event: Deployed {:?}. Emitted event (anonymous: {} topics)",
                contract_address, anonymous_count
            ),
            tx_hash: Some(format!("{:?}", emit_receipt.transaction_hash)),
        })
    }
}
