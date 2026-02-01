//! Transfer Later Stable Task
//!
//! Schedules a stablecoin transfer with a delay.
//!
//! Workflow:
//! 1. Generate random recipient
//! 2. Sleep for random delay
//! 3. Execute stable token transfer

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask, get_random_address};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use rand::seq::SliceRandom;
use std::str::FromStr;
use std::time::Duration;

const TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

#[derive(Debug, Clone, Default)]
pub struct TransferLaterStableTask;

impl TransferLaterStableTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for TransferLaterStableTask {
    fn name(&self) -> &'static str {
        "38_transfer_later_stable"
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

        let mut token_addr = TempoTokens::get_path_usd_address();
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
                            using_created_token = true;
                        }
                    }
                }
            }
        }

        if using_created_token {
            tracing::debug!("Using random created stablecoin: {:?}", token_addr);
        } else {
            tracing::debug!("Using default stablecoin (PathUSD): {:?}", token_addr);
        }

        let mut rng = rand::rngs::OsRng;
        let delay = rng.gen_range(3..=5); // Random 3-5 seconds
        let recipient = get_random_address()?;

        let balance = TempoTokens::get_token_balance(client, token_addr, address).await?;
        let amount = balance / U256::from(100);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let valid_after = now + delay;
        let valid_before = valid_after + 300;

        tracing::debug!(
            "Scheduling stable transfer of {} units to {:?} valid after +{}s (native)...",
            amount,
            recipient,
            delay
        );

        // Get nonce
        let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;

        // Get gas price
        let gas_price = client.provider.get_gas_price().await?;
        let max_fee = U256::from(gas_price) * U256::from(120) / U256::from(100);

        let transfer_calldata = build_transfer_calldata(recipient, amount);

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

        // Sign
        let hash = tx.signature_hash();
        let signature = client.signer.sign_hash(&hash).await?;
        let tempo_sig = TempoSignature::from(signature);

        // Wrap & Encode
        let signed_tx = tx.into_signed(tempo_sig);
        let mut signed_buf = Vec::new();
        signed_tx.eip2718_encode(&mut signed_buf);

        // Broadcast
        let pending = client
            .provider
            .send_raw_transaction(&signed_buf)
            .await
            .context("Failed to send raw Tempo tx")?;
        let tx_hash = *pending.tx_hash();

        tracing::debug!("  -> Tx sent: {:?} (Valid after: {})", tx_hash, valid_after);

        // Update nonce manager to prevent drift
        if let Some(manager) = &client.nonce_manager {
            manager.set(address, nonce + 1).await;
        }

        Ok(TaskResult {
            success: true,
            message: format!(
                "Scheduled stable transfer sent: {:?} (valid_after: {})",
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
