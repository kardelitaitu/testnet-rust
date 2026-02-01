//! Transfer Meme Task
//!
//! Transfers meme tokens to a random recipient.
//!
//! Workflow:
//! 1. Query meme tokens from DB
//! 2. Select random token and check balance (with retries for proxy/network)
//! 3. Mint if balance insufficient (with sequential confirmation)
//! 4. Transfer to random address

use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask, get_random_address};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use rand::seq::SliceRandom;
use std::str::FromStr;

const MINT_SELECTOR: [u8; 4] = [0x40, 0xc1, 0x0f, 0x19];
const TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

#[derive(Debug, Clone, Default)]
pub struct TransferMemeTask;

impl TransferMemeTask {
    pub fn new() -> Self {
        Self
    }

    async fn execute_transfer(
        &self,
        ctx: &TaskContext,
        token_addr: Address,
        symbol: String,
        decimals: u8,
        mut balance: U256,
        amount_wei: U256,
    ) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let amount_base = amount_wei / U256::from(10_u64.pow(decimals as u32));

        // 1. Mint if needed (Confirmed)
        if balance < amount_wei {
            tracing::info!("Low balance. Attempting to Mint...");
            let mint_calldata = build_mint_calldata(address, amount_wei * U256::from(10));
            let mint_tx = TransactionRequest::default()
                .to(token_addr)
                .input(TransactionInput::from(mint_calldata))
                .from(address);

            match client.provider.send_transaction(mint_tx.clone()).await {
                Ok(pending) => {
                    tracing::debug!("Mint Sent: {}. Waiting confirmation...", pending.tx_hash());
                    if let Ok(receipt) = pending.get_receipt().await {
                        if receipt.inner.status() {
                            tracing::info!("✅ Mint Confirmed.");
                            balance = TempoTokens::get_token_balance(client, token_addr, address)
                                .await
                                .unwrap_or(U256::ZERO);
                        } else {
                            anyhow::bail!("Mint transaction reverted on-chain.");
                        }
                    }
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nonce too low") || err_str.contains("already known") {
                        tracing::warn!(
                            "Nonce error on meme mint (t23), resetting cache and retrying..."
                        );
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

                        // Retry
                        match client.provider.send_transaction(mint_tx).await {
                            Ok(pending) => {
                                tracing::debug!(
                                    "Mint Sent (Retry): {}. Waiting confirmation...",
                                    pending.tx_hash()
                                );
                                if let Ok(receipt) = pending.get_receipt().await {
                                    if receipt.inner.status() {
                                        tracing::info!("✅ Mint Confirmed (Retry).");
                                        balance = TempoTokens::get_token_balance(
                                            client, token_addr, address,
                                        )
                                        .await
                                        .unwrap_or(U256::ZERO);
                                    }
                                }
                            }
                            Err(e2) => tracing::warn!("Retry failed: {}", e2),
                        }
                    } else if err_str.contains("aa4bc69a") {
                        return Ok(TaskResult {
                            success: false,
                            message: "Skipped: Token sold out (0xaa4bc69a)".to_string(),
                            tx_hash: None,
                        });
                    } else if err_str.contains("unauthorized") || err_str.contains("82b42900") {
                        tracing::debug!("Cannot mint token (unauthorized), using existing balance");
                        // Continue with existing balance - if it's insufficient, the check below will handle it
                        // The caller (run function) will retry with another token if this fails
                    } else {
                        anyhow::bail!("Mint submission failed: {}", e);
                    }
                }
            }
        }

        if balance < amount_wei {
            return Ok(TaskResult {
                success: false,
                message: format!("Insufficient balance for {} after mint attempt", symbol),
                tx_hash: None,
            });
        }

        // 2. Transfer (Sequential)
        let recipient = get_random_address()?;
        tracing::debug!("Transferring {} {} to {:?}", amount_base, symbol, recipient);

        let transfer_calldata = build_transfer_calldata(recipient, amount_wei);
        let tx = TransactionRequest::default()
            .to(token_addr)
            .input(TransactionInput::from(transfer_calldata))
            .from(address);

        match client.provider.send_transaction(tx.clone()).await {
            Ok(pending) => {
                let tx_hash = *pending.tx_hash();
                if let Ok(receipt) = pending.get_receipt().await {
                    if receipt.inner.status() {
                        Ok(TaskResult {
                            success: true,
                            message: format!(
                                "Transferred {} {} to {:?}.",
                                amount_base, symbol, recipient
                            ),
                            tx_hash: Some(format!("{:?}", tx_hash)),
                        })
                    } else {
                        Ok(TaskResult {
                            success: false,
                            message: "Transfer transaction reverted".to_string(),
                            tx_hash: Some(format!("{:?}", tx_hash)),
                        })
                    }
                } else {
                    anyhow::bail!("Failed to get transfer receipt");
                }
            }
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on meme transfer, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

                    let retry_res = client.provider.send_transaction(tx).await;
                    match retry_res {
                        Ok(pending) => {
                            let tx_hash = *pending.tx_hash();
                            // Assuming success on retry if sent
                            Ok(TaskResult {
                                success: true,
                                message: format!(
                                    "Transferred {} {} to {:?} (Retry).",
                                    amount_base, symbol, recipient
                                ),
                                tx_hash: Some(format!("{:?}", tx_hash)),
                            })
                        }
                        Err(e2) => anyhow::bail!("Transfer (Retry) failed: {}", e2),
                    }
                } else {
                    anyhow::bail!("Transfer submission failed: {}", e);
                }
            }
        }
    }
}

