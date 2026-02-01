//! Swap Stable Task
//!
//! Performs a swap on the Tempo Stablecoin DEX.
//! DEX: 0xdec0000000000000000000000000000000000000

use crate::tasks::prelude::*;
use alloy::rpc::types::TransactionRequest;
use alloy_primitives::{Address, U256};
use anyhow::{Context, Result};
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

const STABLECOIN_DEX_ADDRESS: &str = "0xdec0000000000000000000000000000000000000";

#[derive(Debug, Clone, Default)]
pub struct SwapStableTask;

impl SwapStableTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for SwapStableTask {
    fn name(&self) -> &'static str {
        "05_swap_stable"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let mut last_error = "No tokens with balance found".to_string();

        // 3 Attempts with different token pairs
        for attempt in 1..=3 {
            // Get balance for all system tokens
            let mut tokens_with_balance: Vec<(&str, &str, U256)> = Vec::new();

            for (name, addr) in SYSTEM_TOKENS {
                let token_addr: Address = addr.parse().ok().context("Invalid token address")?;

                let mut calldata = Vec::new();
                calldata.extend_from_slice(&[0x70, 0xa0, 0x82, 0x31]); // balanceOf selector
                calldata.extend_from_slice(&[0u8; 12]); // 12 bytes padding
                calldata.extend_from_slice(address.as_slice()); // 20 bytes address

                let balance_data = TransactionRequest::default()
                    .to(token_addr)
                    .input(calldata.into());

                let balance = if let Ok(data) = client.provider.call(balance_data).await {
                    let bytes = data.as_ref();
                    if !bytes.is_empty() {
                        U256::from_be_slice(bytes)
                    } else {
                        U256::ZERO
                    }
                } else {
                    U256::ZERO
                };
                tokens_with_balance.push((name, addr, balance));
            }

            // Filter to only tokens with balance
            let tokens_with_balance: Vec<_> = tokens_with_balance
                .into_iter()
                .filter(|(_, _, balance)| *balance > U256::ZERO)
                .collect();

            if tokens_with_balance.len() < 1 {
                return Ok(TaskResult {
                    success: false,
                    message: "No tokens with balance found".to_string(),
                    tx_hash: None,
                });
            }

            // Pick random token_in
            let (token_in_name, token_in_addr, balance) =
                *tokens_with_balance.choose(&mut rand::thread_rng()).unwrap();

            // Pick token_out from a DIFFERENT system token
            let (token_out_name, token_out_addr) = loop {
                let entry = SYSTEM_TOKENS
                    .choose(&mut rand::thread_rng())
                    .unwrap_or(&SYSTEM_TOKENS[0]);
                if entry.1 != token_in_addr {
                    break (entry.0, entry.1);
                }
            };

            let token_in: Address = token_in_addr.parse()?;
            let token_out: Address = token_out_addr.parse()?;
            let dex_address: Address = STABLECOIN_DEX_ADDRESS.parse()?;

            // Calculate 2-3% of balance
            let percentage = rand::thread_rng().gen_range(20..=30);
            let amount_raw = balance * U256::from(percentage) / U256::from(1000);
            let swap_amount: u128 = amount_raw.try_into().unwrap_or(100_000);

            if swap_amount == 0 {
                last_error = format!("Swap amount too low for {}", token_in_name);
                continue;
            }

            // Step 2: Approve the token for the DEX
            // 1. Approve 2x for safety
            let approve_amount: U256 = U256::from(swap_amount) * U256::from(2);
            let approve_calldata = build_approve_calldata(STABLECOIN_DEX_ADDRESS, approve_amount);
            let approve_tx = TransactionRequest::default()
                .to(token_in)
                .input(approve_calldata.into())
                .from(address);

            // Send approval with retry logic
            let approve_pending = match client.provider.send_transaction(approve_tx.clone()).await {
                Ok(p) => p,
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nonce too low") || err_str.contains("already known") {
                        tracing::warn!(
                            "Nonce error on approval (swap), resetting cache and retrying..."
                        );
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        client.provider.send_transaction(approve_tx).await?
                    } else {
                        return Err(e.into());
                    }
                }
            };
            let approve_receipt = approve_pending.get_receipt().await?;

            // 3. Check status
            if !approve_receipt.inner.status() {
                last_error = format!("Approval failed for {}", token_in_name);
                continue;
            }

            // Step 3: Execute the swap using swapExactAmountIn
            let min_amount_out = swap_amount * 80 / 100; // 20% slippage protection

            let mut swap_calldata: Vec<u8> = Vec::with_capacity(4 + 128);
            swap_calldata.extend_from_slice(&[0xf8, 0x85, 0x6c, 0x0f]);
            swap_calldata.extend_from_slice(&[0u8; 12]);
            swap_calldata.extend_from_slice(token_in.as_slice());
            swap_calldata.extend_from_slice(&[0u8; 12]);
            swap_calldata.extend_from_slice(token_out.as_slice());
            let amount_in_bytes: [u8; 16] = swap_amount.to_be_bytes();
            swap_calldata.extend_from_slice(&[0u8; 16]);
            swap_calldata.extend_from_slice(&amount_in_bytes);
            let min_out_bytes: [u8; 16] = min_amount_out.to_be_bytes();
            swap_calldata.extend_from_slice(&[0u8; 16]);
            swap_calldata.extend_from_slice(&min_out_bytes);

            // Send swap with retry logic
            let swap_tx = TransactionRequest::default()
                .to(dex_address)
                .input(swap_calldata.clone().into())
                .from(address);

            let pending = match client.provider.send_transaction(swap_tx.clone()).await {
                Ok(p) => p,
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nonce too low") || err_str.contains("already known") {
                        tracing::warn!("Nonce error on swap, resetting cache and retrying...");
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        client.provider.send_transaction(swap_tx).await?
                    } else {
                        return Err(e.into());
                    }
                }
            };

            let tx_hash = *pending.tx_hash();
            let receipt = pending.get_receipt().await?;

            if receipt.inner.status() {
                return Ok(TaskResult {
                    success: true,
                    message: format!(
                        "Swapped {} {} for {} on DEX: {:?}",
                        format_token_amount(swap_amount),
                        token_in_name,
                        token_out_name,
                        tx_hash
                    ),
                    tx_hash: Some(format!("{:?}", tx_hash)),
                });
            } else {
                last_error = format!(
                    "Reverted: {} -> {} ({} swap)",
                    token_in_name,
                    token_out_name,
                    format_token_amount(swap_amount)
                );
                // Continue to next attempt with a likely different pair
            }
        }

        Ok(TaskResult {
            success: false,
            message: format!(
                "Swap failed after 3 attempts with different pairs. Last error: {}",
                last_error
            ),
            tx_hash: None,
        })
    }
}

fn build_approve_calldata(spender: &str, amount: U256) -> Vec<u8> {
    // ERC20 approve selector: 0x095ea7b3
    let mut calldata = hex::decode("095ea7b3000000000000000000000000").unwrap();
    // Append spender address (32 bytes, right-padded with zeros)
    let spender_addr: Address = spender.parse().unwrap();
    calldata.extend_from_slice(spender_addr.as_slice());
    // Append amount (32 bytes, big-endian)
    calldata.extend_from_slice(&amount.to_be_bytes::<32>());
    calldata
}

fn format_token_balance(wei: U256) -> String {
    let units = U256::from(10u64).pow(U256::from(6));
    let whole = wei / units;
    let fractional: u128 = (wei % units).try_into().unwrap_or(0);
    let frac_str = format!("{:06}", fractional);
    let trimmed = frac_str.trim_end_matches('0');
    if trimmed.is_empty() {
        format!("{}", whole)
    } else {
        format!("{}.{}", whole, trimmed)
    }
}

fn format_token_amount(amount: u128) -> String {
    let units = 1_000_000u128;
    let whole = amount / units;
    format!("{}", whole)
}
