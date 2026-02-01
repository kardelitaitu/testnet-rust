use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IStablecoinDEX,
    r#"[
        function place(address token, uint128 amount, bool isBid, int16 tick) external returns (uint128 orderId)
        event OrderPlaced(uint128 indexed orderId, address indexed user, address indexed token, uint128 amount, bool isBid, int16 tick)
    ]"#
);

ethers::contract::abigen!(
    IERC20Full,
    r#"[
        function balanceOf(address owner) view returns (uint256)
        function allowance(address owner, address spender) view returns (uint256)
        function approve(address spender, uint256 amount) returns (bool)
        function decimals() view returns (uint8)
    ]"#
);

pub struct LimitOrderTask;

#[async_trait]
impl TempoTask for LimitOrderTask {
    fn name(&self) -> &str {
        "11_limit_order"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let dex_address = Address::from_str("0x10c0000000000000000000000000000000000000")?;
        let path_usd_address = Address::from_str("0x20c0000000000000000000000000000000000000")?;
        let token_address = Address::from_str("0x20c0000000000000000000000000000000000001")?; // AlphaUSD

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let dex = IStablecoinDEX::new(dex_address, client.clone());

        let wallet_addr = ctx.wallet.address();

        // 1. Force BID (Buy AlphaUSD with PathUSD)
        let token_to_approve = path_usd_address;
        let token_contract = IERC20Full::new(token_to_approve, client.clone());

        let decimals = token_contract.decimals().call().await.unwrap_or(6);
        let amount_base = rand::thread_rng().gen_range(500..1000);
        let amount_wei = U256::from(amount_base) * U256::exp10(decimals as usize);

        // Check Balance
        let balance = token_contract
            .balance_of(wallet_addr)
            .call()
            .await
            .unwrap_or_default();
        if balance < amount_wei {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient PathUSD balance. Need {}, have {}",
                    amount_base,
                    balance / U256::exp10(decimals as usize)
                ),
                tx_hash: None,
            });
        }

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // 2. Approve
        let allowance = token_contract
            .allowance(wallet_addr, dex_address)
            .call()
            .await
            .unwrap_or_default();
        if allowance < amount_wei {
            println!("Approving PathUSD for DEX...");
            let tx_approve = token_contract
                .approve(dex_address, U256::max_value())
                .gas_price(bumped_gas_price);
            let pending_approve = tx_approve.send().await?;
            pending_approve.await?;
        }

        // 3. Place Order (BID)
        println!(
            "Placing Limit Order: Buying ALPHUSD with {} PathUSD @ Tick 0",
            amount_base
        );
        let amount_u128 = amount_wei.as_u128();
        let tick: i16 = 0;
        let is_bid = true;

        let tx_place = dex
            .place(token_address, amount_u128, is_bid, tick)
            .gas_price(bumped_gas_price);
        let pending_place = tx_place.send().await?;
        let receipt = pending_place
            .await?
            .context("Limit Order placement failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!("Placed Limit Order (BID) for {}. Tx: {}", amount_base, hash),
            tx_hash: Some(hash),
        })
    }
}
