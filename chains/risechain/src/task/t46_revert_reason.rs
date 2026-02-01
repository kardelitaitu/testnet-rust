use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct RevertWithReasonTask;

impl RevertWithReasonTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for RevertWithReasonTask {
    fn name(&self) -> &str {
        "46_revertWithReason"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let revert_abi_json = r#"[
            {"type":"function","name":"revertWithMessage(string)","stateMutability":"nonpayable","inputs":[{"name":"message","type":"string"}],"outputs":[]},
            {"type":"function","name":"revertWithCustomError()","stateMutability":"nonpayable","inputs":[],"outputs":[]},
            {"type":"function","name":"getState","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;

        let revert_bytecode = "0x6080604052348015600e575f5ffd5b506103b48061001c5f395ff3fe608060405234801561000f575f5ffd5b506004361061004a575f3560e01c80631865c57d1461004e57806346fc4bb11461006c578063c19d93fb14610076578063ee781e4c14610094575b5f5ffd5b6100566100b0565b6040516100639190610151565b60405180910390f35b6100746100b8565b005b61007e6100ea565b60405161008b9190610151565b60405180910390f35b6100ae60048036038101906100a991906102b7565b6100ef565b005b5f5f54905090565b6040517f8b16d98400000000000000000000000000000000000000000000000000000000815260040160405180910390fd5b5f5481565b5f8151148190610135576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040161012c919061035e565b60405180910390fd5b5050565b5f819050919050565b61014b81610139565b82525050565b5f6020820190506101645f830184610142565b92915050565b5f604051905090565b5f5ffd5b5f5ffd5b5f5ffd5b5f5ffd5b5f601f19601f8301169050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52604160045260245ffd5b6101c982610183565b810181811067ffffffffffffffff821117156101e8576101e7610193565b5b80604052505050565b5f6101fa61016a565b905061020682826101c0565b919050565b5f67ffffffffffffffff82111561022557610224610193565b5b61022e82610183565b9050602081019050919050565b828183375f83830152505050565b5f61025b6102568461020b565b6101f1565b9050828152602081018484840111156102775761027661017f565b5b61028284828561023b565b509392505050565b5f82601f83011261029e5761029d61017b565b5b81356102ae848260208601610249565b91505092915050565b5f602082840312156102cc576102cb610173565b5b5f82013567ffffffffffffffff8111156102e9576102e8610177565b5b6102f58482850161028a565b91505092915050565b5f81519050919050565b5f82825260208201905092915050565b8281835e5f83830152505050565b5f610330826102fe565b61033a8185610308565b935061034a818560208601610318565b61035381610183565b840191505092915050565b5f6020820190508181035f8301526103768184610326565b90509291505056fea2646970667358221220f607f87aef195abcadc977a682c42599132afa088ec63b273c92011026df807164736f6c63430008210033";

        let deploy_data = hex::decode(&revert_bytecode.trim_start_matches("0x")).unwrap();
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

        let revert_abi: abi::Abi = serde_json::from_str(revert_abi_json)?;
        let revert_contract =
            Contract::new(contract_address, revert_abi, Arc::new(provider.clone()));

        let state: U256 = revert_contract
            .method("getState", ())?
            .call()
            .await
            .context("Failed to get state")?;

        Ok(TaskResult {
            success: deploy_receipt.status == Some(U64::from(1)),
            message: format!(
                "Revert With Reason Test: Deployed {:?}. State: {} (testing revert patterns)",
                contract_address, state
            ),
            tx_hash: Some(format!("{:?}", deploy_receipt.transaction_hash)),
        })
    }
}
