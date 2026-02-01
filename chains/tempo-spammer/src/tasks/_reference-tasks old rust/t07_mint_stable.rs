use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    ITIP20Mintable,
    r#"[
        function mint(address to, uint256 amount)
        function hasRole(bytes32 role, address account) view returns (bool)
        function grantRole(bytes32 role, address account)
        function decimals() view returns (uint8)
        function symbol() view returns (string)
    ]"#
);

pub struct MintStableTask;

#[async_trait]
impl TempoTask for MintStableTask {
    fn name(&self) -> &str {
        "07_mint_stable"
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
                message: "No created stablecoins found in DB tasks to mint.".to_string(),
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
        let token = ITIP20Mintable::new(token_address, client.clone());

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);
        let address = ctx.wallet.address();

        // 2. Check Role
        let issuer_role = ethers::utils::keccak256("ISSUER_ROLE".as_bytes());
        let has_role = token
            .has_role(issuer_role, address)
            .call()
            .await
            .unwrap_or(false);

        if !has_role {
            println!("Granting ISSUER_ROLE to {:?}...", address);
            let tx_grant = token
                .grant_role(issuer_role, address)
                .gas_price(bumped_gas_price);
            let pending_grant = tx_grant.send().await?;
            pending_grant.await?;
            println!("Role granted.");
        }

        // 3. Mint Random Amount (10M - 20M)
        let decimals = token.decimals().call().await.unwrap_or(18);
        let amount_base = {
            let mut rng = rand::thread_rng();
            rng.gen_range(10_000_000..20_000_000)
        };
        let amount_wei = U256::from(amount_base) * U256::exp10(decimals as usize);

        let symbol = token
            .symbol()
            .call()
            .await
            .unwrap_or_else(|_| "???".to_string());
        println!("Minting {} {} to {:?}", amount_base, symbol, address);

        let tx_mint = token.mint(address, amount_wei).gas_price(bumped_gas_price);
        let pending_mint = tx_mint.send().await?;
        let receipt = pending_mint.await?.context("Mint failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!(
                "Minted {} {} to {:?}. Tx: {}",
                amount_base, symbol, address, hash
            ),
            tx_hash: Some(hash),
        })
    }
}
