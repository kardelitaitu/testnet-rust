//! Deploy Storm Task
//!
//! Rapidly deploys 10-20 minimal contracts concurrently.
//! Uses manual nonce management to stress test the mempool.

use crate::TempoClient;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::U256;
use alloy::rpc::types::TransactionRequest;
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::future::join_all;
use rand::Rng;

const MINIMAL_BYTECODE: &str = "608060405234801561001057600080fd5b5061012a806100206000396000f3fe6080604052348015600f57600080fd5b506004361060325760003560e01c8063368b8772146037578063d826f88a146068575b600080fd5b606660048036038101906062919060ba565b600055565b60005460749060d6565b60405180910390f35b600080fd5b609e8160eb565b811460a857600080fd5b50565b600081359050610bc565b600080fd5b6000601f19601f83011690549093919060d6560";

#[derive(Debug, Clone, Default)]
pub struct DeployStormTask;

impl DeployStormTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for DeployStormTask {
    fn name(&self) -> &'static str {
        "50_deploy_storm"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        use alloy::primitives::Address;

        let client = &ctx.client;
        let address = ctx.address();

        let mut rng = rand::rngs::OsRng;
        let storm_size = rng.gen_range(10..20);

        tracing::debug!("Starting DEPLOY STORM (Size: {})...", storm_size);

        let bytecode = hex::decode(MINIMAL_BYTECODE).context("Invalid hex")?;

        // 1. Get Base Nonce
        let base_nonce = client
            .get_pending_nonce(&ctx.config.rpc_url)
            .await
            .context("Failed to get pending nonce")?;

        // 2. Prepare Futures
        let mut futures = Vec::new();

        for i in 0..storm_size {
            let mut deploy_tx = TransactionRequest::default()
                .input(bytecode.clone().into())
                .from(address)
                .nonce(base_nonce + i as u64)
                .max_fee_per_gas(200_000_000_000u128)
                .max_priority_fee_per_gas(2_000_000_000u128)
                .gas_limit(2_000_000);
            deploy_tx.to = Some(alloy::primitives::TxKind::Create);

            // println!("  -> Launching missile {}/{}", i+1, storm_size);
            futures.push(client.provider.send_transaction(deploy_tx));
        }

        // 3. Launch All
        let results = join_all(futures).await;

        let mut success_count = 0;
        let mut last_hash = String::new();

        for (i, res) in results.into_iter().enumerate() {
            match res {
                Ok(pending) => {
                    success_count += 1;
                    last_hash = format!("{:?}", pending.tx_hash());
                    // We don't wait for receipts in a storm, just broadcast
                }
                Err(_e) => {
                    // println!("  X Missile {} failed: {:?}", i+1, _e);
                }
            }
        }

        // Update Nonce Manager (next nonce = base + actual successful count)
        if let Some(manager) = &client.nonce_manager {
            manager.set(address, base_nonce + success_count).await;
        }

        Ok(TaskResult {
            success: success_count > 0,
            message: format!(
                "Deploy Storm: {}/{} launched successfully.",
                success_count, storm_size
            ),
            tx_hash: if last_hash.is_empty() {
                None
            } else {
                Some(last_hash)
            },
        })
    }
}
