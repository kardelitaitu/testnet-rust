use crate::tasks::{TaskContext, TaskResult, TempoTask};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;

pub struct ClaimFaucetTask;

#[async_trait]
impl TempoTask for ClaimFaucetTask {
    fn name(&self) -> &str {
        "02_claim_faucet"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let address = ctx.wallet.address();

        // Custom RPC call: tempo_fundAddress
        // Params: [address]
        let params = [address];

        // Return type can be a single hash string or an array of strings
        let res: serde_json::Value = ctx.provider.request("tempo_fundAddress", params).await?;

        let mut tx_hashes = Vec::new();

        if let Some(arr) = res.as_array() {
            for v in arr {
                if let Some(s) = v.as_str() {
                    tx_hashes.push(s.to_string());
                }
            }
        } else if let Some(s) = res.as_str() {
            tx_hashes.push(s.to_string());
        }

        if tx_hashes.is_empty() {
            // It might return null or false if rate limited?
            // But usually it errors.
            return Ok(TaskResult {
                success: false,
                message: "Faucet returned no transaction hashes".to_string(),
                tx_hash: None,
            });
        }

        let first_hash = tx_hashes[0].clone();
        let _count = tx_hashes.len();

        Ok(TaskResult {
            success: true,
            message: format!("    Tx: {}", first_hash),
            tx_hash: Some(first_hash),
        })
    }
}
