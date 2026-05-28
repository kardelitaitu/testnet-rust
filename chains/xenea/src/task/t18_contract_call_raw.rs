use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct ContractCallRawTask;

impl ContractCallRawTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for ContractCallRawTask {
    fn name(&self) -> &str {
        "18_contractCallRaw"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // Get random recipient from address cache
        let recipient = AddressCache::get_random().context("Failed to get random address")?;

        let balance = provider.get_balance(address, None).await?;

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;
        let estimated_gas = gas_limit * gas_price;

        // 1. Balance check: need enough for gas + minimum transfer
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

        // 2. Send 1% of balance (after reserving gas)
        let available = balance - estimated_gas;
        let amount_wei = available / U256::from(100u64);

        // 3. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(Arc::new(provider.clone()), address);
        let nonce = nonce_manager.next().await?;

        // 4. Encode raw calldata: (recipient, amount)
        let data = ethers::abi::encode(&[
            ethers::abi::Token::Address(recipient),
            ethers::abi::Token::Uint(amount_wei),
        ]);

        let tx = TransactionRequest::new()
            .to(recipient)
            .value(amount_wei)
            .data(data)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .from(address);

        // 5. Send (fire-and-forget)
        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => {
                let amount_eth =
                    ethers::utils::format_units(amount_wei, "ether").unwrap_or_else(|_| amount_wei.to_string());
                Ok(TaskResult {
                    success: true,
                    message: format!("Raw call: sent {} TXENE to {:?} with calldata", amount_eth, recipient),
                    tx_hash: Some(format!("{:?}", pending.tx_hash())),
                })
            },
            Err(e) => {
                debug!("ContractCallRaw tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit raw call tx: {}", e),
                    tx_hash: None,
                })
            },
        }
    }
}
