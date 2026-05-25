use super::{ArcTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use ethers::signers::Wallet;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::sync::Arc;

// Standard ERC-20 interface for USDC
abigen!(
    IERC20,
    r#"[
        function balanceOf(address owner) external view returns (uint256)
        function transfer(address to, uint256 amount) external returns (bool)
    ]"#,
);

const USDC_ADDRESS: &str = "0x3600000000000000000000000000000000000000";
const USDC_DECIMALS: usize = 6;
/// Cap at 64k gas for USDC transfers (usually ~50-70k)
const GAS_LIMIT: u64 = 64_000;

pub struct SendUsdcTask;

#[async_trait]
impl ArcTask for SendUsdcTask {
    fn name(&self) -> &str {
        "02_sendUsdc"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let sender = ctx.wallet.address();
        let addr_hex = format!("{:?}", sender);
        let usdc: Address = USDC_ADDRESS.parse().context("Invalid USDC address")?;

        // Get USDC balance
        let provider = Arc::new(ctx.provider.clone());
        let token = IERC20::new(usdc, provider.clone());
        let balance = token
            .balance_of(sender)
            .call()
            .await
            .context("Failed to query USDC balance")?;

        if balance.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: "USDC balance is zero, nothing to send".to_string(),
            });
        }

        // Random percentage: 0.005% to 0.01%
        let mut rng = StdRng::from_entropy();
        let numerator = rng.gen_range(5u128..=10u128); // 5..10 per 100_000
                                                       // send_amount = balance * numerator / 100_000
        let send_amount = balance
            .checked_mul(U256::from(numerator))
            .and_then(|v| v.checked_div(U256::from(100_000u128)))
            .unwrap_or(U256::zero());

        if send_amount.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Calculated send amount is zero (balance={}, numerator={})",
                    ethers::utils::format_units(balance, USDC_DECIMALS).unwrap_or_default(),
                    numerator
                ),
            });
        }

        // Generate a random recipient wallet
        let recipient_wallet = Wallet::new(&mut rng);
        let recipient = recipient_wallet.address();

        // Build signer middleware and send transfer
        let signer = Arc::new(SignerMiddleware::new(ctx.provider.clone(), ctx.wallet));
        let contract = IERC20::new(usdc, signer);

        let receipt = contract
            .transfer(recipient, send_amount)
            .gas(GAS_LIMIT)
            .send()
            .await
            .context("Failed to send USDC transfer")?
            .await
            .context("Failed to get USDC transfer receipt")?
            .context("USDC transfer reverted")?;

        let tx_hash = receipt.transaction_hash;

        let send_formatted =
            ethers::utils::format_units(send_amount, USDC_DECIMALS).unwrap_or_default();

        let message = format!(
            "Sent {} USDC to {:?} (tx: {:?}, block: {})",
            send_formatted,
            recipient,
            tx_hash,
            receipt.block_number.unwrap_or_default()
        );

        // Log to DB if available
        if let Some(db) = &ctx.db {
            let _ = db
                .log_task_result(
                    &addr_hex,
                    &addr_hex,
                    self.name(),
                    true,
                    &format!(
                        "{} (B: {})",
                        message,
                        receipt.block_number.unwrap_or_default()
                    ),
                    0,
                )
                .await;
        }

        Ok(TaskResult {
            success: true,
            message,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_is_correct() {
        let task = SendUsdcTask;
        assert_eq!(task.name(), "02_sendUsdc");
    }

    #[test]
    fn test_usdc_address_parses() {
        let addr: Result<Address, _> = USDC_ADDRESS.parse();
        assert!(addr.is_ok());
    }

    #[test]
    fn test_send_amount_in_range() {
        let mut rng = StdRng::from_entropy();
        // Test percentage logic: balance * numerator / 100_000
        let balance = U256::from(1_000_000u64); // 1 USDC
        for _ in 0..100 {
            let numerator = rng.gen_range(5u128..=10u128);
            let amount = balance * U256::from(numerator) / U256::from(100_000u128);
            // 0.005% of 1 USDC = 0.05 USDC = 50000 (6 dec)
            // 0.01% of 1 USDC = 0.1 USDC = 100000 (6 dec)
            assert!(amount >= U256::from(50u64), "amount too low: {}", amount);
            assert!(amount <= U256::from(100u64), "amount too high: {}", amount);
        }
    }

    #[test]
    fn test_gas_limit_reasonable() {
        // USDC transfer gas should be well under 64k
        assert!(GAS_LIMIT >= 50_000);
        assert!(GAS_LIMIT <= 100_000);
    }
}
