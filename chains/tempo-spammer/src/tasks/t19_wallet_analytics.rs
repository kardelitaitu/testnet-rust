//! Wallet Analytics Task
//!
//! Displays wallet analytics including native token balances and created assets.

use crate::tasks::prelude::*;
use alloy::primitives::Address;
use alloy::rpc::types::TransactionRequest;
use alloy_primitives::U256;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::str::FromStr;

const SYSTEM_TOKENS: &[(&str, &str)] = &[
    ("PathUSD", "0x20C0000000000000000000000000000000000000"),
    ("AlphaUSD", "0x20c0000000000000000000000000000000000001"),
    ("BetaUSD", "0x20c0000000000000000000000000000000000002"),
    ("ThetaUSD", "0x20c0000000000000000000000000000000000003"),
];

#[derive(Debug, Clone, Default)]
pub struct WalletAnalyticsTask;

impl WalletAnalyticsTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for WalletAnalyticsTask {
    fn name(&self) -> &'static str {
        "19_wallet_analytics"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        let mut report = format!("=== Wallet Analytics ===\n");
        report.push_str(&format!("Address: {:?}\n\n", address));

        // 1. Check system stablecoin balances (these are the native tokens on Tempo)
        let mut system_balances = Vec::new();

        // ERC20 balanceOf signature
        let balance_of_selector: [u8; 4] = hex::decode("70a08231").unwrap().try_into().unwrap();

        for (name, addr) in SYSTEM_TOKENS {
            let token_addr: Address = addr.parse().context("Invalid token address")?;

            // Build balanceOf calldata: selector + padded address
            let mut calldata = Vec::new();
            calldata.extend_from_slice(&balance_of_selector);
            calldata.extend_from_slice(&[0u8; 12]);
            calldata.extend_from_slice(address.as_slice());

            let balance_data = TransactionRequest::default()
                .to(token_addr)
                .input(calldata.into());

            if let Ok(data) = client.provider.call(balance_data).await {
                let bytes = data.as_ref();
                if bytes.len() >= 32 {
                    let balance = U256::from_be_slice(bytes);
                    if balance > U256::ZERO {
                        let formatted = format_wei_to_tokens(balance, 6);
                        system_balances.push((name.to_string(), formatted));
                    }
                }
            }
        }

        report.push_str("Native Token Balances:\n");
        if !system_balances.is_empty() {
            for (name, balance) in &system_balances {
                report.push_str(&format!("  {}: {}\n", name, balance));
            }
        } else {
            report.push_str("  No token balances\n");
        }

        // 2. Get created assets from database
        let (my_tokens, my_memes) = if let Some(db) = &ctx.db {
            let stables = db
                .get_assets_by_type(&address.to_string(), "stablecoin")
                .await
                .unwrap_or_default();
            let memes = db
                .get_assets_by_type(&address.to_string(), "meme")
                .await
                .unwrap_or_default();
            (stables, memes)
        } else {
            (Vec::new(), Vec::new())
        };

        // 3. Check balances of created tokens (first 3 stablecoins + first 2 memes)
        let tokens_to_check: Vec<String> = my_tokens
            .iter()
            .take(3)
            .chain(my_memes.iter().take(2))
            .cloned()
            .collect();

        if !tokens_to_check.is_empty() {
            report.push_str("\nCreated Token Balances:\n");
            for token_addr in tokens_to_check {
                if let Ok(addr) = Address::from_str(&token_addr) {
                    let mut calldata = Vec::new();
                    calldata.extend_from_slice(&[0x70, 0xa0, 0x82, 0x31]);
                    calldata.extend_from_slice(&[0u8; 12]);
                    calldata.extend_from_slice(address.as_slice());

                    let balance_data = TransactionRequest::default()
                        .to(addr)
                        .input(calldata.into());

                    if let Ok(data) = client.provider.call(balance_data).await {
                        let bytes = data.as_ref();
                        if bytes.len() >= 32 {
                            let balance = U256::from_be_slice(bytes);
                            if balance > U256::ZERO {
                                let formatted = format_wei_to_tokens(balance, 6);
                                let short_addr = &token_addr[..16];
                                report.push_str(&format!("  {}...: {}\n", short_addr, formatted));
                            }
                        }
                    }
                }
            }
        } else if !my_tokens.is_empty() || !my_memes.is_empty() {
            report.push_str("\n  No token balances (tokens created but none held)\n");
        }

        // 4. Get created assets count
        let (created_stables, created_memes, total_transactions, success_rate) =
            if let Some(db) = &ctx.db {
                let stables = db
                    .get_asset_count_by_address(&address.to_string(), "stablecoin")
                    .await
                    .unwrap_or(0);
                let memes = db
                    .get_asset_count_by_address(&address.to_string(), "meme")
                    .await
                    .unwrap_or(0);
                let tx_count = db
                    .get_transaction_count(&address.to_string())
                    .await
                    .unwrap_or(0);
                let success_count = db
                    .get_success_count(&address.to_string())
                    .await
                    .unwrap_or(0);
                let rate = if tx_count > 0 {
                    (success_count as f64 / tx_count as f64) * 100.0
                } else {
                    0.0
                };
                (stables, memes, tx_count, rate)
            } else {
                (0, 0, 0, 0.0)
            };

        // Format balances with compact notation using TempoTokens helper
        let balances_summary = if system_balances.is_empty() {
            "None".to_string()
        } else {
            system_balances
                .iter()
                .map(|(n, b)| {
                    // b is already the raw number string, need to convert back to U256 and format
                    // Since we already have formatted strings, let's handle this differently
                    // We'll modify the earlier collection to store U256 instead
                    format!("{}: {}", n, b)
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        Ok(TaskResult {
            success: true,
            message: format!(
                "Analytics for {:?}: Native Balances: ({}), Assets: (Stables: {}, Memes: {}), TXs: {} (Success: {:.1}%)",
                address,
                balances_summary,
                created_stables,
                created_memes,
                total_transactions,
                success_rate
            ),
            tx_hash: None,
        })
    }
}

fn format_wei_to_tokens(wei: U256, decimals: u8) -> String {
    crate::tasks::tempo_tokens::TempoTokens::format_compact_colored(wei, decimals)
}
