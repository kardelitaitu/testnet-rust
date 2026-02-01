use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;

pub struct MemoryExpansionTask;

impl MemoryExpansionTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for MemoryExpansionTask {
    fn name(&self) -> &str {
        "51_memoryExpansion"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let memory_abi_json = r#"[
            {"type":"function","name":"processLargeArray(uint256[])","stateMutability":"nonpayable","inputs":[{"name":"arr","type":"uint256[]"}],"outputs":[{"name":"sum","type":"uint256"}]},
            {"type":"function","name":"processBytes(bytes)","stateMutability":"nonpayable","inputs":[{"name":"data","type":"bytes"}],"outputs":[{"name":"result","type":"bytes32"}]}
        ]"#;

        let memory_bytecode = "0x6080604052348015600e575f5ffd5b506104e08061001c5f395ff3fe608060405234801561000f575f5ffd5b5060043610610034575f3560e01c80636e65261014610038578063bb39484414610068575b5f5ffd5b610052600480360381019061004d919061023f565b610098565b60405161005f919061029e565b60405180910390f35b610082600480360381019061007d91906103ae565b6100a8565b60405161008f9190610404565b60405180910390f35b5f81805190602001209050919050565b5f5f5f90505b82518110156100ec578281815181106100ca576100c961041d565b5b6020026020010151826100dd9190610477565b915080806001019150506100ae565b50919050565b5f604051905090565b5f5ffd5b5f5ffd5b5f5ffd5b5f5ffd5b5f601f19601f8301169050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52604160045260245ffd5b6101518261010b565b810181811067ffffffffffffffff821117156101705761016f61011b565b5b80604052505050565b5f6101826100f2565b905061018e8282610148565b919050565b5f67ffffffffffffffff8211156101ad576101ac61011b565b5b6101b68261010b565b9050602081019050919050565b828183375f83830152505050565b5f6101e36101de84610193565b610179565b9050828152602081018484840111156101ff576101fe610107565b5b61020a8482856101c3565b509392505050565b5f82601f83011261022657610225610103565b5b81356102368482602086016101d1565b91505092915050565b5f60208284031215610254576102536100fb565b5b5f82013567ffffffffffffffff811115610271576102706100ff565b5b61027d84828501610212565b91505092915050565b5f819050919050565b61029881610286565b82525050565b5f6020820190506102b15f83018461028f565b92915050565b5f67ffffffffffffffff8211156102d1576102d061011b565b5b602082029050602081019050919050565b5f5ffd5b5f819050919050565b6102f8816102e6565b8114610302575f5ffd5b50565b5f81359050610313816102ef565b92915050565b5f61032b610326846102b7565b610179565b9050808382526020820190506020840283018581111561034e5761034d6102e2565b5b835b8181101561037757806103638882610305565b845260208401935050602081019050610350565b5050509392505050565b5f82601f83011261039557610394610103565b5b81356103a5848260208601610319565b91505092915050565b5f602082840312156103c3576103c26100fb565b5b5f82013567ffffffffffffffff8111156103e0576103df6100ff565b5b6103ec84828501610381565b91505092915050565b6103fe816102e6565b82525050565b5f6020820190506104175f8301846103f5565b92915050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52603260045260245ffd5b7f4e487b71000000000000000000000000000000000000000000000000000000005f52601160045260245ffd5b5f610481826102e6565b915061048c836102e6565b92508282019050808211156104a4576104a361044a565b5b9291505056fea26469706673582212208e4364ea67b06cbcc33378be86444606ecef0fd82236a41efa463487f964599564736f6c63430008210033";

        let deploy_data = hex::decode(&memory_bytecode.trim_start_matches("0x")).unwrap();
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

        let memory_abi: abi::Abi = serde_json::from_str(memory_abi_json)?;
        let memory_contract =
            Contract::new(contract_address, memory_abi, Arc::new(provider.clone()));

        let mut rng = OsRng;
        let large_array: Vec<U256> = (0..100).map(|_| U256::from(rng.gen::<u64>())).collect();

        let process_data = memory_contract.encode("processLargeArray", large_array.clone())?;

        let process_tx = Eip1559TransactionRequest::new()
            .to(contract_address)
            .data(process_data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let process_pending = client.send_transaction(process_tx, None).await?;
        let process_receipt = process_pending
            .await?
            .context("Failed to get process receipt")?;

        Ok(TaskResult {
            success: process_receipt.status == Some(U64::from(1)),
            message: format!(
                "Memory Expansion: Deployed {:?}. Processed array of {} uint256 values",
                contract_address,
                large_array.len()
            ),
            tx_hash: Some(format!("{:?}", process_receipt.transaction_hash)),
        })
    }
}
