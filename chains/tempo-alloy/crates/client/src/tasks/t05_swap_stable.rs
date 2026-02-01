//! Swap Stablecoin Task
//!
//! Performs a swap on the Tempo DEX.

use crate::tasks::{prelude::*, GasManager};
use alloy::primitives::{Address, U256};
use alloy::sol;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use rand::Rng;
use std::str::FromStr;

/// DEX Swap Interface
sol!(
    IDexSwap,
    r#"[
        function swapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn, uint128 minAmountOut) returns (uint128 amountOut)
        function quoteSwapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn) view returns (uint128 amountOut)
        function approve(address spender, uint256 amount) returns (bool)
        function allowance(address owner, address spender) view returns (uint256)
        function balanceOf(address owner) view returns (uint256)
    ]"#
);

/// Swap stablecoins on the DEX
#[derive(Debug, Clone, Default)]
pub struct SwapStableTask;

impl SwapStableTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for SwapStableTask {
    fn name(&self) -> &'static str {
        "05_swap_stable"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let dex_address = Address::from_str("0x10c0000000000000000000000000000000000000")?;
        let tokens = vec![
            (
                "PathUSD",
                Address::from_str("0x20c0000000000000000000000000000000000000")?,
            ),
            (
                "AlphaUSD",
                Address::from_str("0x20c0000000000000000000000000000000000001")?,
            ),
        ];

        let client = &ctx.client;
        let gas_manager = GasManager::default();

        let gas_price = gas_manager.estimate_gas(client).await?;
        let bumped_gas_price = gas_manager.bump_fees(gas_price, 20);

        let dex = IDexSwap::new(dex_address, client.provider.clone());

        // 1. Select random tokens to swap between
        let (token_in_name, token_in) = tokens[0];
        let (_token_out_name, token_out) = tokens[1];

        let amount_in = 500_000u128; // 0.5 USD equivalent

        // 2. Check balance and approve if needed
        let token_contract = IDexSwap::new(token_in, client.provider.clone());
        let balance = token_contract.balanceOf(ctx.address()).call().await?;

        if balance < U256::from(amount_in) {
            return Ok(TaskResult {
                success: false,
                message: format!("Insufficient balance: {}", balance),
                tx_hash: None,
            });
        }

        let allowance = token_contract
            .allowance(ctx.address(), dex_address)
            .call()
            .await?;

        if allowance < U256::from(amount_in) {
            println!("Approving tokens...");
            token_contract
                .approve(dex_address, U256::MAX)
                .max_fee_per_gas(bumped_gas_price)
                .send()
                .await?
                .get_receipt()
                .await?;
        }

        // 3. Get quote
        let quote = dex
            .quoteSwapExactAmountIn(token_in, token_out, amount_in)
            .call()
            .await?;

        if quote == 0 {
            return Ok(TaskResult {
                success: false,
                message: "No liquidity for swap".to_string(),
                tx_hash: None,
            });
        }

        let min_out = (u128::from(quote) * 9) / 10; // 10% slippage

        // 4. Execute swap
        println!(
            "Swapping {} {} for at least {}...",
            amount_in, token_in_name, min_out
        );

        let tx = dex.swapExactAmountIn(token_in, token_out, amount_in, min_out);
        let receipt = tx
            .max_fee_per_gas(bumped_gas_price)
            .send()
            .await?
            .get_receipt()
            .await?;

        Ok(TaskResult {
            success: true,
            message: format!("Swap completed: {:?}", receipt.inner.transaction_hash),
            tx_hash: Some(format!("{:?}", receipt.inner.transaction_hash)),
        })
    }
}
