use crate::contracts::COUNTER_BYTECODE;
use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

pub struct XeneaDeployContractTask;

#[async_trait]
impl Task<TaskContext> for XeneaDeployContractTask {
    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;
        let estimated_gas = gas_limit * gas_price;

        // 1. Balance check
        let balance = provider.get_balance(address, None).await?;
        if balance < estimated_gas {
            return Ok(TaskResult {
                success: false,
                message: format!("Insufficient funds: have {} wei, need {} wei", balance, estimated_gas),
                tx_hash: None,
            });
        }

        // 2. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(Arc::new(provider.clone()), address);
        let nonce = nonce_manager.next().await?;

        // 3. Deploy and wait for receipt to get contract address
        let bytecode = ethers::utils::hex::decode(COUNTER_BYTECODE)?;
        let tx = TransactionRequest::new()
            .from(address)
            .data(Bytes::from(bytecode))
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce);

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => {
                let tx_hash = format!("{:?}", pending.tx_hash());
                match pending.await {
                    Ok(Some(receipt)) if receipt.status == Some(U64::from(1)) => {
                        let contract_address = receipt.contract_address.context("No contract address in receipt")?;
                        Ok(TaskResult {
                            success: true,
                            message: format!("Counter deployed at {:?} (tx: {})", contract_address, tx_hash),
                            tx_hash: Some(tx_hash),
                        })
                    },
                    Ok(Some(_)) => {
                        let _ = nonce_manager.resync().await;
                        Ok(TaskResult {
                            success: false,
                            message: format!("Counter deploy reverted (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        })
                    },
                    Ok(None) => {
                        let _ = nonce_manager.resync().await;
                        Ok(TaskResult {
                            success: false,
                            message: format!("Counter deploy receipt unavailable (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        })
                    },
                    Err(e) => {
                        debug!("Counter deploy receipt failed: {}", e);
                        let _ = nonce_manager.resync().await;
                        Ok(TaskResult {
                            success: false,
                            message: format!("Counter deploy receipt error: {}", e),
                            tx_hash: Some(tx_hash),
                        })
                    },
                }
            },
            Err(e) => {
                debug!("DeployContract tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit Counter deploy tx: {}", e),
                    tx_hash: None,
                })
            },
        }
    }

    fn name(&self) -> &str {
        "03_xeneaDeployContract"
    }
}
