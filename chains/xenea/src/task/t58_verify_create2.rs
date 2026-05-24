use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct VerifyCreate2Task;

impl VerifyCreate2Task {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for VerifyCreate2Task {
    fn name(&self) -> &str {
        "58_verifyCreate2"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();
        let gas_price = U256::from(1_100_000_000u64);

        let deploy_gas_limit = 2_000_000u64;
        let call_gas_limit = 500_000u64;
        let estimated_gas = U256::from(deploy_gas_limit + call_gas_limit) * gas_price;

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

        // 3. Deploy SimpleFactory
        let factory_bytecode = "6080604052348015600f57600080fd5b5061020f8061001f6000396000f3fe608060405234801561001057600080fd5b506004361061002b5760003560e01c806361ff715f14610030575b600080fd5b61004361003e366004610116565b61005f565b6040516001600160a01b03909116815260200160405180910390f35b6000828251602084016000f590506001600160a01b0381166100b85760405162461bcd60e51b815260206004820152600e60248201526d10dc99585d194c8819985a5b195960921b604482015260640160405180910390fd5b604080516001600160a01b0383168152602081018590527fb03c53b28e78a88e31607a27e1fa48234dce28d5d9d9ec7b295aeb02e674a1e1910160405180910390a192915050565b634e487b7160e01b600052604160045260246000fd5b6000806040838503121561012957600080fd5b82359150602083013567ffffffffffffffff81111561014757600080fd5b8301601f8101851361015857600080fd5b803567ffffffffffffffff81111561017257610172610100565b604051601f8201601f19908116603f0116810167ffffffffffffffff811182821017156101a1576101a1610100565b6040528181528282016020018710156101b957600080fd5b81602084016020830137600060208383010152809350505050925092905056fea26469706673582212203d752ff8928077cad5caebcfb0833f2e92dd8a67636e26a88b59280bc1e801cc64736f6c63430008210033";
        let factory_bytes = hex::decode(factory_bytecode)?;

        let deploy_nonce = nonce_manager.next().await?;
        let deploy_tx = TransactionRequest::new()
            .data(factory_bytes)
            .gas(deploy_gas_limit)
            .gas_price(gas_price)
            .nonce(deploy_nonce)
            .from(address);

        let pending_deploy = client.send_transaction(deploy_tx, None).await;
        let factory_address = match pending_deploy {
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
                            message: format!("SimpleFactory deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    }
                }
            }
            Err(e) => {
                debug!("VerifyCreate2 deploy submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit SimpleFactory deploy tx: {}", e),
                    tx_hash: None,
                });
            }
        };

        debug!("SimpleFactory deployed at: {:?}", factory_address);

        // 4. Call deploy() (fire-and-forget)
        let child_bytecode = hex::decode("600060205260206020f3")?;
        let abi_json = r#"[{"inputs":[{"internalType":"uint256","name":"salt","type":"uint256"},{"internalType":"bytes","name":"bytecode","type":"bytes"}],"name":"deploy","outputs":[{"internalType":"address","name":"addr","type":"address"}],"stateMutability":"nonpayable","type":"function"}]"#;
        let abi: abi::Abi = serde_json::from_str(abi_json)?;
        let contract = Contract::new(factory_address, abi, Arc::new(provider.clone()));

        let salt = U256::from(12345);
        let call_nonce = nonce_manager.next().await?;
        let val_data = contract.encode("deploy", (salt, Bytes::from(child_bytecode)))?;

        let call_tx = TransactionRequest::new()
            .to(factory_address)
            .data(val_data)
            .gas(call_gas_limit)
            .gas_price(gas_price)
            .nonce(call_nonce)
            .from(address);

        let pending_call = client.send_transaction(call_tx, None).await;

        match pending_call {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!(
                    "SimpleFactory deployed at {:?}, CREATE2 deploy submitted with salt {} (tx: {:?})",
                    factory_address, salt, pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("VerifyCreate2 call submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit CREATE2 deploy tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
