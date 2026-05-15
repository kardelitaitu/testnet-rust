use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

pub struct ApproveTokenTask;

impl ApproveTokenTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for ApproveTokenTask {
    fn name(&self) -> &str {
        "14_approveTokenNative"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // Get random spender from address cache
        let spender = AddressCache::get_random().context("Failed to get random address")?;

        let amount = 1_000_000_000_000_000_000_000_000_000_000u128;

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;

        // 1. Native balance check
        let balance = provider.get_balance(address, None).await?;
        let estimated_cost = gas_limit * gas_price;
        if balance < estimated_cost {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient TXENE for gas: need {} Wei, have {} Wei",
                    estimated_cost, balance
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

        let abi_json = r#"[
            {"type":"function","name":"approve(address,uint256)","stateMutability":"nonpayable","inputs":[{"name":"spender","type":"address"},{"name":"amount","type":"uint256"}],"outputs":[{"name":"","type":"bool"}]},
            {"type":"function","name":"allowance(address,address)","stateMutability":"view","inputs":[{"name":"owner","type":"address"},{"name":"spender","type":"address"}],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(abi_json)?;
        let token_address: Address = "0x8a93d247134d91e0de6f96547cb0204e5be8e5d8"
            .parse()
            .context("Invalid token address")?;

        let contract = Contract::new(token_address, abi, Arc::new(provider.clone()));

        // 3. Encode and Send
        let data = contract.encode("approve", (spender, amount))?;

        let tx = TransactionRequest::new()
            .to(token_address)
            .data(data)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .from(address);

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!("Approve tx submitted for {:?}", spender),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("Approve tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit approve tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}

