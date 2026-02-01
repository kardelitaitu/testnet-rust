//! Batch Send Transaction Task
//!
//! Sends a batch of system token transfers sequentially.
//!
//! Workflow:
//! 1. Generate random recipients
//! 2. Send transactions sequentially
//! 3. Collect results

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask, get_random_address};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::TransactionRequest;
use anyhow::Result;
use async_trait::async_trait;
use rand::Rng;

#[derive(Debug, Clone, Default)]
pub struct BatchSendTransactionTask;

impl BatchSendTransactionTask {
    pub fn new() -> Self {
        Self
    }
}

const MINT_SELECTOR: [u8; 4] = [0x40, 0xc1, 0x0f, 0x19];

#[async_trait]
impl TempoTask for BatchSendTransactionTask {
    fn name(&self) -> &'static str {
        "34_batch_send_transaction"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        use alloy::primitives::{Address, Bytes, TxKind, U256};
        use alloy::providers::Provider;
        use alloy::rlp::Encodable;
        use alloy::signers::Signer;
        use tempo_primitives::transaction::{Call, TempoSignature, TempoTransaction};

        let client = &ctx.client;
        let address = ctx.address();
        let chain_id = ctx.chain_id();

        // 1. Randomize Transfer and Fee Tokens
        let transfer_token = TempoTokens::get_random_system_token();
        let transfer_addr = transfer_token.address;

        let mut rng = rand::rngs::OsRng;
        // 50% chance for Native fee, 50% for high-probability System Token fee
        let fee_token = if rng.gen_bool(0.5) {
            None
        } else {
            Some(TempoTokens::get_random_system_token())
        };

        let count = rng.gen_range(5..10);

        // 2. Fetch Balance and Calculate Amount (1% per recipient)
        let balance = TempoTokens::get_token_balance(client, transfer_addr, address)
            .await
            .unwrap_or(U256::ZERO);

        let amount_per_recipient = balance / U256::from(100); // 1% of balance
        let total_transfer_needed = amount_per_recipient * U256::from(count);

        let fee_symbol = fee_token
            .as_ref()
            .map(|t| t.symbol.clone())
            .unwrap_or_else(|| "Native".to_string());

        tracing::info!(
            "Burst: {} -> {} (Fee: {}) | {} txs of {} each",
            address,
            transfer_token.symbol,
            fee_symbol,
            count,
            amount_per_recipient
        );

        // 3. Auto-Mint if insufficient balance
        if balance < total_transfer_needed || amount_per_recipient.is_zero() {
            tracing::debug!("Auto-minting {} for transfers...", transfer_token.symbol);
            let mint_amount = if amount_per_recipient.is_zero() {
                U256::from(10_000_000_000_000_000u64) // Minimum viable amount
            } else {
                total_transfer_needed * U256::from(10)
            };
            let mint_call = build_mint_calldata(address, mint_amount);
            let tx = TransactionRequest::default()
                .to(transfer_addr)
                .input(alloy::rpc::types::TransactionInput::from(mint_call))
                .from(address);
            if let Ok(pending) = client.provider.send_transaction(tx).await {
                let _ = pending.get_receipt().await;
            }
        }

        // Fee Token Balance (if not native)
        if let Some(ref ft) = fee_token {
            let ft_bal = TempoTokens::get_token_balance(client, ft.address, address)
                .await
                .unwrap_or(U256::ZERO);
            if ft_bal < U256::from(1_000_000_000_000_000u64) {
                tracing::debug!("Auto-minting {} for fees...", ft.symbol);
                let mint_call = build_mint_calldata(address, U256::from(10_000_000_000_000_000u64));
                let tx = TransactionRequest::default()
                    .to(ft.address)
                    .input(alloy::rpc::types::TransactionInput::from(mint_call))
                    .from(address);
                if let Ok(pending) = client.provider.send_transaction(tx).await {
                    let _ = pending.get_receipt().await;
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

        // 3. Prepare Randomized Pipeline (TempoTransaction)
        let mut current_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let start_nonce = current_nonce; // Capture for tracking
        let gas_price = client.provider.get_gas_price().await?;
        let max_fee = (gas_price * 125) / 100;
        let mut burst_payloads = Vec::new();

        for _ in 0..count {
            let recipient = get_random_address()?;
            let calldata = build_transfer_calldata(recipient, amount_per_recipient);

            let tx = TempoTransaction {
                chain_id,
                nonce: current_nonce,
                max_fee_per_gas: max_fee,
                max_priority_fee_per_gas: 1_500_000_000,
                gas_limit: 150_000,
                calls: vec![Call {
                    to: TxKind::Call(transfer_addr),
                    value: U256::ZERO,
                    input: Bytes::from(calldata),
                }],
                fee_token: fee_token.as_ref().map(|t| t.address),
                ..Default::default()
            };

            let hash = tx.signature_hash();
            let sig = client.signer.sign_hash(&hash).await?;
            let signed_tx = tx.into_signed(TempoSignature::from(sig));

            let mut buf = Vec::new();
            signed_tx.eip2718_encode(&mut buf);
            burst_payloads.push(buf);

            current_nonce += 1;
        }

        // 4. Blast Raw Transactions
        let mut last_submitted_nonce = start_nonce.wrapping_sub(1);
        let mut submission_count = 0;
        let mut last_hash = String::new();

        let mut first_error = None;

        for (idx, payload) in burst_payloads.iter().enumerate() {
            let tx_nonce = start_nonce + idx as u64;
            match client.provider.send_raw_transaction(payload).await {
                Ok(pending) => {
                    last_hash = pending.tx_hash().to_string();
                    last_submitted_nonce = tx_nonce;
                    submission_count += 1;
                }
                Err(e) => {
                    tracing::error!("Tempo Pipelined at nonce {} failure: {}", tx_nonce, e);
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                    break; // CRITICAL: Stop on first failure - nonces must be sequential
                }
            }
        }

        // 5. Update Nonce Manager with next nonce after last successful submission
        if let Some(manager) = &client.nonce_manager {
            let next_nonce = last_submitted_nonce.wrapping_add(1);
            manager.set(address, next_nonce).await;
        }

        if submission_count == 0 {
            if let Some(err) = first_error {
                return Err(anyhow::anyhow!(err));
            }
            anyhow::bail!("Failed to submit any randomized Tempo transactions.");
        }

        Ok(TaskResult {
            success: true,
            message: format!(
                "Randomized Burst: {}/{} {} transfers via {}.",
                submission_count, count, transfer_token.symbol, fee_symbol
            ),
            tx_hash: Some(last_hash),
        })
    }
}

fn build_transfer_calldata(to: Address, amount: U256) -> Vec<u8> {
    const TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];
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
