use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct GasStipendTask;

impl GasStipendTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for GasStipendTask {
    fn name(&self) -> &str {
        "53_gasStipend"
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
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(Arc::new(provider.clone()), address);

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());

        // 3. Deploy gas stipend contract
        let stipend_bytecode = "0x6080604052348015600e575f5ffd5b506102148061001c5f395ff3fe608060405234801561000f575f5ffd5b5060043610610029575f3560e01c8063bc568ec41461002d575b5f5ffd5b610047600480360381019061004291906100fb565b61005e565b6040516100559291906101b0565b60405180910390f35b5f6060825a10156100a9575f6040518060400160405280600e81526020017f4e6f7420656e6f75676820676173000000000000000000000000000000000000815250915091506100bf565b600160405180602001604052805f815250915091505b915091565b5f5ffd5b5f819050919050565b6100da816100c8565b81146100e4575f5ffd5b50565b5f813590506100f5816100d1565b92915050565b5f602082840312156101105761010f6100c4565b5b5f61011d848285016100e7565b91505092915050565b5f8115159050919050565b61013a81610126565b82525050565b5f81519050919050565b5f82825260208201905092915050565b8281835e5f83830152505050565b5f601f19601f8301169050919050565b5f61018282610140565b61018c818561014a565b935061019c81856020860161015a565b6101a581610168565b840191505092915050565b5f6040820190506101c35f830185610131565b81810360208301526101d58184610178565b9050939250505056fea26469706673582212208e11a5da5ad4eecbe429ff084c31b6165c372497a2b4fef337b92620bcb516d964736f6c63430008210033";

        let deploy_data = crate::utils::strip_push0(&hex::decode(stipend_bytecode.trim_start_matches("0x")).unwrap());

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
                        receipt.contract_address.context("No contract address in receipt")?
                    },
                    _ => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("GasStipend deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    },
                }
            },
            Err(e) => {
                debug!("GasStipend deploy submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit GasStipend deploy tx: {}", e),
                    tx_hash: None,
                });
            },
        };

        debug!("Deployed GasStipend at {:?}", contract_address);

        let stipend_abi_json = r#"[
            {"type":"function","name":"callWithGas(uint256)","stateMutability":"nonpayable","inputs":[{"name":"gasAmount","type":"uint256"}],"outputs":[{"name":"success","type":"bool"},{"name":"data","type":"bytes"}]}
        ]"#;
        let stipend_abi: abi::Abi = serde_json::from_str(stipend_abi_json)?;
        let stipend_contract = Contract::new(contract_address, stipend_abi, Arc::new(provider.clone()));

        // 4. callWithGas (fire-and-forget)
        let call_nonce = nonce_manager.next().await?;
        let call_data = stipend_contract.encode("callWithGas", (U256::from(50000),))?;

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
                    "GasStipend deployed at {:?}, callWithGas(50000) submitted (tx: {:?})",
                    contract_address,
                    pending.tx_hash()
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("GasStipend call submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit GasStipend call tx: {}", e),
                    tx_hash: None,
                })
            },
        }
    }
}
