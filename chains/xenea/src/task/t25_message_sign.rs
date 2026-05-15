use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use hex::encode as hex_encode;

pub struct MessageSignTask;

impl MessageSignTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for MessageSignTask {
    fn name(&self) -> &str {
        "25_messageSign"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let message = format!("Hello RISE from {:?}", address);
        let message_hash = ethers::utils::hash_message(&message);

        let signature = wallet
            .sign_hash(message_hash)
            .context("Failed to sign message")?;

        let recovered = signature
            .recover(message_hash)
            .context("Failed to recover signer")?;

        let is_valid = recovered == address;

        let signature_hex = hex_encode(signature.to_vec());

        Ok(TaskResult {
            success: is_valid,
            message: format!(
                "Signed '{}' | Valid: {} | Sig: {}...",
                message,
                is_valid,
                &signature_hex[..12]
            ),
            tx_hash: None,
        })
    }
}
