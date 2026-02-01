//! Batch Send Transaction Stable Task
//!
//! Sends a batch of stable token transfers sequentially.
//!
//! Workflow:
//! 1. Generate random recipients
//! 2. Send stable token transfers sequentially
//! 3. Collect results

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask, get_random_address};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use anyhow::Result;
use async_trait::async_trait;
use rand::Rng;
use std::str::FromStr;

const MINT_SELECTOR: [u8; 4] = [0x40, 0xc1, 0x0f, 0x19];
const TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

#[derive(Debug, Clone, Default)]
pub struct BatchSendTransactionStableTask;

impl BatchSendTransactionStableTask {
    pub fn new() -> Self {
        Self
    }

    async fn execute_burst(
        &self,
        ctx: &TaskContext,
        token_addr: Address,
        count: usize,
        amount_wei: U256,
    ) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        // 1. Prepare Pipeline
        let mut current_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let start_nonce = current_nonce;
        let mut burst_txs = Vec::new();

        for _ in 0..count {
            let recipient = get_random_address()?;
            let transfer_calldata = build_transfer_calldata(recipient, amount_wei);
            let tx = TransactionRequest::default()
                .to(token_addr)
                .input(TransactionInput::from(transfer_calldata))
                .from(address)
                .nonce(current_nonce)
                .gas_limit(100_000);

            burst_txs.push(tx);
            current_nonce += 1;
        }

        // 2. Burst Submit
        tracing::info!(
            "Blasting {} Stable Transfers (Start Nonce: {})",
            count,
            start_nonce
        );

        let mut last_submitted_nonce = start_nonce.wrapping_sub(1);
        let mut last_hash = String::new();
        let mut submission_count = 0;

        for (idx, tx) in burst_txs.iter().enumerate() {
            let tx_nonce = start_nonce + idx as u64;
            match client.provider.send_transaction(tx.clone()).await {
                Ok(pending) => {
                    last_hash = pending.tx_hash().to_string();
                    last_submitted_nonce = tx_nonce;
                    submission_count += 1;
                }
                Err(e) => {
                    tracing::error!("Pipelined Stable Tx at nonce {} failure: {}", tx_nonce, e);
                    break; // CRITICAL: Stop on first failure
                }
            }
        }

        // 3. Update Nonce Manager with next nonce after last successful submission
        if let Some(manager) = &client.nonce_manager {
            let next_nonce = last_submitted_nonce.wrapping_add(1);
            manager.set(address, next_nonce).await;
        }

        if submission_count == 0 {
            anyhow::bail!("Failed to submit any transactions in stable pipeline.");
        }

        Ok(TaskResult {
            success: true,
            message: format!(
                "Pipelined {}/{} stable transfers via Optimistic Burst.",
                submission_count, count
            ),
            tx_hash: Some(last_hash),
        })
    }
}

#[async_trait]
impl TempoTask for BatchSendTransactionStableTask {
    fn name(&self) -> &'static str {
        "35_batch_send_transaction_stable"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        use rand::seq::SliceRandom;
        use std::str::FromStr;

        let client = &ctx.client;
        let address = ctx.address();

        // 1. Select Created Stable Token (or fallback to PathUSD)
        let mut token_addr = TempoTokens::get_path_usd_address();
        let mut token_symbol = "PathUSD".to_string();
        let mut using_created_token = false;

        if let Some(db) = &ctx.db {
            if let Ok(assets) = db
                .get_assets_by_type(&address.to_string(), "stablecoin")
                .await
            {
                if !assets.is_empty() {
                    let mut rng = rand::thread_rng();
                    if let Some(random_asset) = assets.choose(&mut rng) {
                        if let Ok(addr) = Address::from_str(random_asset) {
                            token_addr = addr;
                            token_symbol = format!("Created-Stable-{}", &random_asset[..8]);
                            using_created_token = true;
                        }
                    }
                }
            }
        }

        if using_created_token {
            tracing::info!("Using created stable token: {}", token_symbol);
        } else {
            tracing::info!("Using default stable token (PathUSD)");
        }

        let mut rng = rand::rngs::OsRng;
        let count = rng.gen_range(5..10);

        // 2. Fetch balance and calculate 1% per recipient
        let balance = TempoTokens::get_token_balance(client, token_addr, address)
            .await
            .unwrap_or(U256::ZERO);

        let amount_wei = balance / U256::from(100); // 1% of balance
        let total_needed = amount_wei * U256::from(count);

        // 3. Auto-Mint if insufficient balance
        if balance < total_needed || amount_wei.is_zero() {
            tracing::info!("Low stable balance. Auto-Minting needed amount...");
            let mint_amount = if amount_wei.is_zero() {
                U256::from(10_000_000_000_000_000u64) // Minimum viable
            } else {
                total_needed * U256::from(5)
            };
            let mint_calldata = build_mint_calldata(address, mint_amount);
            let mint_tx = TransactionRequest::default()
                .to(token_addr)
                .input(TransactionInput::from(mint_calldata))
                .from(address);

            match client.provider.send_transaction(mint_tx).await {
                Ok(pending) => {
                    let _ = pending.get_receipt().await;
                    tracing::debug!("Mint confirmed. Waiting for node sync...");
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
                Err(e) => {
                    tracing::warn!("Auto-mint submission failed: {}. Proceeding anyway...", e);
                }
            }
        }

        self.execute_burst(ctx, token_addr, count, amount_wei).await
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
