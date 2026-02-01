use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use std::fs;

pub struct SimpleEthTransferTask;

impl SimpleEthTransferTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for SimpleEthTransferTask {
    fn name(&self) -> &str {
        "02_simpleEthTransfer"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let recipients = fs::read_to_string("address.txt").context("Failed to read address.txt")?;
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

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_TRANSFER;

        // Check Balance
        let balance = provider.get_balance(address, None).await?;

        // Transfer 3% of current balance
        let amount_wei = (balance * U256::from(3u64) / U256::from(100u64)).as_u64();
        let amount_eth = ethers::utils::format_units(amount_wei, "ether")
            .unwrap_or_else(|_| amount_wei.to_string());

        let required_val = amount_wei + (gas_limit.as_u64() * max_fee.as_u64()); // Approx check
        if balance.as_u64() < required_val {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient funds. Have {} Wei, need approx {} Wei",
                    balance, required_val
                ),
                tx_hash: None,
            });
        }

        let tx = Eip1559TransactionRequest::new()
            .to(recipient)
            .value(amount_wei)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        use ethers::middleware::SignerMiddleware;
        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)),
            message: format!("Sent {} ETH to {:?}", amount_eth, recipient),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
