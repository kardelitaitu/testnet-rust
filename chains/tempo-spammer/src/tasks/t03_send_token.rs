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

        let mut balance_data = hex::decode("70a08231000000000000000000000000").unwrap();
        balance_data.extend_from_slice(address.as_slice());

        let response = client
            .provider
            .call(
                TransactionRequest::default()
                    .to(token_address)
                    .input(balance_data.into()),
            )
            .await?;

        let balance = if response.0.len() == 32 {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&response.0[..32]);
            U256::from_be_bytes(bytes)
        } else {
            U256::ZERO
        };

        let min_balance = U256::from(1_000_000u64);

        if balance < min_balance {
            return Ok(TaskResult {
                success: false,
                message: format!("Low {} balance: {} (Need 10^6)", token_name, balance),
                tx_hash: None,
            });
        }

        let dest = get_random_address()?;

        let amount = balance / U256::from(50);

        if amount.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!("Balance too low to send 2% (balance: {})", balance),
                tx_hash: None,
            });
        }

        // tracing::info!("Sending 2% of {} balance to {:?}...", token_name, dest);

        let mut transfer_data = hex::decode("a9059cbb000000000000000000000000").unwrap();
        transfer_data.extend_from_slice(dest.as_slice());
        transfer_data.extend_from_slice(&amount.to_be_bytes_vec());

        // Send with retry logic for nonce errors (1 retry)
        let base_tx = TransactionRequest::default()
            .to(token_address)
            .input(transfer_data.into())
            .from(address);

        let pending = match client.provider.send_transaction(base_tx.clone()).await {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on send_token, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client.provider.send_transaction(base_tx).await?
                } else {
                    return Err(e.into());
                }
            }
        };

        let tx_hash = pending.tx_hash().clone();

        Ok(TaskResult {
            success: true,
            message: format!("Sent 2% of {} to {:?}", token_name, dest),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
