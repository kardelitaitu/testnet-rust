//! Claim Faucet Task
//!
//! Claims tokens from the Tempo testnet faucet.

use crate::tasks::prelude::*;
use alloy::rpc::types::TransactionRequest;
use anyhow::{Context, Result};
use async_trait::async_trait;

const FAUCET_ADDRESS: &str = "0x4200000000000000000000000000000000000019";

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
        let client = &ctx.client;
        let address = ctx.address();

        let mut data = hex::decode("4f9828f6000000000000000000000000").unwrap();
        data.extend_from_slice(address.as_slice());

        // Build base transaction
        let base_tx = TransactionRequest::default()
            .to(FAUCET_ADDRESS.parse().unwrap())
            .input(data.into())
            .from(address);

        // Send with retry logic for nonce errors
        let mut attempt = 0;
        let max_retries = 3;
        let pending = loop {
            match client.provider.send_transaction(base_tx.clone()).await {
                Ok(p) => break p,
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    attempt += 1;

                    if (err_str.contains("nonce too low") || err_str.contains("already known"))
                        && attempt < max_retries
                    {
                        tracing::warn!(
                            "Nonce error on faucet claim, attempt {}/{}, resetting cache...",
                            attempt,
                            max_retries
                        );

                        // Reset nonce cache and wait
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        continue;
                    } else {
                        return Err(e).context("Failed to send faucet claim transaction");
                    }
                }
            }
        };

        let tx_hash = pending.tx_hash().clone();

        Ok(TaskResult {
            success: true,
            message: format!("Faucet claim submitted: {:?}", tx_hash),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
