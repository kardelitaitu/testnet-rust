use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

pub struct RevertWithReasonTask;

impl RevertWithReasonTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for RevertWithReasonTask {
    fn name(&self) -> &str {
        "46_revertWithReason"
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

        // 3. Deploy revert test contract
        let revert_bytecode = "0x6080604052348015600e575f5ffd5b506103b48061001c5f395ff3fe608060405234801561000f575f5ffd5b506004361061004a575f3560e01c80631865c57d1461004e57806346fc4bb11461006c578063c19d93fb14610076578063ee781e4c14610094575b5f5ffd5b6100566100b0565b6040516100639190610151565b60405180910390f35b6100746100b8565b005b61007e6100ea565b60405161008b9190610151565b60405180910390f35b6100ae60048036038101906100a991906102b7565b6100ef565b005b5f5f54905090565b6040517f8b16d98400000000000000000000000000000000000000000000000000000000815260040160405180910390fd5b5f5481565b5f8151148190610135576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040161012c919061035e565b60405180910390fd5b5050565b5f819050919050565b61014b81610139565b82525050565b5f6020820190506101645f830184610142565b92915050565b5f604051905090565b5f5ffd5b5f5ffd5b5f5ffd5b5f5ffd5b5f601f19601f8301169050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52604160045260245ffd5b6101c982610183565b810181811067ffffffffffffffff821117156101e8576101e7610193565b5b80604052505050565b5f6101fa61016a565b905061020682826101c0565b919050565b5f67ffffffffffffffff82111561022557610224610193565b5b61022e82610183565b9050602081019050919050565b828183375f83830152505050565b5f61025b6102568461020b565b6101f1565b9050828152602081018484840111156102775761027661017f565b5b61028284828561023b565b509392505050565b5f82601f83011261029e5761029d61017b565b5b81356102ae848260208601610249565b91505092915050565b5f602082840312156102cc576102cb610173565b5b5f82013567ffffffffffffffff8111156102e9576102e8610177565b5b6102f58482850161028a565b91505092915050565b5f81519050919050565b5f82825260208201905092915050565b8281835e5f83830152505050565b5f610330826102fe565b61033a8185610308565b935061034a818560208601610318565b61035381610183565b840191505092915050565b5f6020820190508181035f8301526103768184610326565b90509291505056fea2646970667358221220f607f87aef195abcadc977a682c42599132afa088ec63b273c92011026df807164736f6c63430008210033";

        let deploy_data = crate::utils::strip_push0(&hex::decode(&revert_bytecode.trim_start_matches("0x")).unwrap());

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
                    Ok(Some(receipt)) if receipt.status == Some(U64::from(1)) => {
                        receipt
                            .contract_address
                            .context("No contract address in receipt")?
                    }
                    _ => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("RevertWithReason deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    }
                }
            }
            Err(e) => {
                debug!("RevertWithReason deploy submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit RevertWithReason deploy tx: {}", e),
                    tx_hash: None,
                });
            }
        };

        debug!("Deployed RevertWithReason at {:?}", contract_address);

        let revert_abi_json = r#"[
            {"type":"function","name":"revertWithMessage(string)","stateMutability":"nonpayable","inputs":[{"name":"message","type":"string"}],"outputs":[]},
            {"type":"function","name":"getState","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;
        let revert_abi: abi::Abi = serde_json::from_str(revert_abi_json)?;
        let revert_contract = Contract::new(contract_address, revert_abi, Arc::new(provider.clone()));

        let state: U256 = revert_contract
            .method("getState", ())?
            .call()
            .await
            .context("Failed to get state")?;

        // 4. revertWithMessage (fire-and-forget)
        let call_nonce = nonce_manager.next().await?;
        let call_data = revert_contract.encode("revertWithMessage", (String::from("test revert reason"),))?;

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
                    "RevertWithReason deployed at {:?}, state: {}, revertWithMessage submitted (tx: {:?})",
                    contract_address, state, pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("RevertWithReason call submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit RevertWithReason call tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
