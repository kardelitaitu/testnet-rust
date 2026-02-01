use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

// DEX Binding
ethers::contract::abigen!(
    IStablecoinDEX,
    r#"[
        function place(address token, uint128 amount, bool isBid, int16 tick) external returns (uint128 orderId)
        function createPair(address base) external returns (bytes32 key)
        function pairKey(address tokenA, address tokenB) external pure returns (bytes32 key)
        function books(bytes32 pairKey) external view returns (address base, address quote, int16 bestBidTick, int16 bestAskTick)
    ]"#
);

// ERC20 Binding
ethers::contract::abigen!(
    IERC20Full,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function approve(address spender, uint256 amount) returns (bool)
        function allowance(address owner, address spender) view returns (uint256)
        function balanceOf(address owner) view returns (uint256)
        function decimals() view returns (uint8)
    ]"#
);

pub struct AddLiquidityTask;

#[async_trait]
impl TempoTask for AddLiquidityTask {
    fn name(&self) -> &str {
        "06_add_liquidity"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let dex_address = Address::from_str("0xdec0000000000000000000000000000000000000")?;
        let quote_token_addr = Address::from_str("0x20C0000000000000000000000000000000000000")?; // PathUSD

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
                message: "No created stablecoins found in DB used for liquidity.".to_string(),
                tx_hash: None,
            });
        }

        // Pick Random Asset
        let base_token_addr = {
            let mut rng = rand::thread_rng();
            let idx = rng.gen_range(0..assets.len());
            Address::from_str(&assets[idx])?
        };

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        let dex = IStablecoinDEX::new(dex_address, client.clone());
        let base_token = IERC20Full::new(base_token_addr, client.clone());

        // 2. Check Pair Existence
        // Note: pairKey order matters? implementation usually sorts, but helper abstracts it?
        // Node js: pairKey(tokenA, tokenB)
        let pair_key = dex
            .pair_key(base_token_addr, quote_token_addr)
            .call()
            .await?;
        let book = dex.books(pair_key).call().await?;

        // book is (base, quote, bestBid, bestAsk)
        // If base is zero, pair doesn't exist
        // book returns tuple: (Address, Address, i16, i16)

        let (book_base, _, _, _) = book;

        if book_base == Address::zero() {
            println!(
                "Pair not found. Creating pair for base: {:?}",
                base_token_addr
            );
            // Create Pair
            // Assuming base_token is "base"
            let tx_create = dex.create_pair(base_token_addr).gas_price(bumped_gas_price);
            let pending = tx_create.send().await?;
            let _receipt = pending.await?;
            println!("Pair created.");
        }

        // 3. Place Liquidity (Limit Order)
        // We will "Sell" our base token (Ask)
        // Parameters: (token, amount, isBid, tick)
        // token: base_token
        // amount: ?
        // isBid: false (Selling Base for Quote)
        // tick: e.g. 100 (Price level)

        // Check Balance
        let _decimals = base_token.decimals().call().await.unwrap_or(18);
        let balance = base_token.balance_of(ctx.wallet.address()).call().await?;

        if balance.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!("Zero balance for asset {:?}", base_token_addr),
                tx_hash: None,
            });
        }

        let amount_place = balance / 2; // Place half

        // Approve
        let allowance = base_token
            .allowance(ctx.wallet.address(), dex_address)
            .call()
            .await?;
        if allowance < amount_place {
            let tx_approve = base_token
                .approve(dex_address, U256::MAX)
                .gas_price(bumped_gas_price);
            let _ = tx_approve.send().await?.await?;
        }

        let tick: i16 = 100; // Arbitrary tick for now
        let is_bid = false; // Sell Base

        println!(
            "Placing Order: Sell {:?} Base @ Tick {}",
            amount_place, tick
        );

        let tx_place = dex
            .place(base_token_addr, amount_place.as_u128(), is_bid, tick)
            .gas_price(bumped_gas_price);
        let pending_place = tx_place.send().await?;
        let hash = format!("{:?}", pending_place.tx_hash());

        // Final receipt wait (optional, but let's keep it minimal)
        let _receipt = pending_place.await?;

        Ok(TaskResult {
            success: true,
            message: format!(
                "Placed Liquidity Order for {:?}. Tx: {}",
                base_token_addr, hash
            ),
            tx_hash: Some(hash),
        })
    }
}
