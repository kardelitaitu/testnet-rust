use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

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

        let gas_price = U256::from(1_100_000_000u64);
        let deploy_gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;
        let emit_gas_limit = 500_000u64;
        let estimated_gas = U256::from(deploy_gas_limit.as_u64() + emit_gas_limit) * gas_price;

        // 1. Balance check
        let balance = provider.get_balance(address, None).await?;
        if balance <= estimated_gas {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient TXENE for gas: need {} Wei, have {} Wei",
                    estimated_gas, balance
                ),
                tx_hash: None,
            });
        }

        // 2. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());

        // 3. Deploy indexed topics contract
        let indexed_bytecode = "0x6080604052348015600e575f5ffd5b506102088061001c5f395ff3fe608060405234801561000f575f5ffd5b5060043610610029575f3560e01c8063b5649b441461002d575b5f5ffd5b61004760048036038101906100429190610146565b610049565b005b818373ffffffffffffffffffffffffffffffffffffffff168573ffffffffffffffffffffffffffffffffffffffff167f6164e5ac06508497c1c4141a22d1fc986e6645a1ad6651fefb2c4ceb63b02260846040516100a791906101b9565b60405180910390a450505050565b5f5ffd5b5f73ffffffffffffffffffffffffffffffffffffffff82169050919050565b5f6100e2826100b9565b9050919050565b6100f2816100d8565b81146100fc575f5ffd5b50565b5f8135905061010d816100e9565b92915050565b5f819050919050565b61012581610113565b811461012f575f5ffd5b50565b5f813590506101408161011c565b92915050565b5f5f5f5f6080858703121561015e5761015d6100b5565b5b5f61016b878288016100ff565b945050602061017c878288016100ff565b935050604061018d87828801610132565b925050606061019e87828801610132565b91505092959194509250565b6101b381610113565b82525050565b5f6020820190506101cc5f8301846101aa565b9291505056fea2646970667358221220cd096a7bde2e5877a5229d52aa7d9cecb095d2307c5f6d58ae1ae8bb1a67842c64736f6c63430008210033";

        let deploy_data = crate::utils::strip_push0(
            &hex::decode(&indexed_bytecode.trim_start_matches("0x")).unwrap(),
        );

        let deploy_nonce = nonce_manager.next().await?;
        let deploy_tx = TransactionRequest::new()
            .data(Bytes::from(deploy_data))
            .gas(deploy_gas_limit)
            .gas_price(gas_price)
            .nonce(deploy_nonce)
            .from(address);

        let pending_deploy = client.send_transaction(deploy_tx, None).await;
        let contract_address = match pending_deploy {
            Ok(pending) => {
                let tx_hash = format!("{:?}", pending.tx_hash());
                match pending.await {
                    Ok(Some(receipt)) if receipt.status == Some(U64::from(1)) => receipt
                        .contract_address
                        .context("No contract address in receipt")?,
                    _ => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("IndexedTopics deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    }
                }
            }
            Err(e) => {
                debug!("IndexedTopics deploy submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit IndexedTopics deploy tx: {}", e),
                    tx_hash: None,
                });
            }
        };

        debug!("Deployed IndexedTopics at {:?}", contract_address);

        let indexed_abi_json = r#"[
            {"type":"function","name":"emitMultiIndexed(address,address,uint256,uint256)","stateMutability":"nonpayable","inputs":[{"name":"from","type":"address"},{"name":"to","type":"address"},{"name":"id1","type":"uint256"},{"name":"id2","type":"uint256"}],"outputs":[]}
        ]"#;
        let indexed_abi: abi::Abi = serde_json::from_str(indexed_abi_json)?;
        let indexed_contract =
            Contract::new(contract_address, indexed_abi, Arc::new(provider.clone()));

        // 4. emitMultiIndexed (fire-and-forget)
        let emit_nonce = nonce_manager.next().await?;
        let recipient: Address = "0x4200000000000000000000000000000000000007"
            .parse()
            .context("Invalid recipient")?;

        let emit_data = indexed_contract.encode(
            "emitMultiIndexed",
            (address, recipient, U256::from(12345), U256::from(67890)),
        )?;

        let emit_tx = TransactionRequest::new()
            .to(contract_address)
            .data(emit_data)
            .gas(emit_gas_limit)
            .gas_price(gas_price)
            .nonce(emit_nonce)
            .from(address);

        let pending_emit = client.send_transaction(emit_tx, None).await;

        match pending_emit {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!(
                    "IndexedTopics deployed at {:?}, emitMultiIndexed submitted (tx: {:?})",
                    contract_address,
                    pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("IndexedTopics emit submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit IndexedTopics emit tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
