use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct AssertFailTask;

impl AssertFailTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for AssertFailTask {
    fn name(&self) -> &str {
        "47_assertFail"
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

        // 3. Deploy assert/require test contract
        let assert_bytecode = "0x6080604052348015600e575f5ffd5b506101e98061001c5f395ff3fe608060405234801561000f575f5ffd5b506004361061004a575f3560e01c8063209652551461004e5780633fa4f2451461006c578063761da2d51461008a578063c4c0c46f146100a6575b5f5ffd5b6100566100c2565b6040516100639190610114565b60405180910390f35b6100746100ca565b6040516100819190610114565b60405180910390f35b6100a4600480360381019061009f919061015b565b6100cf565b005b6100c060048036038101906100bb919061015b565b6100e3565b005b5f5f54905090565b5f5481565b805f819055505f81036100e0575f5ffd5b50565b805f819055505f81036100f9576100f8610186565b5b50565b5f819050919050565b61010e816100fc565b82525050565b5f6020820190506101275f830184610105565b92915050565b5f5ffd5b61013a816100fc565b8114610144575f5ffd5b50565b5f8135905061015581610131565b92915050565b5f602082840312156101705761016f61012d565b5b5f61017d84828501610147565b91505092915050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52600160045260245ffdfea2646970667358221220c515b3db4a3c6aa8e2664f670421c17f1f8ba475385be1010c22390555cfac6064736f6c63430008210033";

        let deploy_data = crate::utils::strip_push0(
            &hex::decode(assert_bytecode.trim_start_matches("0x")).unwrap(),
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
                            message: format!("AssertFail deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    }
                }
            }
            Err(e) => {
                debug!("AssertFail deploy submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit AssertFail deploy tx: {}", e),
                    tx_hash: None,
                });
            }
        };

        debug!("Deployed AssertFail at {:?}", contract_address);

        let assert_abi_json = r#"[
            {"type":"function","name":"assertCheck(uint256)","stateMutability":"nonpayable","inputs":[{"name":"value","type":"uint256"}],"outputs":[]},
            {"type":"function","name":"getValue","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;
        let assert_abi: abi::Abi = serde_json::from_str(assert_abi_json)?;
        let assert_contract =
            Contract::new(contract_address, assert_abi, Arc::new(provider.clone()));

        let value: U256 = assert_contract
            .method("getValue", ())?
            .call()
            .await
            .context("Failed to get value")?;

        // 4. assertCheck (fire-and-forget) — pass 1 so it doesn't revert
        let call_nonce = nonce_manager.next().await?;
        let call_data = assert_contract.encode("assertCheck", (U256::from(1),))?;

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
                    "AssertFail deployed at {:?}, initial value: {}, assertCheck(1) submitted (tx: {:?})",
                    contract_address, value, pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("AssertFail call submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit AssertFail call tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
