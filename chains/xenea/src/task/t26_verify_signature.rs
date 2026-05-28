use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
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

        let signature = wallet.sign_hash(message_hash).context("Failed to sign message")?;

        let recovered = signature.recover(message_hash).context("Failed to recover signer")?;

        let is_valid = recovered == address;

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;
        let estimated_gas = gas_limit * gas_price;

        // 1. Balance check
        let balance = provider.get_balance(address, None).await?;
        if balance <= estimated_gas {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient TXENE for gas: need {} Wei, have {} Wei",
                    estimated_gas, balance
                ),
                tx_hash: None,
            });
        }

        // 2. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(Arc::new(provider.clone()), address);
        let nonce = nonce_manager.next().await?;

        let data = Bytes::from(ethers::abi::encode(&[ethers::abi::Token::String(message.clone())]));

        let tx = TransactionRequest::new()
            .to(address)
            .value(0)
            .data(data)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .from(address);

        // 3. Send (fire-and-forget)
        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => {
                let sig_status = if is_valid { "valid" } else { "INVALID" };
                Ok(TaskResult {
                    success: true,
                    message: format!(
                        "Signature {} (recovered: {:?}), message: {} (tx: {:?})",
                        sig_status,
                        recovered,
                        message,
                        pending.tx_hash()
                    ),
                    tx_hash: Some(format!("{:?}", pending.tx_hash())),
                })
            },
            Err(e) => {
                debug!("VerifySignature tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit verify signature tx: {}", e),
                    tx_hash: None,
                })
            },
        }
    }
}
