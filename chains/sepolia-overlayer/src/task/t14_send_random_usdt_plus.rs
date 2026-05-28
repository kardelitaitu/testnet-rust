use super::{confirm_with_retry, SepoliaTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;

/// USDT+ (T+) on Sepolia
const USDT_PLUS: &str = "0xe20534a32f9162488a90026f268a74fbe28d272d";

/// Minimal ERC-20 ABI: balanceOf + transfer
const TPLUS_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":false,"inputs":[{"name":"to","type":"address"},{"name":"amount","type":"uint256"}],"name":"transfer","outputs":[{"name":"","type":"bool"}],"type":"function"}
]"#;

pub struct SendRandomUsdtPlusTask;

#[async_trait]
impl SepoliaTask for SendRandomUsdtPlusTask {
    fn name(&self) -> &str {
        "14_sendRandomUsdtPlus"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let tplus_addr: Address = USDT_PLUS.parse()?;

        // --- 1. Get USDT+ balance ---
        let contract = Contract::new(
            tplus_addr,
            serde_json::from_str::<ethers::abi::Abi>(TPLUS_ABI)?,
            Arc::new(provider.clone()),
        );

        let balance: U256 = contract
            .method::<_, U256>("balanceOf", address)?
            .call()
            .await
            .context("Failed to query USDT+ balance")?;

        // --- 2. Calculate 0.2% = balance * 2 / 1000 ---
        let amount = balance * U256::from(2) / U256::from(1000);

        if amount.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!("USDT+ balance is zero or 0.2% rounds to 0 (balance: {})", balance),
            });
        }

        // --- 3. Generate random recipient address ---
        let random_addr_hex = core_logic::generate_random_address();
        let random_addr: Address = random_addr_hex
            .parse()
            .context("Failed to parse generated random address")?;

        // --- 4. Send transfer ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        let tplus_contract = Contract::new(
            tplus_addr,
            serde_json::from_str::<ethers::abi::Abi>(TPLUS_ABI)?,
            Arc::new(middleware),
        );

        let transfer_call = tplus_contract
            .method::<_, H256>("transfer", (random_addr, amount))?
            .gas(100_000)
            .gas_price(max_fee);

        let tx = transfer_call.send().await.context("Failed to send USDT+ transfer")?;

        let tx_hash = tx.tx_hash();

        let receipt = confirm_with_retry(tx_hash, provider).await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));

        let readable_amount = format_amount(amount);

        Ok(TaskResult {
            success,
            message: if success {
                format!(
                    "Sent {} USDT+ to {} (tx: {:?})",
                    readable_amount, random_addr_hex, tx_hash
                )
            } else {
                format!("Failed to send USDT+ - receipt not confirmed (tx: {:?})", tx_hash)
            },
        })
    }
}

/// Format a U256 amount (18 decimals) to a human-readable string
fn format_amount(amount: U256) -> String {
    let divisor = U256::from(10u128.pow(18));
    let whole = amount / divisor;
    let fraction = amount % divisor;
    if fraction.is_zero() {
        format!("{}", whole)
    } else {
        let frac_str = format!("{:018}", fraction);
        let trimmed = frac_str.trim_end_matches('0');
        format!("{}.{}", whole, trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_amount_whole() {
        let amount = U256::from(5u128) * U256::from(10u128.pow(18));
        assert_eq!(format_amount(amount), "5");
    }

    #[test]
    fn test_format_amount_fractional() {
        let amount = U256::from(5_500_000_000_000_000_000u128); // 5.5 T+
        assert_eq!(format_amount(amount), "5.5");
    }

    #[test]
    fn test_format_amount_small() {
        let amount = U256::from(1_000_000_000_000_000_000u128); // 1.0 T+
        assert_eq!(format_amount(amount), "1");
    }

    #[test]
    fn test_format_amount_zero() {
        assert_eq!(format_amount(U256::zero()), "0");
    }

    #[test]
    fn test_format_amount_tiny() {
        let amount = U256::from(1u128); // 1 wei of T+
        assert_eq!(format_amount(amount), "0.000000000000000001");
    }
}
