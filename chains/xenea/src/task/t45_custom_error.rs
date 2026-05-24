use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct CustomErrorTestTask;

impl CustomErrorTestTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for CustomErrorTestTask {
    fn name(&self) -> &str {
        "45_customErrorTest"
    }

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

        // 3. Deploy custom error contract
        let error_bytecode = "0x6080604052348015600e575f5ffd5b506101b58061001c5f395ff3fe608060405234801561000f575f5ffd5b506004361061003f575f3560e01c80633bc5de301461004357806373d4a13a146100615780639467c6ed1461007f575b5f5ffd5b61004b61009b565b6040516100589190610102565b60405180910390f35b6100696100a3565b6040516100769190610102565b60405180910390f35b61009960048036038101906100949190610154565b6100a8565b005b5f5f54905090565b5f5481565b80156100e0576040517f4e7254d600000000000000000000000000000000000000000000000000000000815260040160405180910390fd5b602d5f8190555050565b5f819050919050565b6100fc816100ea565b82525050565b5f6020820190506101155f8301846100f3565b92915050565b5f5ffd5b5f8115159050919050565b6101338161011f565b811461013d575f5ffd5b50565b5f8135905061014e8161012a565b92915050565b5f602082840312156101695761016861011b565b5b5f61017684828501610140565b9150509291505056fea26469706673582212201dbaaf7a107333b9b5895808c42d4a4c6a234ffd99ca2388d43581460d29550764736f6c63430008210033";

        let deploy_data = crate::utils::strip_push0(
            &hex::decode(error_bytecode.trim_start_matches("0x")).unwrap(),
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
                            message: format!("CustomError deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    }
                }
            }
            Err(e) => {
                debug!("CustomError deploy submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit CustomError deploy tx: {}", e),
                    tx_hash: None,
                });
            }
        };

        debug!("Deployed CustomError at {:?}", contract_address);

        let error_abi_json = r#"[
            {"type":"function","name":"testError(bool)","stateMutability":"nonpayable","inputs":[{"name":"shouldFail","type":"bool"}],"outputs":[]},
            {"type":"function","name":"getData","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;
        let error_abi: abi::Abi = serde_json::from_str(error_abi_json)?;
        let error_contract = Contract::new(contract_address, error_abi, Arc::new(provider.clone()));

        let data: U256 = error_contract
            .method("getData", ())?
            .call()
            .await
            .context("Failed to get data")?;

        // 4. testError (fire-and-forget) — call with false so it doesn't revert
        let call_nonce = nonce_manager.next().await?;
        let call_data = error_contract.encode("testError", (false,))?;

        let call_tx = TransactionRequest::new()
            .to(contract_address)
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
                    "CustomError deployed at {:?}, initial data: {}, testError(false) submitted (tx: {:?})",
                    contract_address, data, pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("CustomError call submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit CustomError testError tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
