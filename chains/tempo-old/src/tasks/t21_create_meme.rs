use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    ITIP20Factory,
    r#"[
        function createToken(string name, string symbol, string currency, address quoteToken, address admin, bytes32 salt) returns (address)
        event TokenCreated(address indexed token, string name, string symbol, string currency, address quoteToken, address admin, bytes32 salt)
    ]"#
);

ethers::contract::abigen!(
    IMemeToken,
    r#"[
        function mint(address to, uint256 amount)
        function grantRole(bytes32 role, address account)
        function approve(address spender, uint256 amount) returns (bool)
    ]"#
);

pub struct CreateMemeTask;

#[async_trait]
impl TempoTask for CreateMemeTask {
    fn name(&self) -> &str {
        "21_create_meme"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let factory_addr = Address::from_str("0x00c0000000000000000000000000000000000000")?; // Mock Factory
        let path_usd_addr = Address::from_str("0x20c0000000000000000000000000000000000000")?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let factory = ITIP20Factory::new(factory_addr, client.clone());
        let quote = IMemeToken::new(path_usd_addr, client.clone());

        let wallet_addr = ctx.wallet.address();

        // 1. Random Name/Symbol
        let name = format!("Meme_{}", rand::thread_rng().gen_range(100..999));
        let symbol = format!("MEME{}", rand::thread_rng().gen_range(10..99));

        println!("Creating Meme Token: {} ({})...", name, symbol);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // 2. Approve Factory
        println!("Approving PathUSD for Factory...");
        let tx_approve = quote
            .approve(factory_addr, U256::max_value())
            .gas_price(bumped_gas_price);
        tx_approve.send().await?.await?;

        // 3. Create Token
        let salt = [0u8; 32]; // Simplification
        let tx_create = factory
            .create_token(
                name.clone(),
                symbol.clone(),
                "USD".to_string(),
                path_usd_addr,
                wallet_addr,
                salt,
            )
            .gas_price(bumped_gas_price);
        let pending = tx_create.send().await?;
        let receipt = pending.await?.context("Meme creation failed")?;

        // 4. Try extract address
        let mut token_addr = Address::zero();
        for log in receipt.logs {
            if let Ok(TokenCreatedFilter { token, .. }) =
                factory.decode_event::<TokenCreatedFilter>("TokenCreated", log.topics, log.data)
            {
                token_addr = token;
                break;
            }
        }

        if token_addr.is_zero() {
            return Err(anyhow::anyhow!("Could not find TokenCreated event"));
        }

        println!(
            "Meme Token created at {:?}. Minting initial supply...",
            token_addr
        );

        // 5. Grant & Mint
        let meme = IMemeToken::new(token_addr, client.clone());
        let issuer_role = ethers::utils::keccak256("ISSUER_ROLE");

        let tx_grant = meme
            .grant_role(issuer_role, wallet_addr)
            .gas_price(bumped_gas_price);
        tx_grant.send().await?.await?;

        let tx_mint = meme
            .mint(wallet_addr, U256::from(1_000_000) * U256::exp10(6))
            .gas_price(bumped_gas_price);
        let receipt_mint = tx_mint.send().await?.await?.context("Mint failed")?;

        // Log to DB
        if let Some(db) = ctx.db.as_ref() {
            db.log_asset_creation(
                &format!("{:?}", wallet_addr),
                &format!("{:?}", token_addr),
                "meme",
                &name,
                &symbol,
            )
            .await?;
        }

        Ok(TaskResult {
            success: true,
            message: format!("Created Meme {} at {:?}.", symbol, token_addr),
            tx_hash: Some(format!("{:?}", receipt_mint.transaction_hash)),
        })
    }
}
