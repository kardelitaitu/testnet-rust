use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;

pub struct GasPriceZeroTask;

impl GasPriceZeroTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for GasPriceZeroTask {
    fn name(&self) -> &str {
        "54_gasPriceZero"
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

        let recipient_str = recipient_list.first().context("address.txt is empty")?;

        let recipient: Address = recipient_str
            .trim()
            .parse()
            .context(format!("Invalid address: {}", recipient_str))?;

        let amount_wei: u64 = 1_000_000_000;

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let zero_priority_fee = U256::from(0);

        let gas_limit = crate::utils::gas::GasManager::LIMIT_TRANSFER;

        let client = std::sync::Arc::new(SignerMiddleware::new(
            std::sync::Arc::new(provider.clone()),
            wallet.clone(),
        ));

        let amount_eth = ethers::utils::format_units(amount_wei, "ether")
            .unwrap_or_else(|_| amount_wei.to_string());

        let tx = Eip1559TransactionRequest::new()
            .to(recipient)
            .value(amount_wei)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(zero_priority_fee)
            .from(address);

        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        let priority_fee_display = if priority_fee == U256::from(0) {
            "0 (zero)"
        } else {
            "normal"
        };

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)),
            message: format!(
                "Gas Price Zero: Sent {} ETH with priority fee: {}. Tx: {:?}",
                amount_eth, priority_fee_display, receipt.transaction_hash
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
