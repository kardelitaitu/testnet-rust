use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct IndexedTopicsTask;

impl IndexedTopicsTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for IndexedTopicsTask {
    fn name(&self) -> &str {
        "49_indexedTopics"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let indexed_abi_json = r#"[
            {"type":"function","name":"emitMultiIndexed(address,address,uint256,uint256)","stateMutability":"nonpayable","inputs":[{"name":"from","type":"address"},{"name":"to","type":"address"},{"name":"id1","type":"uint256"},{"name":"id2","type":"uint256"}],"outputs":[]},
            {"type":"event","name":"MultiTransfer(address indexed,address indexed,uint256 indexed,uint256)","inputs":[{"name":"from","type":"address","indexed":true},{"name":"to","type":"address","indexed":true},{"name":"id1","type":"uint256","indexed":true},{"name":"id2","type":"uint256"}],"anonymous":false}
        ]"#;

        let indexed_bytecode = "0x6080604052348015600e575f5ffd5b506102088061001c5f395ff3fe608060405234801561000f575f5ffd5b5060043610610029575f3560e01c8063b5649b441461002d575b5f5ffd5b61004760048036038101906100429190610146565b610049565b005b818373ffffffffffffffffffffffffffffffffffffffff168573ffffffffffffffffffffffffffffffffffffffff167f6164e5ac06508497c1c4141a22d1fc986e6645a1ad6651fefb2c4ceb63b02260846040516100a791906101b9565b60405180910390a450505050565b5f5ffd5b5f73ffffffffffffffffffffffffffffffffffffffff82169050919050565b5f6100e2826100b9565b9050919050565b6100f2816100d8565b81146100fc575f5ffd5b50565b5f8135905061010d816100e9565b92915050565b5f819050919050565b61012581610113565b811461012f575f5ffd5b50565b5f813590506101408161011c565b92915050565b5f5f5f5f6080858703121561015e5761015d6100b5565b5b5f61016b878288016100ff565b945050602061017c878288016100ff565b935050604061018d87828801610132565b925050606061019e87828801610132565b91505092959194509250565b6101b381610113565b82525050565b5f6020820190506101cc5f8301846101aa565b9291505056fea2646970667358221220cd096a7bde2e5877a5229d52aa7d9cecb095d2307c5f6d58ae1ae8bb1a67842c64736f6c63430008210033";

        let deploy_data = hex::decode(&indexed_bytecode.trim_start_matches("0x")).unwrap();
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

        let indexed_abi: abi::Abi = serde_json::from_str(indexed_abi_json)?;
        let indexed_contract =
            Contract::new(contract_address, indexed_abi, Arc::new(provider.clone()));

        let recipient: Address = "0x4200000000000000000000000000000000000007"
            .parse()
            .context("Invalid recipient")?;
        let id1: u64 = 12345;
        let id2: u64 = 67890;

        let emit_data =
            indexed_contract.encode("emitMultiIndexed", (address, recipient, id1, id2))?;

        let emit_tx = Eip1559TransactionRequest::new()
            .to(contract_address)
            .data(emit_data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let emit_pending = client.send_transaction(emit_tx, None).await?;
        let emit_receipt = emit_pending.await?.context("Failed to get emit receipt")?;

        let event_topic =
            ethers::utils::keccak256("MultiTransfer(address,address,uint256,uint256)");

        let indexed_count = emit_receipt
            .logs
            .iter()
            .filter(|log| {
                log.topics.len() >= 3 && log.topics[0] == ethers::types::TxHash(event_topic)
            })
            .count();

        Ok(TaskResult {
            success: emit_receipt.status == Some(U64::from(1)),
            message: format!(
                "Indexed Topics: Deployed {:?}. Event with 3 indexed params found: {}",
                contract_address, indexed_count
            ),
            tx_hash: Some(format!("{:?}", emit_receipt.transaction_hash)),
        })
    }
}
