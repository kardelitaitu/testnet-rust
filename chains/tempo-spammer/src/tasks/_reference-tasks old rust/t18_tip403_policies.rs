use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::str::FromStr;

ethers::contract::abigen!(
    ITIP403Registry,
    r#"[
        function createPolicy(address admin, uint8 policyType) returns (uint64)
    ]"#
);

pub struct Tip403PoliciesTask;

#[async_trait]
impl TempoTask for Tip403PoliciesTask {
    fn name(&self) -> &str {
        "18_tip403_policies"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let registry_addr = Address::from_str("0x40c0000000000000000000000000000000000000")?; // Mock Registry

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let registry = ITIP403Registry::new(registry_addr, client.clone());

        let wallet_addr = ctx.wallet.address();

        println!("Creating TIP-403 Whitelist Policy...");

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // policyType 0 = Whitelist
        let tx = registry
            .create_policy(wallet_addr, 0)
            .gas_price(bumped_gas_price);
        let pending = tx.send().await?;
        let receipt = pending.await?.context("TIP-403 Policy creation failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!("Created TIP-403 Policy. Tx: {}", hash),
            tx_hash: Some(hash),
        })
    }
}
