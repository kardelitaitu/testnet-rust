//! Transfer with Memo Task
//!
//! Transfers system tokens with a bytes32 memo attached.
//! Uses transferWithMemo(address to, uint256 amount, bytes32 memo)
//!
//! Workflow:
//! 1. Check PathUSD balance, ensure >= 50 units
//! 2. Generate random memo (Memo/Note/Message + # + 3-4 digit number)
//! 3. Execute transferWithMemo(to, amount, memo)
//! 4. Verify transaction success

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask, get_random_address};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy_sol_types::SolCall;
use alloy_sol_types::sol;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use std::str::FromStr;

sol!(
    interface ITransferWithMemo {
        function transferWithMemo(address to, uint256 amount, bytes32 memo);
    }
);

#[derive(Debug, Clone, Default)]
pub struct TransferMemoTask;

impl TransferMemoTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for TransferMemoTask {
    fn name(&self) -> &'static str {
        "10_transfer_memo"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        const PATHUSD_ADDR: &str = "0x20c0000000000000000000000000000000000000";
        let token_addr = Address::from_str(PATHUSD_ADDR).context("Invalid PathUSD address")?;
        let token_decimals = TempoTokens::get_token_decimals(client, token_addr).await?;

        let mut balance = U256::ZERO;
        for attempt in 1..=3 {
            balance = TempoTokens::get_token_balance(client, token_addr, address).await?;
            if !balance.is_zero() {
                break;
            }
            if attempt < 3 {
                // println!("Balance is 0, retrying check ({}/3)...", attempt);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }

        // Debugging log if still zero
        if balance.is_zero() {
            tracing::warn!(
                "WARNING: PathUSD Balance is 0 for {:?} even after retries.",
                address
            );
        }

        let min_balance = U256::from(50) * U256::from(10_u64.pow(token_decimals as u32));

        if balance < min_balance {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient PathUSD balance (have {}, need {})",
                    TempoTokens::format_amount(balance, token_decimals),
                    TempoTokens::format_amount(min_balance, token_decimals)
                ),
                tx_hash: None,
            });
        }

        let amount_units = rand::rngs::OsRng.gen_range(10..51);
        let amount_wei = U256::from(amount_units) * U256::from(10_u64.pow(token_decimals as u32));
        let actual_amount = if balance < amount_wei {
            balance / U256::from(2)
        } else {
            amount_wei
        };

        let recipient = get_random_address()?;
        let memo = get_random_memo();
        let recipient_formatted = format!("{:?}", recipient);
        let recipient_short = recipient_formatted.get(..14).unwrap_or("?");

        // println!(
        //     "Transferring {} PathUSD to {} with memo: \"{}\"",
        //     TempoTokens::format_amount(actual_amount, token_decimals),
        //     recipient_short,
        //     memo
        // );

        // Convert memo string to bytes32
        let memo_bytes = string_to_bytes32(&memo);

        // Build transferWithMemo call using alloy
        let call = ITransferWithMemo::transferWithMemoCall {
            to: recipient,
            amount: actual_amount,
            memo: alloy_primitives::B256::from(memo_bytes),
        };
        let calldata = call.abi_encode();
        let calldata_for_retry = calldata.clone();

        let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let tx = TransactionRequest::default()
            .to(token_addr)
            .input(TransactionInput::from(calldata))
            .from(address)
            .nonce(nonce)
            .max_fee_per_gas(150_000_000_000u128)
            .max_priority_fee_per_gas(1_500_000_000u128);

        // Send with retry logic for nonce errors (1 retry)
        let pending = match client.provider.send_transaction(tx.clone()).await {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on transfer_memo, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    // Rebuild tx with fresh nonce
                    let fresh_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
                    let retry_tx = TransactionRequest::default()
                        .to(token_addr)
                        .input(TransactionInput::from(calldata_for_retry))
                        .from(address)
                        .nonce(fresh_nonce)
                        .max_fee_per_gas(150_000_000_000u128)
                        .max_priority_fee_per_gas(1_500_000_000u128);
                    client
                        .provider
                        .send_transaction(retry_tx)
                        .await
                        .context("Failed to send transfer with memo")?
                } else {
                    return Err(e).context("Failed to send transfer with memo");
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
                message: "Transfer with memo reverted".to_string(),
                tx_hash: Some(format!("{:?}", tx_hash)),
            });
        }

        // println!(
        //     "✅ Transfer with memo successful: {:?} (Block {:?})",
        //     tx_hash, receipt.block_number
        // );

        Ok(TaskResult {
            success: true,
            message: format!(
                "Transferred {} PathUSD to {} with memo: \"{}\"",
                TempoTokens::format_amount(actual_amount, token_decimals),
                recipient_short,
                memo
            ),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}

fn get_random_memo() -> String {
    const PREFIXES: &[&str] = &["Memo", "Note", "Message"];
    let prefix = PREFIXES[rand::rngs::OsRng.gen_range(0..PREFIXES.len())];
    let digits = rand::rngs::OsRng.gen_range(3..5);
    let min_num = 10_u64.pow(digits - 1);
    let max_num = 10_u64.pow(digits) - 1;
    let number = rand::rngs::OsRng.gen_range(min_num..=max_num);
    format!("{} #{}", prefix, number)
}

fn string_to_bytes32(s: &str) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    let s_bytes = s.as_bytes();
    let len = std::cmp::min(s_bytes.len(), 32);
    bytes[..len].copy_from_slice(&s_bytes[..len]);
    bytes
}
