use super::{ArcTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use ethers::signers::Wallet;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::sync::Arc;

// Standard ERC-20 interface for cirBTC
abigen!(
    IERC20,
    r#"[
        function balanceOf(address owner) external view returns (uint256)
        function transfer(address to, uint256 amount) external returns (bool)
    ]"#,
);

const CIRBTC_ADDRESS: &str = "0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF";
const CIRBTC_DECIMALS: usize = 8;
/// Cap at 70k gas for cirBTC transfers (usually ~50-70k)
const GAS_LIMIT: u64 = 70_000;

pub struct SendCirbtcTask;

#[async_trait]
impl ArcTask for SendCirbtcTask {
    fn name(&self) -> &str {
        "04_sendCirbtc"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let sender = ctx.wallet.address();
        let addr_hex = format!("{:?}", sender);
        let cirbtc: Address = CIRBTC_ADDRESS.parse().context("Invalid cirBTC address")?;

        // Get cirBTC balance
        let provider = Arc::new(ctx.provider.clone());
        let token = IERC20::new(cirbtc, provider.clone());
        let balance = token
            .balance_of(sender)
            .call()
            .await
            .context("Failed to query cirBTC balance")?;

        if balance.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: "cirBTC balance is zero, nothing to send".to_string(),
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
                    ethers::utils::format_units(balance, CIRBTC_DECIMALS).unwrap_or_default(),
                    numerator
                ),
            });
        }

        // Generate a random recipient wallet
        let recipient_wallet = Wallet::new(&mut rng);
        let recipient = recipient_wallet.address();

        // Build signer middleware and send transfer
        let signer = Arc::new(SignerMiddleware::new(ctx.provider.clone(), ctx.wallet));
        let contract = IERC20::new(cirbtc, signer);

        let receipt = contract
            .transfer(recipient, send_amount)
            .gas(GAS_LIMIT)
            .send()
            .await
            .context("Failed to send cirBTC transfer")?
            .await
            .context("Failed to get cirBTC transfer receipt")?
            .context("cirBTC transfer reverted")?;

        let tx_hash = receipt.transaction_hash;

        let send_formatted =
            ethers::utils::format_units(send_amount, CIRBTC_DECIMALS).unwrap_or_default();

        let message = format!(
            "Sent {} cirBTC to {:?} (tx: {:?}, block: {})",
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
        let task = SendCirbtcTask;
        assert_eq!(task.name(), "04_sendCirbtc");
    }

    #[test]
    fn test_cirbtc_address_parses() {
        let addr: Result<Address, _> = CIRBTC_ADDRESS.parse();
        assert!(addr.is_ok());
    }

    #[test]
    fn test_send_amount_in_range() {
        let mut rng = StdRng::from_entropy();
        // Test percentage logic: balance * numerator / 100_000
        let balance = U256::from(10_000_000_000u64); // 100 cirBTC (8 dec)
        for _ in 0..100 {
            let numerator = rng.gen_range(5u128..=10u128);
            let amount = balance * U256::from(numerator) / U256::from(100_000u128);
            assert!(amount > U256::zero(), "amount should not be zero");
        }
    }

    #[test]
    fn test_gas_limit_reasonable() {
        // cirBTC transfer gas should be well under 70k
        assert!(GAS_LIMIT >= 50_000);
        assert!(GAS_LIMIT <= 100_000);
    }
}
