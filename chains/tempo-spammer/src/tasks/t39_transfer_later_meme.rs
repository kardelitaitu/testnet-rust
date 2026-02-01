//! Transfer Later Meme Task
//!
//! Schedules a meme token transfer with a delay.
//!
//! Workflow:
//! 1. Query meme token from DB
//! 2. Generate random recipient
//! 3. Sleep for random delay
//! 4. Execute meme token transfer

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask, get_random_address};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use std::str::FromStr;
use std::time::Duration;

const TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];
const MINT_SELECTOR: [u8; 4] = [0x40, 0xc1, 0x0f, 0x68];

#[derive(Debug, Clone, Default)]
pub struct TransferLaterMemeTask;

impl TransferLaterMemeTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for TransferLaterMemeTask {
    fn name(&self) -> &'static str {
        "39_transfer_later_meme"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        use alloy::primitives::{Bytes, TxKind, U256};
        use alloy::providers::Provider;
        use alloy::rlp::Encodable;
        use alloy::signers::Signer;
        use tempo_primitives::transaction::{Call, TempoSignature, TempoTransaction};

        let client = &ctx.client;
        let address = ctx.address();
        let chain_id = ctx.chain_id();
        let wallet_addr_str = address.to_string();

        let meme_tokens = if let Some(db) = &ctx.db {
            match db.get_assets_by_type(&wallet_addr_str, "meme").await {
                Ok(addresses) => addresses,
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        if meme_tokens.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No created meme tokens found in DB for scheduled transfer.".to_string(),
                tx_hash: None,
            });
        }

        let token_addr_str = meme_tokens[0].clone();
        let token_addr = if let Ok(addr) = Address::from_str(&token_addr_str) {
            addr
        } else {
            return Ok(TaskResult {
                success: false,
                message: "Invalid token address".to_string(),
                tx_hash: None,
            });
        };

        let mut rng = rand::rngs::OsRng;
        let delay = rng.gen_range(3..=5); // Random 3-5 seconds
        let recipient = get_random_address()?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let valid_after = now + delay;
        let valid_before = valid_after + 300;

        let decimals = TempoTokens::get_token_decimals(client, token_addr).await?;
        let mut balance = TempoTokens::get_token_balance(client, token_addr, address).await?;

        // If balance is zero, mint some first
        if balance.is_zero() {
            let mint_amount = U256::from(1000) * U256::from(10_u64.pow(decimals as u32));
            let mint_calldata = build_mint_calldata(address, mint_amount);
            let mint_tx = TransactionRequest::default()
                .to(token_addr)
                .input(TransactionInput::from(mint_calldata))
                .from(address)
                .max_fee_per_gas(150_000_000_000u128)
                .max_priority_fee_per_gas(1_500_000_000u128);

            match client.provider.send_transaction(mint_tx).await {
                Ok(pending) => {
                    let _ = pending.get_receipt().await;
                    // Optimistically assume mint worked for calculation or just set balance
                    balance = mint_amount;
                }
                Err(_) => {
                    // If mint fails, we can't do 1% transfer effectively, but let's try 0
                    balance = U256::ZERO;
                }
            }
        }

        let amount_wei = balance / U256::from(100);

        tracing::debug!(
            "Scheduling meme transfer of {} units to {:?} valid after +{}s (native)...",
            amount_wei,
            recipient,
            delay
        );

        // Get nonce
        let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let gas_price = client.provider.get_gas_price().await?;
        let max_fee = U256::from(gas_price) * U256::from(120) / U256::from(100);

        let transfer_calldata = build_transfer_calldata(recipient, amount_wei);

        // Construct Tx
        let tx = TempoTransaction {
            chain_id,
            max_priority_fee_per_gas: 1_500_000_000,
            max_fee_per_gas: max_fee.to::<u128>(),
            gas_limit: 250_000,
            calls: vec![Call {
                to: TxKind::Call(token_addr),
                value: U256::ZERO,
                input: Bytes::from(transfer_calldata),
            }],
            access_list: Default::default(),
            nonce_key: U256::ZERO,
            nonce,
            valid_before: Some(valid_before),
            valid_after: Some(valid_after),
            fee_token: None,
            fee_payer_signature: None,
            ..Default::default()
        };

        // Keep a clone for potential retries
        let tx_template = tx.clone();

        // Sign
        let hash = tx.signature_hash();
        let signature = client.signer.sign_hash(&hash).await?;
        let tempo_sig = TempoSignature::from(signature);

        // Wrap & Encode
        let signed_tx = tx.into_signed(tempo_sig);
        let mut signed_buf = Vec::new();
        signed_tx.eip2718_encode(&mut signed_buf);

        // Broadcast with retry logic for nonce errors
        let mut last_error = None;
        let max_retries = 3;
        let mut attempt = 0;

        let tx_hash = loop {
            match client.provider.send_raw_transaction(&signed_buf).await {
                Ok(pending) => {
                    break *pending.tx_hash();
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    attempt += 1;

                    if (err_str.contains("nonce too low") || err_str.contains("already known"))
                        && attempt < max_retries
                    {
                        tracing::warn!(
                            "Nonce error on raw tx send, attempt {}/{}, resetting cache...",
                            attempt,
                            max_retries
                        );

                        // Reset nonce cache and wait
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

                        // Rebuild transaction with fresh nonce
                        let fresh_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
                        let mut updated_tx = tx_template.clone();
                        updated_tx.nonce = fresh_nonce;

                        // Re-sign with new nonce
                        let hash = updated_tx.signature_hash();
                        let signature = client.signer.sign_hash(&hash).await?;
                        let tempo_sig = TempoSignature::from(signature);
                        let signed_tx = updated_tx.into_signed(tempo_sig);
                        signed_buf.clear();
                        signed_tx.eip2718_encode(&mut signed_buf);

                        last_error = Some(e);
                        continue;
                    } else {
                        return Err(e).context("Failed to send raw Tempo tx");
                    }
                }
            }
        };

        tracing::debug!("  -> Tx sent: {:?} (Valid after: {})", tx_hash, valid_after);

        Ok(TaskResult {
            success: true,
            message: format!(
                "Scheduled meme transfer sent: {:?} (valid_after: {})",
                tx_hash, valid_after
            ),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
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
