use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use rand::Rng;
use std::sync::Arc;

pub struct GasPriceTestTask;

impl GasPriceTestTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for GasPriceTestTask {
    fn name(&self) -> &str {
        "20_gasPriceTest"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let recipients =
            std::fs::read_to_string("address.txt").context("Failed to read address.txt")?;
        let recipient_list: Vec<&str> = recipients
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();

        let recipient_str = recipient_list
            .choose(&mut OsRng)
            .context("address.txt is empty")?;

        let recipient: Address = recipient_str
            .trim()
            .parse()
            .context(format!("Invalid address in address.txt: {}", recipient_str))?;

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
        let min_amount = U256::from(5_000_000_000_000u64);
        let amount_wei = amount_wei.max(min_amount);

        let amount_eth = ethers::utils::format_units(amount_wei, "ether")
            .unwrap_or_else(|_| amount_wei.to_string());

        tracing::debug!(target: "smart_main", "Sending {}% of balance = {} wei", percentage, amount_wei);

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_TRANSFER;
        let test_max_fee: U256 = max_fee * 2;

        let tx = Eip1559TransactionRequest::new()
            .to(recipient)
            .value(amount_wei)
            .gas(gas_limit)
            .max_fee_per_gas(test_max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        use ethers::middleware::SignerMiddleware;
        let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.clone()));
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        let priority_fee_gwei = ethers::utils::format_units(priority_fee, "gwei")
            .unwrap_or_else(|_| priority_fee.to_string());
        let max_fee_gwei = ethers::utils::format_units(test_max_fee, "gwei")
            .unwrap_or_else(|_| test_max_fee.to_string());

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)),
            message: format!(
                "Gas price test: {} ETH to {:?} (priority: {} gwei, max: {} gwei)",
                amount_eth, recipient, priority_fee_gwei, max_fee_gwei
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
