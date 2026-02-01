use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use std::sync::Arc;

pub struct BatchApproveTask;

impl BatchApproveTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for BatchApproveTask {
    fn name(&self) -> &str {
        "33_batchApprove"
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

        let spender_str = recipient_list
            .choose(&mut OsRng)
            .context("address.txt is empty")?;

        let spender: Address = spender_str
            .trim()
            .parse()
            .context(format!("Invalid spender: {}", spender_str))?;

        let tokens = [
            ("USDC", "0x8a93d247134d91e0de6f96547cb0204e5be8e5d8"),
            ("USDT", "0x40918ba7f132e0acba2ce4de4c4baf9bd2d7d849"),
        ];

        let amount: u128 = 500_000_000; // 500k USDC (6 decimals)
        let amount_formatted =
            ethers::utils::format_units(amount, 6u32).unwrap_or_else(|_| amount.to_string());

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;

        let client = std::sync::Arc::new(SignerMiddleware::new(
            std::sync::Arc::new(provider.clone()),
            wallet.clone(),
        ));

        let mut tx_hashes = Vec::new();
        let mut successes = 0;

        for (_, token_addr) in &tokens {
            let token_address: Address = token_addr.parse().context("Invalid token")?;

            let abi_json = r#"[
                {"type":"function","name":"approve(address,uint256)","stateMutability":"nonpayable","inputs":[{"name":"spender","type":"address"},{"name":"amount","type":"uint256"}],"outputs":[{"name":"","type":"bool"}]}
            ]"#;

            let abi: abi::Abi = serde_json::from_str(abi_json)?;
            let contract = Contract::new(token_address, abi, Arc::new(provider.clone()));

            let data = contract.encode("approve", (spender, amount))?;

            let tx = Eip1559TransactionRequest::new()
                .to(token_address)
                .data(data)
                .gas(gas_limit)
                .max_fee_per_gas(max_fee)
                .max_priority_fee_per_gas(priority_fee)
                .from(address);

            let mut send_result = client.send_transaction(tx, None).await;
            let receipt_result = match send_result {
                Ok(ref mut pending) => pending.await.map_err(|e| anyhow::anyhow!("{:?}", e)),
                Err(ref e) => Err(anyhow::anyhow!("Send failed: {:?}", e)),
            };

            match (send_result, receipt_result) {
                (Ok(_), Ok(Some(receipt))) => {
                    tx_hashes.push(format!("{:?}", receipt.transaction_hash));
                    if receipt.status == Some(U64::from(1)) {
                        successes += 1;
                    }
                }
                (_, _) => tx_hashes.push("failed".to_string()),
            }
        }

        Ok(TaskResult {
            success: successes == 2,
            message: format!(
                "Batch approved {} tokens for {:?} ({}/{} successful)",
                amount_formatted,
                spender,
                successes,
                tokens.len()
            ),
            tx_hash: Some(tx_hashes.join(",")),
        })
    }
}
