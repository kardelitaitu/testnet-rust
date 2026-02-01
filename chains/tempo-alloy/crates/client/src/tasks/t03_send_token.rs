//! Send Token Task
//!
//! Demonstrates using the sol! macro for type-safe contract interactions.

use crate::tasks::{prelude::*, GasManager};
use alloy::primitives::{address, Address, U256};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rand::prelude::SliceRandom;
use std::str::FromStr;
use crate::utils::get_random_address;

/// TIP-20 Token Interface using sol! macro
sol!(
    ITIP20,
    r#"[
        function transfer(address to, uint256 amount) external returns (bool)
        function balanceOf(address account) external view returns (uint256)
        function decimals() external pure returns (uint8)
        function symbol() external view returns (string)
    ]"#
);

/// System token addresses on Tempo
const SYSTEM_TOKENS: &[(&str, &str)] = &[
    ("PathUSD", "0x20C0000000000000000000000000000000000000"),
    ("AlphaUSD", "0x20c0000000000000000000000000000000000001"),
    ("BetaUSD", "0x20c0000000000000000000000000000000000002"),
    ("ThetaUSD", "0x20c0000000000000000000000000000000000003"),
];

/// Send system tokens to a random address
#[derive(Debug, Clone, Default)]
pub struct SendTokenTask;

impl SendTokenTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for SendTokenTask {
    fn name(&self) -> &'static str {
        "03_send_token"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        // 1. Randomly select a system token
        let (token_name, token_addr_str) = {
            let mut rng = rand::thread_rng();
            SYSTEM_TOKENS
                .choose(&mut rng)
                .copied()
                .unwrap_or(("PathUSD", SYSTEM_TOKENS[0].1))
        };
        let token_address = Address::from_str(token_addr_str)?;

        let client = &ctx.client;

        // 2. Get gas price
        let gas_price = GasManager::default().estimate_gas(client).await?;
        let bumped_gas_price = GasManager::default().bump_fees(gas_price, 20);

        let address = ctx.address();

        // 3. Create contract instance
        let contract = ITIP20::new(token_address, client.provider.clone());

        // 4. Check balance
        let balance = contract.balanceOf(address).call().await?;

        // TIP-20 uses 6 decimals, minimum balance check
        let min_balance = U256::from(1_000_000u64);

        if balance < min_balance {
            return Ok(TaskResult {
                success: false,
                message: format!("Low {} balance: {} (Need 10^6)", token_name, balance),
                tx_hash: None,
            });
        }

        // 5. Get random recipient
        let dest = get_random_address()?;

        // 6. Calculate 2% of balance
        let amount = balance / U256::from(50);

        if amount.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!("Balance too low to send 2% (balance: {})", balance),
                tx_hash: None,
            });
        }

        // 7. Get token info for display
        let decimals = contract.decimals().call().await.unwrap_or(6);
        let symbol = match contract.symbol().call().await {
            Ok(s) => s,
            Err(_) => token_name.to_string(),
        };

        println!(
            "Sending 2% of {} balance to {:?}...",
            symbol, dest
        );

        // 8. Send transfer
        let tx_hash = contract
            .transfer(dest, amount)
            .max_fee_per_gas(bumped_gas_price)
            .send()
            .await?
            .watch()
            .await?;

        Ok(TaskResult {
            success: true,
            message: format!("Sent 2% of {} to {:?}", symbol, dest),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
