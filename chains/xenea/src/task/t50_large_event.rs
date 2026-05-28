use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
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
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(Arc::new(provider.clone()), address);

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());

        // 3. Deploy large event contract
        let large_event_bytecode = "0x6080604052348015600e575f5ffd5b506102cd8061001c5f395ff3fe608060405234801561000f575f5ffd5b5060043610610029575f3560e01c806396dba1241461002d575b5f5ffd5b610047600480360381019061004291906101d0565b610049565b005b7f178db412f5c2aa6788c65368d58c78f81681c56c2f6c8001a8ecb108e72a02ae816040516100789190610277565b60405180910390a150565b5f604051905090565b5f5ffd5b5f5ffd5b5f5ffd5b5f5ffd5b5f601f19601f8301169050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52604160045260245ffd5b6100e28261009c565b810181811067ffffffffffffffff82111715610101576101006100ac565b5b80604052505050565b5f610113610083565b905061011f82826100d9565b919050565b5f67ffffffffffffffff82111561013e5761013d6100ac565b5b6101478261009c565b9050602081019050919050565b828183375f83830152505050565b5f61017461016f84610124565b61010a565b9050828152602081018484840111156101905761018f610098565b5b61019b848285610154565b509392505050565b5f82601f8301126101b7576101b6610094565b5b81356101c7848260208601610162565b91505092915050565b5f602082840312156101e5576101e461008c565b5b5f82013567ffffffffffffffff81111561020257610201610090565b5b61020e848285016101a3565b91505092915050565b5f81519050919050565b5f82825260208201905092915050565b8281835e5f83830152505050565b5f61024982610217565b6102538185610221565b9350610263818560208601610231565b61026c8161009c565b840191505092915050565b5f6020820190508181035f83015261028f818461023f565b90509291505056fea2646970667358221220d3b71faf2fbcd6961456b6c2b210bf931f00c4082092d2ae59358d22de765ceb64736f6c63430008210033";

        let deploy_data =
            crate::utils::strip_push0(&hex::decode(large_event_bytecode.trim_start_matches("0x")).unwrap());

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
                    Ok(Some(receipt)) if receipt.status == Some(U64::from(1)) => {
                        receipt.contract_address.context("No contract address in receipt")?
                    },
                    _ => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("LargeEventData deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    },
                }
            },
            Err(e) => {
                debug!("LargeEventData deploy submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit LargeEventData deploy tx: {}", e),
                    tx_hash: None,
                });
            },
        };

        debug!("Deployed LargeEventData at {:?}", contract_address);

        let large_event_abi_json = r#"[
            {"type":"function","name":"emitLargeData(bytes)","stateMutability":"nonpayable","inputs":[{"name":"data","type":"bytes"}],"outputs":[]}
        ]"#;
        let large_event_abi: abi::Abi = serde_json::from_str(large_event_abi_json)?;
        let large_event_contract = Contract::new(contract_address, large_event_abi, Arc::new(provider.clone()));

        // 4. emitLargeData (fire-and-forget)
        let mut rng = OsRng;
        let mut large_data = vec![0u8; 256];
        for byte in large_data.iter_mut() {
            *byte = rng.gen();
        }

        let emit_nonce = nonce_manager.next().await?;
        let emit_data = large_event_contract.encode("emitLargeData", Bytes::from(large_data.clone()))?;

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
                    "LargeEventData deployed at {:?}, emitLargeData(256 bytes) submitted (tx: {:?})",
                    contract_address,
                    pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("LargeEventData emit submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit LargeEventData emit tx: {}", e),
                    tx_hash: None,
                })
            },
        }
    }
}
