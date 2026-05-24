use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct CalldataSizeTask;

impl CalldataSizeTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for CalldataSizeTask {
    fn name(&self) -> &str {
        "52_calldataSize"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let gas_price = U256::from(1_100_000_000u64);
        let deploy_gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;
        let store_gas_limit = 500_000u64;
        let estimated_gas = U256::from(deploy_gas_limit.as_u64() + store_gas_limit) * gas_price;

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

        // 3. Deploy calldata storage contract
        let calldata_bytecode = "0x6080604052348015600e575f5ffd5b506102b38061001c5f395ff3fe608060405234801561000f575f5ffd5b506004361061003f575f3560e01c80631b3012a314610043578063a4da229014610061578063ac5c85351461007f575b5f5ffd5b61004b61009b565b60405161005891906100d0565b60405180910390f35b6100696100a0565b60405161007691906100d0565b60405180910390f35b61009960048036038101906100949190610236565b6100a8565b005b5f5481565b5f5f54905090565b80805190602001205f8190555050565b5f819050919050565b6100ca816100b8565b82525050565b5f6020820190506100e35f8301846100c1565b92915050565b5f604051905090565b5f5ffd5b5f5ffd5b5f5ffd5b5f5ffd5b5f601f19601f8301169050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52604160045260245ffd5b61014882610102565b810181811067ffffffffffffffff8211171561016757610166610112565b5b80604052505050565b5f6101796100e9565b9050610185828261013f565b919050565b5f67ffffffffffffffff8211156101a4576101a3610112565b5b6101ad82610102565b9050602081019050919050565b828183375f83830152505050565b5f6101da6101d58461018a565b610170565b9050828152602081018484840111156101f6576101f56100fe565b5b6102018482856101ba565b509392505050565b5f82601f83011261021d5761021c6100fa565b5b813561022d8482602086016101c8565b91505092915050565b5f6020828403121561024b5761024a6100f2565b5b5f82013567ffffffffffffffff811115610268576102676100f6565b5b61027484828501610209565b9150509291505056fea26469706673582212201c68fa332f46ef2db292cf454bef9dab2cfaddf76d5c7f33bd22fcafe397b1c264736f6c63430008210033";

        let deploy_data = crate::utils::strip_push0(
            &hex::decode(calldata_bytecode.trim_start_matches("0x")).unwrap(),
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
                            message: format!("CalldataSize deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    }
                }
            }
            Err(e) => {
                debug!("CalldataSize deploy submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit CalldataSize deploy tx: {}", e),
                    tx_hash: None,
                });
            }
        };

        debug!("Deployed CalldataSize at {:?}", contract_address);

        let calldata_abi_json = r#"[
            {"type":"function","name":"storeData(bytes)","stateMutability":"nonpayable","inputs":[{"name":"data","type":"bytes"}],"outputs":[]}
        ]"#;
        let calldata_abi: abi::Abi = serde_json::from_str(calldata_abi_json)?;
        let calldata_contract =
            Contract::new(contract_address, calldata_abi, Arc::new(provider.clone()));

        // 4. storeData (fire-and-forget)
        let mut rng = OsRng;
        let mut large_calldata = vec![0u8; 512];
        for byte in large_calldata.iter_mut() {
            *byte = rng.gen();
        }

        let store_nonce = nonce_manager.next().await?;
        let store_data =
            calldata_contract.encode("storeData", Bytes::from(large_calldata.clone()))?;

        let store_tx = TransactionRequest::new()
            .to(contract_address)
            .data(store_data)
            .gas(store_gas_limit)
            .gas_price(gas_price)
            .nonce(store_nonce)
            .from(address);

        let pending_store = client.send_transaction(store_tx, None).await;

        match pending_store {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!(
                    "CalldataSize deployed at {:?}, storeData({} bytes) submitted (tx: {:?})",
                    contract_address,
                    large_calldata.len(),
                    pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("CalldataSize store submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit CalldataSize store tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
