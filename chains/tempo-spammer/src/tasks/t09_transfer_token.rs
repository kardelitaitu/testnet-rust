//! Transfer Token Task
//!
//! Transfers tokens (system or created) to a random recipient.
//! Supports PathUSD, AlphaUSD, BetaUSD, ThetaUSD, and created stablecoins.
//!
//! Workflow:
//! 1. Build token list from system tokens + created tokens from DB
//! 2. Check balances on random subset of tokens
//! 3. Find token with sufficient balance
//! 4. Generate random recipient address
//! 5. Calculate transfer amount (10-50 units or 50% of balance)
//! 6. Execute transfer with appropriate fee token

use crate::TempoClient;
use crate::tasks::tempo_tokens::{TempoTokens, TokenInfo};
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use rand::prelude::SliceRandom;
use std::str::FromStr;

#[derive(Debug, Clone, Default)]
pub struct TransferTokenTask;

impl TransferTokenTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for TransferTokenTask {
    fn name(&self) -> &'static str {
        "09_transfer_token"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let wallet_addr_str = format!("{:?}", address);

        let mut available_tokens: Vec<TokenInfo> = TempoTokens::get_system_tokens();

        if let Some(db) = &ctx.db {
            if let Ok(created_addresses) =
                db.get_assets_by_type(&wallet_addr_str, "stablecoin").await
            {
                for addr in created_addresses {
                    if let Ok(token_addr) = Address::from_str(&addr) {
                        let symbol = addr.get(..8).unwrap_or("Unknown").to_string();
                        available_tokens.push(TokenInfo {
                            symbol,
                            address: token_addr,
                            is_system: false,
                        });
                    }
                }
            }
        }

        if available_tokens.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No tokens available (system or created)".to_string(),
                tx_hash: None,
            });
        }

        let mut rng = rand::rngs::OsRng;
        available_tokens.shuffle(&mut rng);

        let mut selected_token: Option<TokenInfo> = None;
        let mut token_decimals: u8 = 6;

        for token in available_tokens.iter().take(5) {
            let balance = TempoTokens::get_token_balance(client, token.address, address).await?;
            // token_decimals = TempoTokens::get_token_decimals(client, token.address).await?; // Optimization: only fetch if needed or just fetch now for debug

            // Debug prints
            tracing::debug!(
                "Checking {} ({}) Balance: {}",
                token.symbol,
                token.address,
                balance
            );

            if balance > U256::from(100_000u64) {
                token_decimals = TempoTokens::get_token_decimals(client, token.address).await?;
                let formatted_balance = TempoTokens::format_amount(balance, token_decimals);

                tracing::debug!(
                    "MATCH! {} balance: {} {}",
                    token.symbol,
                    formatted_balance,
                    token.symbol
                );

                selected_token = Some(token.clone());
                break;
            }
        }

        let Some(token) = selected_token else {
            return Ok(TaskResult {
                success: false,
                message: "No tokens with sufficient balance found".to_string(),
                tx_hash: None,
            });
        };

        let balance = TempoTokens::get_token_balance(client, token.address, address).await?;
        let amount_units = rng.gen_range(10..51);
        let amount_wei = U256::from(amount_units) * U256::from(10_u64.pow(token_decimals as u32));

        let actual_amount = if balance < amount_wei {
            balance / U256::from(2)
        } else {
            amount_wei
        };

        let recipient = {
            let bytes: [u8; 20] = rng.r#gen();
            Address::from_slice(&bytes)
        };

        let recipient_formatted = format!("{:?}", recipient);
        let recipient_short = recipient_formatted.get(..14).unwrap_or("?");

        // println!(
        //     "Transferring {} {} to {}...",
        //     TempoTokens::format_amount(actual_amount, token_decimals),
        //     token.symbol,
        //     recipient_short
        // );

        let fee_token = if token.is_system {
            token.address
        } else {
            let random_system = TempoTokens::get_random_system_token();
            // println!("Using {} as fee token", random_system.symbol);
            random_system.address
        };

        let transfer_calldata = build_transfer_calldata(recipient, actual_amount);

        let tx = TransactionRequest::default()
            .to(token.address)
            .input(TransactionInput::from(transfer_calldata))
            .from(address)
            .max_fee_per_gas(150_000_000_000u128)
            .max_priority_fee_per_gas(1_500_000_000u128);

        // Send with retry logic for nonce errors (1 retry)
        let pending = match client.provider.send_transaction(tx.clone()).await {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!(
                        "Nonce error on transfer_token, resetting cache and retrying..."
                    );
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client
                        .provider
                        .send_transaction(tx)
                        .await
                        .context("Failed to send transfer")?
                } else {
                    return Err(e).context("Failed to send transfer");
                }
            }
        };

        let tx_hash = *pending.tx_hash();

        let receipt = pending
            .get_receipt()
            .await
            .context("Failed to get receipt")?;

        if !receipt.inner.status() {
            return Ok(TaskResult {
                success: false,
                message: "Transfer reverted".to_string(),
                tx_hash: Some(format!("{:?}", tx_hash)),
            });
        }

        // println!(
        //     "✅ Transfer successful: {:?} (Block {:?})",
        //     tx_hash, receipt.block_number
        // );

        Ok(TaskResult {
            success: true,
            message: format!(
                "Transferred {} {} to {}",
                TempoTokens::format_amount(actual_amount, token_decimals),
                token.symbol,
                recipient_short
            ),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}

fn build_transfer_calldata(to: Address, amount: U256) -> Vec<u8> {
    let mut calldata = Vec::new();
    calldata.extend_from_slice(&[0xa9, 0x05, 0x9c, 0xbb]);
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(to.as_slice());
    calldata.extend_from_slice(&amount.to_be_bytes::<32>());
    calldata
}
