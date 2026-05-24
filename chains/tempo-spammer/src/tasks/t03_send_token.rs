//! Send Token Task
//!
//! Sends TIP-20 tokens using raw contract calls.

use crate::tasks::prelude::*;
use alloy::rpc::types::TransactionRequest;
use alloy_primitives::{Address, U256};
use anyhow::Result;
use async_trait::async_trait;
use rand::Rng;
use rand::prelude::SliceRandom;
use std::str::FromStr;

const SYSTEM_TOKENS: &[(&str, &str)] = &[
    ("PathUSD", "0x20C0000000000000000000000000000000000000"),
    ("AlphaUSD", "0x20c0000000000000000000000000000000000001"),
    ("BetaUSD", "0x20c0000000000000000000000000000000000002"),
    ("ThetaUSD", "0x20c0000000000000000000000000000000000003"),
];

#[derive(Debug, Clone, Default)]
pub struct SendTokenTask;

impl SendTokenTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for SendTokenTask {
    fn name(&self) -> &'static str {
        "03_send_token"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        let (token_name, token_addr_str) = {
            let mut rng = rand::thread_rng();
            SYSTEM_TOKENS
                .choose(&mut rng)
                .copied()
                .unwrap_or(("PathUSD", SYSTEM_TOKENS[0].1))
        };
        let token_address = Address::from_str(token_addr_str)?;

        // Optimization: Skip balance check to save 1 RPC call
        // Spammers assume wallets are funded. If not, the tx will fail on chain or be rejected.
        let amount = U256::from(1000u64); // Send small amount

        let dest = get_random_address()?;

        // tracing::info!("Sending token to {:?}...", dest);

        let mut transfer_data = hex::decode("a9059cbb000000000000000000000000").unwrap();
        transfer_data.extend_from_slice(dest.as_slice());
        transfer_data.extend_from_slice(&amount.to_be_bytes_vec());

        // Send with retry logic for nonce errors using explicit nonce management
        let mut attempt = 0;
        let max_retries = 3;
        let pending = loop {
            // Get fresh nonce BEFORE building transaction
            let nonce = match client.get_pending_nonce(&ctx.config.rpc_url).await {
                Ok(n) => n,
                Err(e) => {
                    attempt += 1;
                    if attempt >= max_retries {
                        return Err(e);
                    }
                    // Reduced sleep for speed
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    continue;
                }
            };

            let tx = TransactionRequest::default()
                .to(token_address)
                .input(transfer_data.clone().into())
                .from(address)
                .nonce(nonce); // EXPLICIT NONCE - prevents race conditions

            match client.provider.send_transaction(tx).await {
                Ok(p) => break p,
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    attempt += 1;

                    if (err_str.contains("nonce too low") || err_str.contains("already known"))
                        && attempt < max_retries
                    {
                        // Reset nonce cache and retry quickly
                        client.reset_nonce_cache().await;
                        // Reduced sleep for speed
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        continue;
                    } else {
                        return Err(e.into());
                    }
                }
            }
        };

        let tx_hash = *pending.tx_hash();

        Ok(TaskResult {
            success: true,
            message: format!("Sent 2% of {} to {:?}", token_name, dest),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
