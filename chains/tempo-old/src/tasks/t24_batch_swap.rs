use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IDexSwap,
    r#"[
        function swapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn, uint128 minAmountOut) returns (uint128 amountOut)
        function quoteSwapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn) view returns (uint128 amountOut)
        function approve(address spender, uint256 amount) returns (bool)
        function allowance(address owner, address spender) view returns (uint256)
        function balanceOf(address owner) view returns (uint256)
    ]"#
);

pub struct BatchSwapTask;

#[async_trait]
impl TempoTask for BatchSwapTask {
    fn name(&self) -> &str {
        "24_batch_swap"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let dex_addr = Address::from_str("0x10c0000000000000000000000000000000000000")?;
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

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let dex = IDexSwap::new(dex_addr, client.clone());

        let count = rand::thread_rng().gen_range(2..4);

        println!("Executing Batch of {} Swaps...", count);

        let mut success_count = 0;
        let mut last_hash = String::new();

        for i in 0..count {
            let token_in = tokens[i % tokens.len()].1;
            let token_out = tokens[(i + 1) % tokens.len()].1;

            let amount_in = 500_000u128; // 0.5 USD

            let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
            let bumped_gas_price = GasManager::bump_fees(gas_price);

            // Approve if needed
            let token_contract = IDexSwap::new(token_in, client.clone());
            let allowance = token_contract
                .allowance(ctx.wallet.address(), dex_addr)
                .call()
                .await
                .unwrap_or_default();
            if allowance < U256::from(amount_in) {
                token_contract
                    .approve(dex_addr, U256::max_value())
                    .gas_price(bumped_gas_price)
                    .send()
                    .await?
                    .await?;
            }

            // Quote
            let quote = dex
                .quote_swap_exact_amount_in(token_in, token_out, amount_in)
                .call()
                .await
                .unwrap_or_default();
            if quote == 0 {
                continue;
            }

            let min_out = (quote * 9) / 10; // 10% slippage

            let tx = dex
                .swap_exact_amount_in(token_in, token_out, amount_in, min_out)
                .gas_price(bumped_gas_price);
            let pending = tx.send().await?;
            let receipt = pending.await?.context("Swap failed")?;

            success_count += 1;
            last_hash = format!("{:?}", receipt.transaction_hash);

            if i < count - 1 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }

        Ok(TaskResult {
            success: success_count > 0,
            message: format!("Completed {}/{} swaps in batch.", success_count, count),
            tx_hash: if last_hash.is_empty() {
                None
            } else {
                Some(last_hash)
            },
        })
    }
}
