use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct TimedInteractionTask;

impl TimedInteractionTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for TimedInteractionTask {
    fn name(&self) -> &str {
        "23_timedInteraction"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;

        let l1_block_address: Address = "0x4200000000000000000000000000000000000015"
            .parse()
            .context("Invalid L1Block address")?;

        let abi_json = r#"[
            {"type":"function","name":"number()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]},
            {"type":"function","name":"timestamp()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]},
            {"type":"function","name":"basefee()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]},
            {"type":"function","name":"l1BaseFee()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(abi_json)?;
        let contract = Contract::new(l1_block_address, abi, Arc::new(provider.clone()));

        let block_number: U256 = contract
            .method("number", ())?
            .call()
            .await
            .context("Failed to get block number")?;
        let block_timestamp: U256 = contract
            .method("timestamp", ())?
            .call()
            .await
            .context("Failed to get timestamp")?;

        let timestamp_secs = block_timestamp.as_u64();
        let formatted_time = timestamp_secs.to_string();

        let base_fee_eth = if let Ok(base_fee) = contract.method("basefee", ())?.call().await {
            let eth = ethers::utils::format_units::<U256, _>(base_fee, "ether")
                .unwrap_or_else(|_| base_fee.to_string());
            format!("{} ETH", eth)
        } else if let Ok(l1_base_fee) = contract.method("l1BaseFee", ())?.call().await {
            let eth = ethers::utils::format_units::<U256, _>(l1_base_fee, "ether")
                .unwrap_or_else(|_| l1_base_fee.to_string());
            format!("{} ETH (L1)", eth)
        } else {
            "N/A".to_string()
        };

        Ok(TaskResult {
            success: true,
            message: format!(
                "Block #{} at {} (timestamp: {}), base fee: {}",
                block_number, formatted_time, timestamp_secs, base_fee_eth
            ),
            tx_hash: None,
        })
    }
}
