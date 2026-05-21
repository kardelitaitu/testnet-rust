use super::{SepoliaTask, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

/// USDC on Sepolia
const USDC: &str = "0x94a9d9ac8a22534e3faca9f4e7f2e2cf85d5e4c8";
/// USDT on Sepolia
const USDT: &str = "0xaa8e23fb1079ea71e0a56f48a2aa51851d8433d0";
/// USDC+ (C+) on Sepolia
const USDC_PLUS: &str = "0xe815718d44694ec4637cb775c468d87f6e15b538";
/// USDT+ (T+) on Sepolia
const USDT_PLUS: &str = "0xe20534a32f9162488a90026f268a74fbe28d272d";

/// Minimal ERC-20 ABI: balanceOf + decimals
const ERC20_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"}
]"#;

async fn get_token_balance(
    provider: &Provider<Http>,
    token_addr: &str,
    wallet: Address,
) -> Result<(String, String)> {
    let addr: Address = token_addr.parse()?;
    let contract = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(ERC20_ABI)?,
        Arc::new(provider.clone()),
    );

    let raw_balance: U256 = contract
        .method::<_, U256>("balanceOf", wallet)?
        .call()
        .await?;

    let decimals: u8 = contract.method::<_, u8>("decimals", ())?.call().await?; // Format as integer (no decimals — truncate/floored)
    let divisor = 10u128.pow(decimals as u32);
    let integer = raw_balance.as_u128() / divisor;
    let formatted = integer.to_string();

    Ok((formatted, raw_balance.to_string()))
}

/// Format ETH (18 decimals) with 5 decimal places, truncation (no rounding)
fn format_eth_5dec(raw: U256) -> String {
    const ETH_DECIMALS: u8 = 18;
    const DISPLAY_DECIMALS: u8 = 5;
    let divisor = 10u128.pow(ETH_DECIMALS as u32);
    let step = 10u128.pow((ETH_DECIMALS - DISPLAY_DECIMALS) as u32);
    let balance_u128: u128 = raw.as_u128();
    // Truncate (floor) — no rounding
    let integer = balance_u128 / divisor;
    let display_fraction = (balance_u128 % divisor) / step;
    if display_fraction == 0 {
        integer.to_string()
    } else {
        let frac_str = format!(
            "{:0width$}",
            display_fraction,
            width = DISPLAY_DECIMALS as usize
        );
        let trimmed = frac_str.trim_end_matches('0');
        format!("{}.{}", integer, trimmed)
    }
}

pub struct SepoliaCheckBalanceTask;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_eth_5dec_exact_eth() {
        // 1.0 ETH
        let result = format_eth_5dec(U256::from(10u128.pow(18)));
        assert_eq!(result, "1");
    }

    #[test]
    fn test_format_eth_5dec_with_decimals() {
        // 0.046847631228226791 ETH -> truncated to 5dp = 0.04684
        let raw = U256::from(46847631228226791u128);
        let result = format_eth_5dec(raw);
        assert_eq!(result, "0.04684");
    }

    #[test]
    fn test_format_eth_5dec_small_value() {
        // 0.0000012345 ETH -> truncates to 0 (below 0.00001 ETH resolution with 5dp)
        let raw = U256::from(1234500000000u128);
        let result = format_eth_5dec(raw);
        assert_eq!(result, "0");
    }

    #[test]
    fn test_format_eth_5dec_zero() {
        let result = format_eth_5dec(U256::zero());
        assert_eq!(result, "0");
    }

    #[test]
    fn test_format_eth_5dec_ten_eth() {
        let result = format_eth_5dec(U256::from(10u128.pow(19)));
        assert_eq!(result, "10");
    }

    #[test]
    fn test_format_eth_5dec_strips_trailing_zeros() {
        // 0.000100000 ETH -> 0.0001
        let raw = U256::from(100000000000000u128);
        let result = format_eth_5dec(raw);
        assert_eq!(result, "0.0001");
    }
}

#[async_trait]
impl SepoliaTask for SepoliaCheckBalanceTask {
    fn name(&self) -> &str {
        "01_checkBalance"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let address = ctx.wallet.address();

        // --- Native ETH balance ---
        let balance = ctx.provider.get_balance(address, None).await?;
        let eth_display = format_eth_5dec(balance);

        // --- Token balances ---
        let tokens = [
            (USDC, "USDC"),
            (USDT, "USDT"),
            (USDC_PLUS, "USDC+"),
            (USDT_PLUS, "USDT+"),
        ];
        let mut token_lines = Vec::new();

        for (token_addr, token_name) in &tokens {
            match get_token_balance(&ctx.provider, token_addr, address).await {
                Ok((formatted, _raw)) => {
                    token_lines.push(format!("{}: {}", token_name, formatted));
                }
                Err(_e) => {
                    token_lines.push(format!("{}: error", token_name));
                }
            }
        }

        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;
        let max_fee_gwei: f64 = max_fee.as_u128() as f64 / 1e9;

        let token_str = token_lines.join(" | ");
        Ok(TaskResult {
            success: true,
            message: format!(
                "ETH: {} | {} | Gas: {:.2}",
                eth_display, token_str, max_fee_gwei,
            ),
        })
    }
}
