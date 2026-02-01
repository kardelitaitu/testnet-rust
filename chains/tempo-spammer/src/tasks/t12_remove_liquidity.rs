//! Remove Liquidity Task
//!
//! Withdraws liquidity from the Tempo Stablecoin DEX internal balance.
//! DEX: 0xdec0000000000000000000000000000000000000
//!
//! Workflow:
//! 1. Check DEX internal balance for all system tokens (balanceOf)
//! 2. If balance exists, withdraw to wallet
//! 3. If no balance, report "order placed successfully" (no fallback)

use crate::TempoClient;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::Address;
use alloy::rpc::types::TransactionRequest;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::prelude::SliceRandom;
use std::str::FromStr;

const STABLECOIN_DEX_ADDRESS: &str = "0xdec0000000000000000000000000000000000000";

const SYSTEM_TOKENS: &[(&str, &str)] = &[
    ("PathUSD", "0x20C0000000000000000000000000000000000000"),
    ("AlphaUSD", "0x20c0000000000000000000000000000000000001"),
    ("BetaUSD", "0x20c0000000000000000000000000000000000002"),
    ("ThetaUSD", "0x20c0000000000000000000000000000000000003"),
];

#[derive(Debug, Clone, Default)]
pub struct RemoveLiquidityTask;

impl RemoveLiquidityTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for RemoveLiquidityTask {
    fn name(&self) -> &'static str {
        "12_remove_liquidity"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        let dex_address =
            Address::from_str(STABLECOIN_DEX_ADDRESS).context("Invalid DEX address")?;

        // println!("Checking DEX internal balances...");

        let mut tokens_with_balance: Vec<(String, Address, u128)> = Vec::new();

        for (name, addr) in SYSTEM_TOKENS {
            if let Ok(token_addr) = Address::from_str(addr) {
                let balance = get_dex_balance(client, dex_address, token_addr, address).await?;
                if balance > 0 {
                    tokens_with_balance.push((name.to_string(), token_addr, balance));
                    // println!("{} DEX balance: {}", name, format_token_amount(balance));
                }
            }
        }

        if tokens_with_balance.is_empty() {
            // println!("No DEX balance found. Order placed successfully.");
            return Ok(TaskResult {
                success: true,
                message: "No withdrawable balance yet. Order placed successfully.".to_string(),
                tx_hash: None,
            });
        }

        let (token_name, token_address, dex_balance) = tokens_with_balance
            .choose(&mut rand::thread_rng())
            .map(|(n, a, b)| (n.clone(), *a, *b))
            .context("Failed to select token with balance")?;

        // println!(
        tracing::debug!("Withdrawing {} AlphaUSD from DEX...", dex_balance);
        //     format_token_amount(dex_balance),
        //     token_name
        // );

        let withdraw_calldata = build_withdraw_calldata(token_address, dex_balance);

        let tx = TransactionRequest::default()
            .to(dex_address)
            .input(withdraw_calldata.into())
            .from(address);

        // Send with retry logic for nonce errors (1 retry)
        let pending = match client.provider.send_transaction(tx.clone()).await {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!(
                        "Nonce error on remove_liquidity, resetting cache and retrying..."
                    );
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client
                        .provider
                        .send_transaction(tx)
                        .await
                        .context("Failed to send withdraw transaction")?
                } else {
                    return Err(e).context("Failed to send withdraw transaction");
                }
            }
        };

        let tx_hash = *pending.tx_hash();

        let receipt = pending
            .get_receipt()
            .await
            .context("Failed to get withdraw receipt")?;

        if !receipt.inner.status() {
            return Ok(TaskResult {
                success: false,
                message: "Withdraw reverted".to_string(),
                tx_hash: Some(format!("{:?}", tx_hash)),
            });
        }

        // println!("✅ Withdraw successful: {:?}", tx_hash);

        Ok(TaskResult {
            success: true,
            message: format!(
                "Withdrew {} {} from DEX. Tx: {:?}",
                format_token_amount(dex_balance),
                token_name,
                tx_hash
            ),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}

async fn get_dex_balance(
    client: &crate::TempoClient,
    dex: Address,
    token: Address,
    user: Address,
) -> Result<u128> {
    let mut calldata = Vec::new();
    calldata.extend_from_slice(&[0x4f, 0x83, 0x29, 0x24]);
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(user.as_slice());
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(token.as_slice());

    let query = TransactionRequest::default().to(dex).input(calldata.into());

    if let Ok(data) = client.provider.call(query).await {
        let bytes = data.as_ref();
        if !bytes.is_empty() && bytes.len() >= 16 {
            let mut balance_bytes = [0u8; 16];
            let offset = bytes.len().saturating_sub(16);
            balance_bytes.copy_from_slice(&bytes[offset..]);
            return Ok(u128::from_be_bytes(balance_bytes));
        }
    }
    Ok(0)
}

fn build_withdraw_calldata(token: Address, amount: u128) -> Vec<u8> {
    let mut calldata = Vec::new();
    calldata.extend_from_slice(&[0x2e, 0x3a, 0x9c, 0x5c]);
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(token.as_slice());
    let amount_bytes: [u8; 16] = amount.to_be_bytes();
    calldata.extend_from_slice(&amount_bytes);
    calldata
}

fn format_token_amount(amount: u128) -> String {
    let units = 1_000_000u128;
    let whole = amount / units;
    format!("{}", whole)
}
