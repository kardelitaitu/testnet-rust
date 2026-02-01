use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::str::FromStr;

ethers::contract::abigen!(
    IViralNft,
    r#"[
        function claim()
        function balanceOf(address owner) view returns (uint256)
        function ownerOf(uint256 tokenId) view returns (address)
        function tokenURI(uint256 tokenId) view returns (string memory)
    ]"#
);

pub struct MintViralNftTask;

#[async_trait]
impl TempoTask for MintViralNftTask {
    fn name(&self) -> &str {
        "48_mint_viral_nft"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet_addr = ctx.wallet.address();

        // Get NFT Asset
        let assets = if let Some(db) = ctx.db.as_ref() {
            db.get_assets_by_type(&format!("{:?}", wallet_addr), "viral_nft")
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };

        if assets.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No viral NFTs found in DB to mint.".to_string(),
                tx_hash: None,
            });
        }

        let contract_addr = Address::from_str(&assets[0])?;
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let nft = IViralNft::new(contract_addr, client.clone());

        println!("Minting Viral NFT at {:?}...", contract_addr);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        let tx = nft.claim().gas_price(bumped_gas_price);
        let receipt = tx.send().await?.await?.context("Mint failed")?;

        Ok(TaskResult {
            success: true,
            message: "Minted Viral NFT.".to_string(),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
