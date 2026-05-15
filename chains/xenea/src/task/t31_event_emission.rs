use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

pub struct EventEmissionTask;

impl EventEmissionTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for EventEmissionTask {
    fn name(&self) -> &str {
        "31_eventEmission"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;
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

        // 2. Send 1% of balance as deposit value
        let available = balance - estimated_gas;
        let amount_wei = available / U256::from(100u64);

        // 3. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );
        let nonce = nonce_manager.next().await?;

        let weth_abi_json = r#"[
            {"type":"function","name":"deposit()","stateMutability":"payable","inputs":[],"outputs":[]},
            {"type":"event","name":"Deposit(address indexed,uint256)","inputs":[{"name":"dst","type":"address","indexed":true},{"name":"wad","type":"uint256"}],"anonymous":false}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(weth_abi_json)?;
        let weth_address: Address = "0x4200000000000000000000000000000000000006"
            .parse()
            .context("Invalid WETH address")?;

        let contract = Contract::new(weth_address, abi, Arc::new(provider.clone()));
        let data = contract.encode("deposit", ())?;

        let tx = TransactionRequest::new()
            .to(weth_address)
            .data(data)
            .value(amount_wei)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .from(address);

        // 4. Send (fire-and-forget)
        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => {
                let amount_eth = ethers::utils::format_units(amount_wei, "ether")
                    .unwrap_or_else(|_| amount_wei.to_string());
                Ok(TaskResult {
                    success: true,
                    message: format!(
                        "WETH deposit submitted: {} TXENE (tx: {:?})",
                        amount_eth,
                        pending.tx_hash()
                    ),
                    tx_hash: Some(format!("{:?}", pending.tx_hash())),
                })
            }
            Err(e) => {
                debug!("EventEmission tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit WETH deposit tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
