use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;

pub struct VerifySignatureTask;

impl VerifySignatureTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for VerifySignatureTask {
    fn name(&self) -> &str {
        "26_verifySignature"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let mut rng = OsRng;
        let random_value: u64 = rng.gen();
        let message = format!("Verify signature test #{}", random_value);
        let message_hash = ethers::utils::hash_message(&message);

        let signature = wallet
            .sign_hash(message_hash)
            .context("Failed to sign message")?;

        let recovered = signature
            .recover(message_hash)
            .context("Failed to recover signer")?;

        let is_valid = recovered == address;

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;

        let data = Bytes::from(ethers::abi::encode(&[ethers::abi::Token::String(
            message.clone(),
        )]));

        let tx = Eip1559TransactionRequest::new()
            .to(address)
            .value(0)
            .data(data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let client = std::sync::Arc::new(SignerMiddleware::new(
            std::sync::Arc::new(provider.clone()),
            wallet.clone(),
        ));
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)) && is_valid,
            message: format!(
                "Signature verification: {} (on-chain tx: {:?}). Signature valid: {}",
                message, receipt.transaction_hash, is_valid
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
