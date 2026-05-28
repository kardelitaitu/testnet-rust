use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct StoragePatternTask;

impl StoragePatternTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for StoragePatternTask {
    fn name(&self) -> &str {
        "44_storagePattern"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let gas_price = U256::from(1_100_000_000u64);
        let deploy_gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;
        let set_gas_limit = 500_000u64;
        let estimated_gas = U256::from(deploy_gas_limit.as_u64() + set_gas_limit) * gas_price;

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

        // 3. Deploy storage contract
        let storage_bytecode = "0x6080604052348015600e575f5ffd5b505f5f819055506101ca806100225f395ff3fe608060405234801561000f575f5ffd5b506004361061003f575f3560e01c80638f23d5f614610043578063f1435cd314610061578063ffbfc1d41461007d575b5f5ffd5b61004b61009b565b60405161005891906100f4565b60405180910390f35b61007b60048036038101906100769190610156565b6100a0565b005b6100856100d4565b60405161009291906100f4565b60405180910390f35b5f5481565b806fffffffffffffffffffffffffffffffff166080836fffffffffffffffffffffffffffffffff16901b175f819055505050565b5f5f54905090565b5f819050919050565b6100ee816100dc565b82525050565b5f6020820190506101075f8301846100e5565b92915050565b5f5ffd5b5f6fffffffffffffffffffffffffffffffff82169050919050565b61013581610111565b811461013f575f5ffd5b50565b5f813590506101508161012c565b92915050565b5f5f6040838503121561016c5761016b61010d565b5b5f61017985828601610142565b925050602061018a85828601610142565b915050925092905056fea26469706673582212202f18e5aa758480b2d317d31bee28a4d99989417749aca2f451dd01d8a0589c0964736f6c63430008210033";

        let mut rng = OsRng;
        let value_a: u128 = rng.gen();
        let value_b: u128 = rng.gen();
        let packed = (U256::from(value_a) << 128) | U256::from(value_b);

        let deploy_data = crate::utils::strip_push0(&hex::decode(storage_bytecode.trim_start_matches("0x")).unwrap());

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
                            message: format!("StoragePattern deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    },
                }
            },
            Err(e) => {
                debug!("StoragePattern deploy submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit StoragePattern deploy tx: {}", e),
                    tx_hash: None,
                });
            },
        };

        debug!("Deployed StoragePattern at {:?}", contract_address);

        let storage_abi_json = r#"[
            {"type":"function","name":"getPacked","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]},
            {"type":"function","name":"setValues(uint128,uint128)","stateMutability":"nonpayable","inputs":[{"name":"a","type":"uint128"},{"name":"b","type":"uint128"}],"outputs":[]}
        ]"#;
        let storage_abi: abi::Abi = serde_json::from_str(storage_abi_json)?;
        let storage = Contract::new(contract_address, storage_abi, Arc::new(provider.clone()));

        // 4. setValues (fire-and-forget)
        let set_nonce = nonce_manager.next().await?;
        let set_data = storage.encode("setValues", (value_a, value_b))?;

        let set_tx = TransactionRequest::new()
            .to(contract_address)
            .data(set_data)
            .gas(set_gas_limit)
            .gas_price(gas_price)
            .nonce(set_nonce)
            .from(address);

        let pending_set = client.send_transaction(set_tx, None).await;

        match pending_set {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!(
                    "StoragePattern deployed at {:?}, setValues submitted: {} = {} (<<128) + {} (tx: {:?})",
                    contract_address,
                    packed,
                    value_a,
                    value_b,
                    pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("StoragePattern setValues submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit StoragePattern setValues tx: {}", e),
                    tx_hash: None,
                })
            },
        }
    }
}
