use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct AnonymousEventTask;

impl AnonymousEventTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for AnonymousEventTask {
    fn name(&self) -> &str {
        "48_anonymousEvent"
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
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());

        // 3. Deploy anonymous event contract
        let event_bytecode = "0x6080604052348015600e575f5ffd5b506101838061001c5f395ff3fe608060405234801561000f575f5ffd5b5060043610610034575f3560e01c8063088f5397146100385780638130f89a14610054575b5f5ffd5b610052600480360381019061004d91906100fa565b610070565b005b61006e600480360381019061006991906100fa565b6100aa565b005b7fa7abb6db5a64d6f6d865cdd7cc2e4a0a49d3483ce9ec81b5bde62e1cd80ff0308160405161009f9190610134565b60405180910390a150565b806040516100b89190610134565b60405180910390a050565b5f5ffd5b5f819050919050565b6100d9816100c7565b81146100e3575f5ffd5b50565b5f813590506100f4816100d0565b92915050565b5f6020828403121561010f5761010e6100c3565b5b5f61011c848285016100e6565b91505092915050565b61012e816100c7565b82525050565b5f6020820190506101475f830184610125565b9291505056fea2646970667358221220fceeeebbb2efb92b44d97c67610fc4f5af03c464069f40da1e021cb5f999bbb664736f6c63430008210033";

        let deploy_data = crate::utils::strip_push0(
            &hex::decode(event_bytecode.trim_start_matches("0x")).unwrap(),
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
                            message: format!("AnonymousEvent deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    }
                }
            }
            Err(e) => {
                debug!(
                    "AnonymousEvent deploy submit failed, resyncing nonce: {}",
                    e
                );
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit AnonymousEvent deploy tx: {}", e),
                    tx_hash: None,
                });
            }
        };

        debug!("Deployed AnonymousEvent at {:?}", contract_address);

        let event_abi_json = r#"[
            {"type":"function","name":"emitAnonymous(uint256)","stateMutability":"nonpayable","inputs":[{"name":"value","type":"uint256"}],"outputs":[]}
        ]"#;
        let event_abi: abi::Abi = serde_json::from_str(event_abi_json)?;
        let event_contract = Contract::new(contract_address, event_abi, Arc::new(provider.clone()));

        // 4. emitAnonymous (fire-and-forget)
        let emit_nonce = nonce_manager.next().await?;
        let emit_data = event_contract.encode("emitAnonymous", (U256::from(42),))?;

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
                    "AnonymousEvent deployed at {:?}, emitAnonymous(42) submitted (tx: {:?})",
                    contract_address,
                    pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("AnonymousEvent emit submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit AnonymousEvent emit tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
