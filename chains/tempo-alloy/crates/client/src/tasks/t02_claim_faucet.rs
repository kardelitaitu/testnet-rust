//! Claim Faucet Task
//!
//! Claims test tokens from Tempo's native faucet via custom RPC call.

use crate::tasks::prelude::*;
use anyhow::Result;
use async_trait::async_trait;

/// Claim tokens from the Tempo faucet
#[derive(Debug, Clone, Default)]
pub struct ClaimFaucetTask;

impl ClaimFaucetTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for ClaimFaucetTask {
    fn name(&self) -> &'static str {
        "02_claim_faucet"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let address = ctx.address();

        // Call custom Tempo RPC method: tempo_fundAddress
        let params = serde_json::json!([address.to_string()]);

        let res: serde_json::Value = ctx
            .client
            .provider
            .request("tempo_fundAddress", params)
            .await
            .context("Failed to call faucet")?;

        let mut tx_hashes = Vec::new();

        // Parse response (can be single string or array)
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
            return Ok(TaskResult {
                success: false,
                message: "Faucet returned no transaction hashes".to_string(),
                tx_hash: None,
            });
        }

        let first_hash = tx_hashes[0].clone();

        Ok(TaskResult {
            success: true,
            message: format!("Faucet claimed: {}", first_hash),
            tx_hash: Some(first_hash),
        })
    }
}
