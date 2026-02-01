//! Time Bomb Task
//!
//! Sets a delayed "detonation" by broadcasting a transaction with `valid_after`.
//! The transaction will remain in the mempool until the time is reached.
//!
//! Workflow:
//! 1. Generate random delay (20-30s)
//! 2. Construct TempoTransaction with `valid_after`
//! 3. Broadcast immediately (Arming the bomb)

use crate::TempoClient;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;

// Minimal Contract Bytecode (STOP opcode pattern from Node.js reference)
const MINIMAL_BYTECODE: &str = "60008060093d393df3";

#[derive(Debug, Clone, Default)]
pub struct TimeBombTask;

impl TimeBombTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for TimeBombTask {
    fn name(&self) -> &'static str {
        "49_time_bomb"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        use alloy::primitives::{Bytes, TxKind, U256};
        use alloy::providers::Provider;
        use alloy::rlp::Encodable;
        use alloy::signers::Signer;
        use tempo_primitives::transaction::{Call, TempoSignature, TempoTransaction};

        let client = &ctx.client;
        let chain_id = ctx.chain_id();
        let address = ctx.address();

        let mut rng = rand::rngs::OsRng;
        let delay = rng.gen_range(20..30); // 20-30 seconds delay

        // 1. Calculate Timestamps
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let valid_after = now + delay;
        let valid_before = valid_after + 300; // 5 minute window

        tracing::debug!("Arming Time Bomb (Explosion in {}s)...", delay);

        // 2. Prepare Deployment Transaction
        let bytecode = hex::decode(MINIMAL_BYTECODE).context("Invalid hex")?;

        let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let gas_price = client.provider.get_gas_price().await?;
        // println!("Current Gas Price: {}", gas_price);

        let tx = TempoTransaction {
            chain_id,
            max_priority_fee_per_gas: 1_500_000_000,
            max_fee_per_gas: 200_000_000_000u128, // High gas for priority
            gas_limit: 100_000,                   // Sufficient for minimal deploy
            calls: vec![Call {
                to: TxKind::Create, // Deployment
                value: U256::ZERO,
                input: Bytes::from(bytecode),
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

        // 3. Sign
        let hash = tx.signature_hash();
        let signature = client.signer.sign_hash(&hash).await?;
        let tempo_sig = TempoSignature::from(signature);

        // 4. Wrap & Encode
        let signed_tx = tx.into_signed(tempo_sig);
        let mut signed_buf = Vec::new();
        signed_tx.eip2718_encode(&mut signed_buf);

        // 5. Broadcast (Fire & Forget, or return hash)
        // Since it's a Time Bomb, it won't confirm immediately.
        // We broadcast and return the hash.
        let pending = client
            .provider
            .send_raw_transaction(&signed_buf)
            .await
            .context("Failed to arm time bomb")?;

        let tx_hash = *pending.tx_hash();

        Ok(TaskResult {
            success: true,
            message: format!(
                "Time Bomb ARMED! Scheduled for detonation at T+{}s (timestamp: {}). Tx: {:?}",
                delay, valid_after, tx_hash
            ),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
