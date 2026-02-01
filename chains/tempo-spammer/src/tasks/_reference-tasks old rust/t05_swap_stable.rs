use crate::tasks::{TaskContext, TaskResult, TempoTask};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
// use ethers::abi::AbiEncode;
use crate::utils::gas_manager::GasManager;
use rand::Rng;
use std::str::FromStr;

// DEX Binding
ethers::contract::abigen!(
    IStablecoinDEX,
    r#"[
        function quoteSwapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn) view returns (uint128 amountOut)
        function swapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn, uint128 minAmountOut) returns (uint128 amountOut)
    ]"#
);

// Token Binding for Approve/Balance
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

pub struct SwapStableTask;

#[async_trait]
impl TempoTask for SwapStableTask {
    fn name(&self) -> &str {
        "05_swap_stable"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let dex_address = Address::from_str("0xdec0000000000000000000000000000000000000")?;

        // Hardcoded System Tokens
        let tokens = vec![
            (
                Address::from_str("0x20c0000000000000000000000000000000000000")?,
                "PathUSD",
            ),
            (
                Address::from_str("0x20c0000000000000000000000000000000000001")?,
                "AlphaUSD",
            ),
            (
                Address::from_str("0x20c0000000000000000000000000000000000002")?,
                "BetaUSD",
            ),
            (
                Address::from_str("0x20c0000000000000000000000000000000000003")?,
                "ThetaUSD",
            ),
        ];

        // Pick Random Token In and Out
        let (token_in_addr, symbol_in, token_out_addr, symbol_out) = {
            let mut rng = rand::thread_rng();
            let idx_in = rng.gen_range(0..tokens.len());
            let mut idx_out = rng.gen_range(0..tokens.len());
            while idx_out == idx_in {
                idx_out = rng.gen_range(0..tokens.len());
            }
            let (addr_in, sym_in) = &tokens[idx_in];
            let (addr_out, sym_out) = &tokens[idx_out];
            (*addr_in, *sym_in, *addr_out, *sym_out)
        };

        println!("Swapping {} -> {}", symbol_in, symbol_out);

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        let token_in = IERC20Full::new(token_in_addr, client.clone());
        let dex = IStablecoinDEX::new(dex_address, client.clone());

        // Fetch decimals (default to 18 if fail)
        let decimals = token_in.decimals().call().await.unwrap_or(18);

        // Check Balance
        let balance = token_in.balance_of(ctx.wallet.address()).call().await?;

        if balance.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!("Zero balance for {}", symbol_in),
                tx_hash: None,
            });
        }

        let amount_in = balance * 3 / 100;
        let amount_formatted = format_amount(amount_in, decimals);
        println!("Swapping 3% of balance: {} {}", amount_formatted, symbol_in);

        // Check Allowance
        let allowance = token_in
            .allowance(ctx.wallet.address(), dex_address)
            .call()
            .await?;
        if allowance < amount_in {
            println!("Approving {}...", symbol_in);
            let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
            let bumped_gas_price = GasManager::bump_fees(gas_price);
            if let Ok(pending) = token_in
                .approve(dex_address, U256::MAX)
                .gas_price(bumped_gas_price)
                .send()
                .await
            {
                let _ = pending.await;
            }
        }

        // Quote
        let amount_in_u128 = amount_in.as_u128();

        // Handle quote revert (no liquidity)
        let quote_res = dex
            .quote_swap_exact_amount_in(token_in_addr, token_out_addr, amount_in_u128)
            .call()
            .await;

        match quote_res {
            Ok(amount_out) => {
                if amount_out == 0 {
                    return Ok(TaskResult {
                        success: true,
                        message: "No liquidity (quote 0)".to_string(),
                        tx_hash: None,
                    });
                }

                // Swap
                let min_out = (amount_out * 80) / 100; // 20% slippage
                let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
                let bumped_gas_price = GasManager::bump_fees(gas_price);

                let tx = dex
                    .swap_exact_amount_in(token_in_addr, token_out_addr, amount_in_u128, min_out)
                    .gas(2_000_000)
                    .gas_price(bumped_gas_price);
                let pending = tx.send().await?;
                let hash = format!("{:?}", pending.tx_hash());

                Ok(TaskResult {
                    success: true,
                    message: format!(
                        "Swapped {} {} -> {}. Tx: {}",
                        amount_formatted, symbol_in, symbol_out, hash
                    ),
                    tx_hash: Some(hash),
                })
            }
            Err(e) => {
                // Try to decode error or print raw
                Ok(TaskResult {
                    success: true, // Not a task failure, just market condition
                    message: format!(
                        "No liquidity (quote reverted) for {} -> {}. Error: {:?}",
                        symbol_in, symbol_out, e
                    ),
                    tx_hash: None,
                })
            }
        }
    }
}

fn format_amount(amount: U256, decimals: u8) -> String {
    let amount_f64: f64 = ethers::utils::format_units(amount, decimals as u32)
        .unwrap_or_else(|_| "0".to_string())
        .parse()
        .unwrap_or(0.0);

    if amount_f64 >= 1_000_000.0 {
        format!("{:.2}M", amount_f64 / 1_000_000.0)
    } else if amount_f64 >= 1_000.0 {
        format!("{:.2}k", amount_f64 / 1_000.0)
    } else {
        format!("{:.4}", amount_f64)
    }
}
