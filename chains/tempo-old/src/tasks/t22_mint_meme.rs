use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IMemeMint,
    r#"[
        function mint(address to, uint256 amount)
        function grantRole(bytes32 role, address account)
        function hasRole(bytes32 role, address account) view returns (bool)
        function decimals() view returns (uint8)
        function symbol() view returns (string)
    ]"#
);

pub struct MintMemeTask;

#[async_trait]
impl TempoTask for MintMemeTask {
    fn name(&self) -> &str {
        "22_mint_meme"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet_addr = ctx.wallet.address();

        // 1. Get Meme Asset from DB
        let assets = if let Some(db) = ctx.db.as_ref() {
            db.get_assets_by_type(&format!("{:?}", wallet_addr), "meme")
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };

        if assets.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No created meme tokens found in DB to mint.".to_string(),
                tx_hash: None,
            });
        }

        let asset_addr_str = { assets[rand::thread_rng().gen_range(0..assets.len())].clone() };
        let token_address = Address::from_str(&asset_addr_str)?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let meme = IMemeMint::new(token_address, client.clone());

        let symbol = meme
            .symbol()
            .call()
            .await
            .unwrap_or_else(|_| "???".to_string());
        let decimals = meme.decimals().call().await.unwrap_or(6);
        let amount_base = rand::thread_rng().gen_range(1000..5000);
        let amount_wei = U256::from(amount_base) * U256::exp10(decimals as usize);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // 2. Ensure Role
        let issuer_role = ethers::utils::keccak256("ISSUER_ROLE");
        let has_role = meme
            .has_role(issuer_role, wallet_addr)
            .call()
            .await
            .unwrap_or(false);
        if !has_role {
            println!("Granting ISSUER_ROLE for {}...", symbol);
            meme.grant_role(issuer_role, wallet_addr)
                .gas_price(bumped_gas_price)
                .send()
                .await?
                .await?;
        }

        // 3. Mint
        println!("Minting {} {}...", amount_base, symbol);
        let tx = meme
            .mint(wallet_addr, amount_wei)
            .gas_price(bumped_gas_price);
        let pending = tx.send().await?;
        let receipt = pending.await?.context("Mint failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!("Minted {} {}. Tx: {}", amount_base, symbol, hash),
            tx_hash: Some(hash),
        })
    }
}
