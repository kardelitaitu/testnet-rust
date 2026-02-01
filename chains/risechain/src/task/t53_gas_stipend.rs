use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct GasStipendTask;

impl GasStipendTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for GasStipendTask {
    fn name(&self) -> &str {
        "53_gasStipend"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let stipend_abi_json = r#"[
            {"type":"function","name":"callWithGas(uint256)","stateMutability":"nonpayable","inputs":[{"name":"gasAmount","type":"uint256"}],"outputs":[{"name":"success","type":"bool"},{"name":"data","type":"bytes"}]}
        ]"#;

        let stipend_bytecode = "0x6080604052348015600e575f5ffd5b506102148061001c5f395ff3fe608060405234801561000f575f5ffd5b5060043610610029575f3560e01c8063bc568ec41461002d575b5f5ffd5b610047600480360381019061004291906100fb565b61005e565b6040516100559291906101b0565b60405180910390f35b5f6060825a10156100a9575f6040518060400160405280600e81526020017f4e6f7420656e6f75676820676173000000000000000000000000000000000000815250915091506100bf565b600160405180602001604052805f815250915091505b915091565b5f5ffd5b5f819050919050565b6100da816100c8565b81146100e4575f5ffd5b50565b5f813590506100f5816100d1565b92915050565b5f602082840312156101105761010f6100c4565b5b5f61011d848285016100e7565b91505092915050565b5f8115159050919050565b61013a81610126565b82525050565b5f81519050919050565b5f82825260208201905092915050565b8281835e5f83830152505050565b5f601f19601f8301169050919050565b5f61018282610140565b61018c818561014a565b935061019c81856020860161015a565b6101a581610168565b840191505092915050565b5f6040820190506101c35f830185610131565b81810360208301526101d58184610178565b9050939250505056fea26469706673582212208e11a5da5ad4eecbe429ff084c31b6165c372497a2b4fef337b92620bcb516d964736f6c63430008210033";

        let deploy_data = hex::decode(&stipend_bytecode.trim_start_matches("0x")).unwrap();
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

        let stipend_abi: abi::Abi = serde_json::from_str(stipend_abi_json)?;
        let stipend_contract =
            Contract::new(contract_address, stipend_abi, Arc::new(provider.clone()));

        let gas_amount = 50000u64;
        let call_data = stipend_contract.encode("callWithGas", gas_amount)?;

        let call_tx = Eip1559TransactionRequest::new()
            .to(contract_address)
            .data(call_data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let call_pending = client.send_transaction(call_tx, None).await?;
        let call_receipt = call_pending.await?.context("Failed to get call receipt")?;

        Ok(TaskResult {
            success: call_receipt.status == Some(U64::from(1)),
            message: format!(
                "Gas Stipend: Deployed {:?}. Called with gas stipend: {}",
                contract_address, gas_amount
            ),
            tx_hash: Some(format!("{:?}", call_receipt.transaction_hash)),
        })
    }
}
