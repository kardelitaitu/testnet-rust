use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

pub struct TransparentProxyTask;

impl TransparentProxyTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for TransparentProxyTask {
    fn name(&self) -> &str {
        "38_transparentProxy"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;
        let estimated_gas = gas_limit * gas_price;

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
        let nonce = nonce_manager.next().await?;

        let impl_bytecode = "6080604052348015600f57600080fd5b506040516101f33803806101f3833981016040819052602c916050565b600080546001600160a01b0319166001600160a01b0392909216919091179055607e565b600060208284031215606157600080fd5b81516001600160a01b0381168114607757600080fd5b9392505050565b6101668061008d6000396000f3fe608060405234801561001057600080fd5b506004361061007d5760003560e01c80635c60da1b1161005b5780635c60da1b146100b75780636e9960c3146100e2578063aaf10f42146100f3578063f851a4401461010457600080fd5b806320965255146100825780633fa4f2451461009957806355241077146100a2575b600080fd5b6002545b6040519081526020015b60405180910390f35b61008660025481565b6100b56100b0366004610117565b600255565b005b6001546100ca906001600160a01b031681565b6040516001600160a01b039091168152602001610090565b6000546001600160a01b03166100ca565b6001546001600160a01b03166100ca565b6000546100ca906001600160a01b031681565b60006020828403121561012957600080fd5b503591905056fea26469706673582212209da41bf25da902b77696685abec48be665e2268d4c1ae1729442bf60960f400864736f6c63430008210033";

        let mut impl_data = hex::decode(&impl_bytecode.trim_start_matches("0x"))
            .context("Failed to decode transparent proxy bytecode")?;
        let encoded_admin = ethers::abi::encode(&[ethers::abi::Token::Address(address)]);
        impl_data.extend(encoded_admin);

        let tx = TransactionRequest::new()
            .data(impl_data)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .from(address);

        // 3. Send and wait for receipt to verify deploy
        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => {
                let tx_hash = format!("{:?}", pending.tx_hash());
                match pending.await {
                    Ok(Some(receipt)) if receipt.status == Some(U64::from(1)) => {
                        let deployed_addr = receipt.contract_address
                            .context("No contract address in receipt")?;
                        Ok(TaskResult {
                            success: true,
                            message: format!(
                                "Transparent proxy deployed at {:?} (tx: {:?})",
                                deployed_addr, tx_hash
                            ),
                            tx_hash: Some(tx_hash),
                        })
                    }
                    Ok(Some(_)) => {
                        let _ = nonce_manager.resync().await;
                        Ok(TaskResult {
                            success: false,
                            message: format!("Transparent proxy deploy reverted (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        })
                    }
                    Ok(None) => {
                        let _ = nonce_manager.resync().await;
                        Ok(TaskResult {
                            success: false,
                            message: format!("Transparent proxy deploy receipt unavailable (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        })
                    }
                    Err(e) => {
                        debug!("TransparentProxy deploy receipt failed: {}", e);
                        let _ = nonce_manager.resync().await;
                        Ok(TaskResult {
                            success: false,
                            message: format!("Transparent proxy deploy receipt error: {}", e),
                            tx_hash: Some(tx_hash),
                        })
                    }
                }
            }
            Err(e) => {
                debug!("TransparentProxy tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit transparent proxy deploy tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