#[async_trait]
impl TempoTask for TransferMemeTask {
    fn name(&self) -> &'static str {
        "23_transfer_meme"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        tracing::info!("--- DIAGNOSTIC START: Task 23 (Transfer Meme) ---");
        tracing::info!(
            "Wallet: {:?}, Proxy: {:?}",
            address,
            client.proxy_config.as_ref().map(|p| &p.url)
        );

        let mut meme_tokens = if let Some(db) = &ctx.db {
            db.get_assets_by_type(&address.to_string(), "meme")
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        if meme_tokens.is_empty() {
            meme_tokens.push(TempoTokens::FALLBACK_MEME_TOKEN.to_string());
        }

        let mut rng = rand::rngs::OsRng;
        let amount_base = rng.gen_range(1..10);
        let mut attempts = 0;
        let max_attempts = 10;

        loop {
            attempts += 1;
            let token_addr_str = meme_tokens.choose(&mut rng).unwrap().clone();
            let token_addr = Address::from_str(&token_addr_str).context("Invalid token address")?;
            let symbol = token_addr_str.get(..8).unwrap_or("MEME").to_string();

            // Setup Token (View Calls with Retries)
            match async {
                let d = TempoTokens::get_token_decimals(client, token_addr).await?;
                let b = TempoTokens::get_token_balance(client, token_addr, address).await?;
                Result::<(u8, U256)>::Ok((d, b))
            }
            .await
            {
                Ok((decimals, balance)) => {
                    let amount_wei =
                        U256::from(amount_base) * U256::from(10_u64.pow(decimals as u32));
                    tracing::info!(
                        "Token: {} ({}), Balance: {}, Needed: {}",
                        symbol,
                        token_addr,
                        balance,
                        amount_wei
                    );
                    match self
                        .execute_transfer(
                            ctx,
                            token_addr,
                            symbol.clone(),
                            decimals,
                            balance,
                            amount_wei,
                        )
                        .await
                    {
                        Ok(result) => {
                            // If insufficient balance, try another token
                            if !result.success && result.message.contains("Insufficient balance") {
                                tracing::warn!(
                                    "Token {} has insufficient balance after mint attempt. Trying another...",
                                    symbol
                                );
                                if meme_tokens.len() > 1 {
                                    meme_tokens.retain(|t| t != &token_addr_str);
                                    if attempts < max_attempts {
                                        continue;
                                    }
                                }
                            }
                            return Ok(result);
                        }
                        Err(e) => return Err(e),
                    }
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("aa4bc69a") {
                        tracing::warn!(
                            "Token {} is dead/sold-out (0xaa4bc69a). Trying another...",
                            symbol
                        );
                        if meme_tokens.len() > 1 {
                            meme_tokens.retain(|t| t != &token_addr_str);
                            if attempts < max_attempts {
                                continue;
                            }
                        }
                    }
                    if attempts >= max_attempts {
                        anyhow::bail!("Failed setup after {} attempts: {}", attempts, e);
                    }
                    tracing::warn!(
                        "Proxy/Network error on attempt {}: {}. Retrying...",
                        attempts,
                        e
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }
    }
}

fn build_transfer_calldata(to: Address, amount: U256) -> Vec<u8> {
    let mut calldata = Vec::new();
    calldata.extend_from_slice(&TRANSFER_SELECTOR);
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(to.as_slice());
    calldata.extend_from_slice(&amount.to_be_bytes::<32>());
    calldata
}

fn build_mint_calldata(to: Address, amount: U256) -> Vec<u8> {
    let mut calldata = Vec::new();
    calldata.extend_from_slice(&MINT_SELECTOR);
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(to.as_slice());
    calldata.extend_from_slice(&amount.to_be_bytes::<32>());
    calldata
}
