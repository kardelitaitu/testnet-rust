use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

pub struct UUPSProxyTask;

impl UUPSProxyTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for UUPSProxyTask {
    fn name(&self) -> &str {
        "37_uupsProxy"
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

        let implementation_bytecode = "608060405260018055348015601357600080fd5b5060405161021e38038061021e8339810160408190526030916037565b600055604f565b600060208284031215604857600080fd5b5051919050565b6101c08061005e6000396000f3fe608060405234801561001057600080fd5b506004361061007d5760003560e01c806352d1902d1161005b57806352d1902d146100d457806354fd4d50146100fa57806355241077146101035780635c60da1b1461011657600080fd5b806320965255146100825780633659cfe6146100995780633fa4f245146100cb575b600080fd5b6000545b6040519081526020015b60405180910390f35b6100c96100a7366004610141565b600280546001600160a01b0319166001600160a01b0392909216919091179055565b005b61008660005481565b7fc5f16f0fcc639fa48a6947836d9850f504798523bf8c9a3a87d5876cf622bcf7610086565b61008660015481565b6100c9610111366004610171565b600055565b600254610129906001600160a01b031681565b6040516001600160a01b039091168152602001610090565b60006020828403121561015357600080fd5b81356001600160a01b038116811461016a57600080fd5b9392505050565b60006020828403121561018357600080fd5b503591905056fea2646970667358221220383dd5d8a7af200405a518702c65fd9901581130e05268926b2584a59136ffdc64736f6c63430008210033";

        let mut rng = OsRng;
        let salt: u64 = rng.gen();

        let clean_bytecode = implementation_bytecode.trim().trim_start_matches("0x");
        let mut impl_bytecode_vec =
            hex::decode(clean_bytecode).context("Failed to decode bytecode")?;

        // Append constructor args (uint256)
        let encoded_args = ethers::abi::encode(&[ethers::abi::Token::Uint(U256::from(salt))]);
        impl_bytecode_vec.extend(encoded_args);

        let tx = TransactionRequest::new()
            .data(impl_bytecode_vec)
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
                                "UUPS implementation deployed at {:?} (tx: {:?})",
                                deployed_addr, tx_hash
                            ),
                            tx_hash: Some(tx_hash),
                        })
                    }
                    Ok(Some(_)) => {
                        let _ = nonce_manager.resync().await;
                        Ok(TaskResult {
                            success: false,
                            message: format!("UUPS implementation deploy reverted (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        })
                    }
                    Ok(None) => {
                        let _ = nonce_manager.resync().await;
                        Ok(TaskResult {
                            success: false,
                            message: format!("UUPS implementation deploy receipt unavailable (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        })
                    }
                    Err(e) => {
                        debug!("UUPSProxy deploy receipt failed: {}", e);
                        let _ = nonce_manager.resync().await;
                        Ok(TaskResult {
                            success: false,
                            message: format!("UUPS implementation deploy receipt error: {}", e),
                            tx_hash: Some(tx_hash),
                        })
                    }
                }
            }
            Err(e) => {
                debug!("UUPSProxy tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit UUPS implementation deploy tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
