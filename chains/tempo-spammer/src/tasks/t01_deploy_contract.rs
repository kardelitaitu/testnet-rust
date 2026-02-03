//! Deploy Contract Task
//!
//! Deploys a minimal Counter contract to the Tempo blockchain.

use crate::tasks::prelude::*;
use alloy::rpc::types::TransactionRequest;
use alloy_primitives::{Address, Bytes, U256};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use std::str::FromStr;

#[derive(Debug, Clone, Default)]
pub struct DeployContractTask;

impl DeployContractTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for DeployContractTask {
    fn name(&self) -> &'static str {
        "01_deploy_contract"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;

        let bytecode_hex = "608060405234801561001057600080fd5b5061012a806100206000396000f3fe6080604052348015600f57600080fd5b506004361060325760003560e01c8063368b8772146037578063d826f88a146068575b600080fd5b606660048036038101906062919060ba565b600055565b60005460749060d6565b60405180910390f35b600080fd5b609e8160eb565b811460a857600080fd5b50565b600081359050610bc565b600080fd5b6000601f19601f83011690549093919060d6560";
        let bytecode =
            hex::decode(bytecode_hex).map_err(|e| anyhow!("Invalid bytecode hex: {}", e))?;
        let bytecode = Bytes::from(bytecode);

        // Send with retry logic for nonce errors using explicit nonce management
        let mut attempt = 0;
        let max_retries = 3;
        let pending = loop {
            // Get fresh nonce BEFORE building transaction
            let nonce = match client.get_pending_nonce(&ctx.config.rpc_url).await {
                Ok(n) => n,
                Err(e) => {
                    attempt += 1;
                    tracing::error!(
                        "Failed to get nonce for deploy (attempt {}/{}): {}",
                        attempt,
                        max_retries,
                        e
                    );
                    if attempt >= max_retries {
                        return Err(e).context("Failed to get nonce after max retries");
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    continue;
                }
            };

            let mut tx = TransactionRequest::default()
                .input(bytecode.clone().into())
                .from(ctx.address())
                .nonce(nonce) // EXPLICIT NONCE - prevents race conditions
                .gas_limit(500_000);
            tx.to = Some(alloy::primitives::TxKind::Create);

            match client.provider.send_transaction(tx).await {
                Ok(p) => break p,
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    attempt += 1;

                    if (err_str.contains("nonce too low") || err_str.contains("already known"))
                        && attempt < max_retries
                    {
                        tracing::warn!(
                            "Nonce error on contract deploy, attempt {}/{}, resetting cache...",
                            attempt,
                            max_retries
                        );

                        // Reset nonce cache and wait
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        continue;
                    } else {
                        return Err(e).context("Failed to send deployment transaction");
                    }
                }
            }
        };

        let tx_hash = pending.tx_hash().clone();

        if let Some(db) = &ctx.db {
            db.log_counter_contract_creation(
                &ctx.address().to_string(),
                &format!("{:?}", tx_hash),
                ctx.chain_id(),
            )
            .await?;
        }

        Ok(TaskResult {
            success: true,
            message: format!("Contract deployed: {:?}", tx_hash),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
