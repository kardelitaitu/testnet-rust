//! TIP-403 Policies Task
//!
//! Creates a TIP-403 whitelist policy.
//!
//! Workflow:
//! 1. Call createPolicy on TIP-403 registry
//! 2. Wallet becomes admin of the policy

use crate::TempoClient;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::Address;
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy_sol_types::SolCall;
use alloy_sol_types::sol;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::str::FromStr;

sol!(
    interface ITIP403Registry {
        function createPolicy(address admin, uint8 policyType) returns (uint64 policyId);
    }
);

const TIP403_REGISTRY_ADDRESS: &str = "0x403c000000000000000000000000000000000000";

#[derive(Debug, Clone, Default)]
pub struct Tip403PoliciesTask;

impl Tip403PoliciesTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for Tip403PoliciesTask {
    fn name(&self) -> &'static str {
        "18_tip403_policies"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        let registry_addr =
            Address::from_str(TIP403_REGISTRY_ADDRESS).context("Invalid TIP403 registry")?;

        tracing::debug!("Creating TIP-403 Whitelist Policy...");

        // Policy Type 0 = Whitelist
        let policy_type = 0u8;

        let call = ITIP403Registry::createPolicyCall {
            admin: address,
            policyType: policy_type,
        };
        let calldata = call.abi_encode();

        let tx = TransactionRequest::default()
            .to(registry_addr)
            .input(TransactionInput::from(calldata))
            .from(address)
            .max_fee_per_gas(150_000_000_000u128)
            .max_priority_fee_per_gas(1_500_000_000u128);

        // Send with retry logic for nonce errors (1 retry)
        let pending = match client.provider.send_transaction(tx.clone()).await {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!(
                        "Nonce error on tip403_policies, resetting cache and retrying..."
                    );
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client
                        .provider
                        .send_transaction(tx)
                        .await
                        .context("Failed to send createPolicy")?
                } else {
                    return Err(e).context("Failed to send createPolicy");
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
                message: "TIP-403 Policy creation reverted".to_string(),
                tx_hash: Some(format!("{:?}", tx_hash)),
            });
        }

        // println!(
        //     "✅ TIP-403 Policy created: {:?} (Block {:?})",
        //     tx_hash, receipt.block_number
        // );

        Ok(TaskResult {
            success: true,
            message: format!("Created TIP-403 Whitelist Policy. Tx: {}", tx_hash),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
