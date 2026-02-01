use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IAccessControlMinimal,
    r#"[
        function grantRole(bytes32 role, address account)
        function hasRole(bytes32 role, address account) view returns (bool)
        function symbol() view returns (string)
    ]"#
);

pub struct GrantRoleTask;

#[async_trait]
impl TempoTask for GrantRoleTask {
    fn name(&self) -> &str {
        "13_grant_role"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        // 1. Get User Assets from DB
        let assets = if let Some(db) = ctx.db.as_ref() {
            db.get_assets_by_type(&format!("{:?}", ctx.wallet.address()), "stablecoin")
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };

        if assets.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No created stablecoins found in DB to grant roles.".to_string(),
                tx_hash: None,
            });
        }

        // Pick Random Asset
        let asset_addr_str = {
            let mut rng = rand::thread_rng();
            assets[rng.gen_range(0..assets.len())].clone()
        };
        let token_address = Address::from_str(&asset_addr_str)?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let token = IAccessControlMinimal::new(token_address, client.clone());

        let address = ctx.wallet.address();

        // 2. Select Role (ISSUER_ROLE or PAUSE_ROLE)
        let is_issuer = rand::thread_rng().gen_bool(0.8);
        let role_name = if is_issuer {
            "ISSUER_ROLE"
        } else {
            "PAUSE_ROLE"
        };
        let role_hash = ethers::utils::keccak256(role_name.as_bytes());

        // 3. Check Role
        let has_role = token
            .has_role(role_hash, address)
            .call()
            .await
            .unwrap_or(false);

        if has_role {
            return Ok(TaskResult {
                success: true,
                message: format!("Role {} already granted on {}.", role_name, asset_addr_str),
                tx_hash: None,
            });
        }

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        println!(
            "Granting {} to {:?} on token {}...",
            role_name, address, asset_addr_str
        );

        let tx = token
            .grant_role(role_hash, address)
            .gas_price(bumped_gas_price);
        let pending = tx.send().await?;
        let receipt = pending.await?.context("GrantRole failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!("Granted {} on {}. Tx: {}", role_name, asset_addr_str, hash),
            tx_hash: Some(hash),
        })
    }
}
