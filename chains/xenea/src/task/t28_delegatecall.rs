use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tracing::debug;

#[derive(Default)]
pub struct DelegatecallTask;

impl DelegatecallTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for DelegatecallTask {
    fn name(&self) -> &str {
        "28_simpleContract"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let gas_price = U256::from(1_100_000_000u64);
        let deploy_gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;
        let call_gas_limit = 500_000u64;
        let estimated_gas = (deploy_gas_limit + call_gas_limit) * gas_price;

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

        // 3. Deploy Counter directly (not via CREATE2)
        let counter_abi_str = include_str!("../../contracts/Counter_abi.txt").trim();
        let counter_bytecode_str = include_str!("../../contracts/Counter_bytecode.txt").trim();
        let counter_bytecode_bytes =
            Bytes::from(hex::decode(counter_bytecode_str).context("Failed to decode Counter bytecode")?);

        let deploy_nonce = nonce_manager.next().await?;
        let deploy_tx = TransactionRequest::new()
            .data(counter_bytecode_bytes)
            .gas(deploy_gas_limit)
            .gas_price(gas_price)
            .nonce(deploy_nonce)
            .from(address);

        let pending_deploy = client.send_transaction(deploy_tx, None).await;
        let contract_address = match pending_deploy {
            Ok(pending) => {
                let tx_hash = format!("{:?}", pending.tx_hash());
                match timeout(Duration::from_secs(60), pending).await {
                    Ok(Ok(Some(receipt))) if receipt.status == Some(U64::from(1)) => {
                        receipt.contract_address.context("No contract address in receipt")?
                    },
                    Ok(Ok(Some(_))) => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("Counter deploy reverted (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    },
                    Ok(Ok(None)) | Err(_) => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("Counter deploy timed out (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    },
                    Ok(Err(e)) => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("Counter deploy receipt failed (tx: {}): {}", tx_hash, e),
                            tx_hash: Some(tx_hash),
                        });
                    },
                }
            },
            Err(e) => {
                debug!("Delegatecall deploy submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit Counter deploy tx: {}", e),
                    tx_hash: None,
                });
            },
        };

        debug!("Deployed Counter at {:?}", contract_address);

        let counter_abi: abi::Abi = serde_json::from_str(counter_abi_str)?;
        let counter = Contract::new(contract_address, counter_abi, Arc::new(provider.clone()));

        let initial_value: U256 = counter
            .method("count", ())?
            .call()
            .await
            .context("Failed to get initial value")?;

        // 4. Increment (fire-and-forget)
        let call_nonce = nonce_manager.next().await?;
        let increment_data = counter.encode("increment", ())?;

        let increment_tx = TransactionRequest::new()
            .to(contract_address)
            .data(increment_data)
            .gas(call_gas_limit)
            .gas_price(gas_price)
            .nonce(call_nonce)
            .from(address);

        let pending_increment = client.send_transaction(increment_tx, None).await;

        match pending_increment {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!(
                    "Counter deployed at {:?}, initial count: {}, increment submitted (tx: {:?})",
                    contract_address,
                    initial_value,
                    pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("Counter increment submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit Counter increment tx: {}", e),
                    tx_hash: None,
                })
            },
        }
    }
}
