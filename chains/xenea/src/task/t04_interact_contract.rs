use crate::contracts::COUNTER_BYTECODE;
use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

pub struct XeneaInteractContractTask;

#[async_trait]
impl Task<TaskContext> for XeneaInteractContractTask {
    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let gas_price = U256::from(1_100_000_000u64);
        let deploy_gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;
        let call_gas_limit = 500_000u64;
        let estimated_gas = U256::from(deploy_gas_limit.as_u64() + call_gas_limit) * gas_price;

        // 1. Balance check
        let balance = provider.get_balance(address, None).await?;
        if balance < estimated_gas {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient funds: have {} wei, need {} wei",
                    balance, estimated_gas
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

        // 3. Deploy Counter (wait for address)
        let bytecode = ethers::utils::hex::decode(COUNTER_BYTECODE)?;
        let deploy_nonce = nonce_manager.next().await?;
        let deploy_tx = TransactionRequest::new()
            .from(address)
            .data(Bytes::from(bytecode))
            .gas(deploy_gas_limit)
            .gas_price(gas_price)
            .nonce(deploy_nonce);

        let pending_deploy = client.send_transaction(deploy_tx, None).await;
        let counter_address = match pending_deploy {
            Ok(pending) => {
                let tx_hash = format!("{:?}", pending.tx_hash());
                match pending.await {
                    Ok(Some(receipt)) if receipt.status == Some(U64::from(1)) => {
                        receipt.contract_address.context("No contract address")?
                    }
                    _ => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("Counter deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    }
                }
            }
            Err(e) => {
                debug!(
                    "InteractContract deploy submit failed, resyncing nonce: {}",
                    e
                );
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit Counter deploy tx: {}", e),
                    tx_hash: None,
                });
            }
        };

        debug!("Deployed Counter at {:?}", counter_address);

        // Log to DB
        if let Some(db) = &ctx.db {
            let _ = db
                .log_counter_contract_creation(
                    &format!("{:?}", address),
                    &format!("{:?}", counter_address),
                    ctx.config.chain_id,
                )
                .await;
        }

        // 4. Call increment() (fire-and-forget)
        let counter_abi = r#"[{"type":"function","name":"increment","stateMutability":"nonpayable","inputs":[],"outputs":[]},{"type":"function","name":"getCount","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]}]"#;
        let abi: abi::Abi = serde_json::from_str(counter_abi)?;
        let contract = Contract::new(counter_address, abi, Arc::new(provider.clone()));

        let call_nonce = nonce_manager.next().await?;
        let call_data = contract.encode("increment", ())?;

        let call_tx = TransactionRequest::new()
            .to(counter_address)
            .data(call_data)
            .gas(call_gas_limit)
            .gas_price(gas_price)
            .nonce(call_nonce)
            .from(address);

        let pending_call = client.send_transaction(call_tx, None).await;

        match pending_call {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!(
                    "Counter deployed at {:?}, increment() submitted (tx: {:?})",
                    counter_address,
                    pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!(
                    "InteractContract call submit failed, resyncing nonce: {}",
                    e
                );
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit Counter increment tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }

    fn name(&self) -> &str {
        "04_xeneaInteractContract"
    }
}
