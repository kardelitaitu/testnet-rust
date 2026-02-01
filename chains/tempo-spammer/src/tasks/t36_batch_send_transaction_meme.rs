//! Batch Send Transaction Meme Task
//!
//! Executes multiple meme transfers in a batch.
//!
//! Workflow:
//! 1. Select meme token from DB
//! 2. Setup token (with retries for proxy errors)
//! 3. Mint needed total if insufficient (confirmed)
//! 4. Execute transfers sequentially with confirmations

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
pub struct BatchSendTransactionMemeTask;

impl BatchSendTransactionMemeTask {
    pub fn new() -> Self {
        Self
    }

    async fn execute_batch(
        &self,
        ctx: &TaskContext,
        token_addr: Address,
        symbol: String,
        decimals: u8,
        mut balance: U256,
        count: usize,
        amount_wei: U256,
    ) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let total_needed = amount_wei * U256::from(count);
        let amount_base = amount_wei / U256::from(10_u64.pow(decimals as u32));

        // 1. Mint if needed (Confirmed)
        if balance < total_needed {
            tracing::info!("Low balance. Attempting to Mint total...");
            let mint_calldata = build_mint_calldata(address, total_needed * U256::from(2));
            let mint_tx = TransactionRequest::default()
                .to(token_addr)
                .input(TransactionInput::from(mint_calldata))
                .from(address);

            match client.provider.send_transaction(mint_tx).await {
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
                    if err_str.contains("aa4bc69a") {
                        return Ok(TaskResult {
                            success: false,
                            message: "Skipped: Token sold out (0xaa4bc69a)".to_string(),
                            tx_hash: None,
                        });
                    }
                    anyhow::bail!("Mint submission failed: {}", e);
                }
            }
        }

        if balance < total_needed {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient balance for {} batch after mint attempt",
                    symbol
                ),
                tx_hash: None,
            });
        }

        // 2. Prepare Pipeline
        let mut current_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let start_nonce = current_nonce;
        let mut burst_txs = Vec::new();

        for i in 1..=count {
            let recipient = get_random_address()?;
            let transfer_calldata = build_transfer_calldata(recipient, amount_wei);
            let tx = TransactionRequest::default()
                .to(token_addr)
                .input(TransactionInput::from(transfer_calldata))
                .from(address)
                .nonce(current_nonce)
                .gas_limit(150_000); // Standard safe limit for TIP-20 transfers

            burst_txs.push(tx);
            current_nonce += 1;
        }

        // 3. Burst Submit (Zero-Wait)
        tracing::info!(
            "Blasting {} Transactions (Nonces {}..{})",
            burst_txs.len(),
            start_nonce,
            current_nonce - 1
        );

        let mut last_submitted_nonce = start_nonce.wrapping_sub(1);
        let mut last_hash = String::new();
        let mut submission_count = 0;

        let mut first_error = None;

        for (idx, tx) in burst_txs.iter().enumerate() {
            let tx_nonce = start_nonce + idx as u64;
            match client.provider.send_transaction(tx.clone()).await {
                Ok(pending) => {
                    last_hash = pending.tx_hash().to_string();
                    last_submitted_nonce = tx_nonce;
                    submission_count += 1;
                }
                Err(e) => {
                    tracing::error!("Pipelined Tx at nonce {} failure: {}", tx_nonce, e);
                    if first_error.is_none() {
                        first_error = Some(anyhow::anyhow!(e));
                    }
                    // if e.to_string().contains("TunnelUnsuccessful") {
                    //    anyhow::bail!("Proxy TunnelUnsuccessful during burst: {}", e);
                    // }
                    break; // CRITICAL: Stop on first failure
                }
            }
        }

        // 4. Update Nonce Manager with next nonce after last successful submission
        if let Some(manager) = &client.nonce_manager {
            let next_nonce = last_submitted_nonce.wrapping_add(1);
            manager.set(address, next_nonce).await;
        }

        if submission_count == 0 {
            if let Some(err) = first_error {
                return Err(err);
            }
            anyhow::bail!("Failed to submit any transactions in pipeline.");
        }

        Ok(TaskResult {
            success: true,
            message: format!(
                "Pipelined {}/{} {} transfers via Optimistic Burst.",
                submission_count, count, symbol
            ),
            tx_hash: Some(last_hash),
        })
    }
}

#[async_trait]
impl TempoTask for BatchSendTransactionMemeTask {
    fn name(&self) -> &'static str {
        "36_batch_send_transaction_meme"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        tracing::info!("--- DIAGNOSTIC START: Task 36 (Batch Meme Transfers) ---");
        tracing::info!(
            "Wallet: {:?}, Proxy: {:?}",
            address,
            client.proxy_config.as_ref().map(|p| &p.url)
        );

        let mut meme_tokens = if let Some(db) = &ctx.db {
            let tokens = db
                .get_assets_by_type(&address.to_string(), "meme")
                .await
                .unwrap_or_default();

            // Filter out system tokens (starting with 0x20c0)
            tokens
                .into_iter()
                .filter(|t| !t.to_lowercase().starts_with("0x20c0"))
                .collect()
        } else {
            Vec::new()
        };

        if meme_tokens.is_empty() {
            meme_tokens.push(TempoTokens::FALLBACK_MEME_TOKEN.to_string());
        }

        let mut rng = rand::rngs::OsRng;
        let count = rng.gen_range(5..10);
        let mut attempts = 0;
        let max_attempts = 10;

        loop {
            attempts += 1;
            let token_addr_str = meme_tokens.choose(&mut rng).unwrap().clone();
            let token_addr = Address::from_str(&token_addr_str).context("Invalid token address")?;
            let symbol = token_addr_str.get(..8).unwrap_or("MEME").to_string();

            match async {
                let d = TempoTokens::get_token_decimals(client, token_addr).await?;
                let b = TempoTokens::get_token_balance(client, token_addr, address).await?;
                Result::<(u8, U256)>::Ok((d, b))
            }
            .await
            {
                Ok((decimals, balance)) => {
                    // Calculate 1% of balance per recipient
                    let amount_wei = balance / U256::from(100);

                    tracing::info!(
                        "Token: {} ({}), Balance: {}, Amount per tx: {} (1%)",
                        symbol,
                        token_addr,
                        balance,
                        amount_wei
                    );
                    return self
                        .execute_batch(
                            ctx, token_addr, symbol, decimals, balance, count, amount_wei,
                        )
                        .await;
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("aa4bc69a") {
                        tracing::warn!("Token {} is dead (0xaa4bc69a). Trying another...", symbol);
                        if meme_tokens.len() > 1 {
                            meme_tokens.retain(|t| t != &token_addr_str);
                            if attempts < max_attempts {
                                continue;
                            }
                        }
                    }
                    if attempts >= max_attempts {
                        anyhow::bail!("Failed batch setup after {} attempts: {}", attempts, e);
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
