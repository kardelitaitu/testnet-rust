use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;

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
        let balance_eth =
            ethers::utils::format_units(balance, "ether").unwrap_or_else(|_| balance.to_string());
        tracing::debug!(target: "smart_main", "Wallet balance: {} ETH", balance_eth);

        let mut rng = OsRng;
        let percentage: f64 = if balance > U256::from(10_000_000_000_000_000_000u64) {
            rng.gen_range(1.0..2.0)
        } else if balance > U256::from(5_000_000_000_000_000_000u64) {
            rng.gen_range(0.5..1.0)
        } else if balance > U256::from(1_000_000_000_000_000_000u64) {
            rng.gen_range(0.1..0.5)
        } else {
            rng.gen_range(0.01..0.1)
        };

        let amount_wei = balance * U256::from((percentage * 100.0) as u64) / U256::from(100u64);
        let min_amount = U256::from(10_000_000_000_000u64);
        let amount_wei = amount_wei.max(min_amount);

        tracing::debug!(target: "smart_main", "Sending {}% of balance = {} wei", percentage, amount_wei);

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;

        let data = ethers::abi::encode(&[
            ethers::abi::Token::Address(recipient),
            ethers::abi::Token::Uint(amount_wei.into()),
        ]);

        let tx = Eip1559TransactionRequest::new()
            .to(recipient)
            .value(amount_wei)
            .data(data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        use ethers::middleware::SignerMiddleware;
        let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.clone()));
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        let amount_eth = ethers::utils::format_units(amount_wei, "ether")
            .unwrap_or_else(|_| amount_wei.to_string());

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)),
            message: format!(
                "Raw call: sent {} ETH to {:?} with calldata",
                amount_eth, recipient
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
