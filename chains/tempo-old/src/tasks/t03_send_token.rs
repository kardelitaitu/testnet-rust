use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_random_address;
use crate::utils::gas_manager::GasManager;
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use rand::prelude::SliceRandom;
use std::str::FromStr;

ethers::contract::abigen!(
    ITIP20,
    r#"[
        function transfer(address to, uint256 amount) external returns (bool)
        function balanceOf(address account) external view returns (uint256)
        function decimals() external pure returns (uint8)
        function symbol() external view returns (string)
        function currency() external view returns (string)
    ]"#
);

const SYSTEM_TOKENS: &[(&str, &str)] = &[
    ("PathUSD", "0x20C0000000000000000000000000000000000000"),
    ("AlphaUSD", "0x20c0000000000000000000000000000000000001"),
    ("BetaUSD", "0x20c0000000000000000000000000000000000002"),
    ("ThetaUSD", "0x20c0000000000000000000000000000000000003"),
];

pub struct SendTokenTask;

#[async_trait]
impl TempoTask for SendTokenTask {
    fn name(&self) -> &str {
        "03_send_token"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        // 1. Randomly select a system token BEFORE any await
        let (token_name, token_addr_str) = {
            let mut rng = rand::thread_rng();
            SYSTEM_TOKENS
                .choose(&mut rng)
                .copied()
                .unwrap_or(("PathUSD", SYSTEM_TOKENS[0].1))
        };
        let token_address = Address::from_str(token_addr_str)?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        let address = ctx.wallet.address();

        let contract = ITIP20::new(token_address, client.clone());

        // 2. Check Balance
        let balance = contract.balance_of(address).call().await?;

        // TIP-20 uses 6 decimals, minimum balance check
        let min_balance = U256::from(1_000_000u64);

        if balance < min_balance {
            return Ok(TaskResult {
                success: false,
                message: format!("Low {} balance: {} (Need 10^6)", token_name, balance),
                tx_hash: None,
            });
        }

        // 3. Random recipient from address.txt
        let dest = get_random_address()?;

        // 4. Calculate 2% of balance
        let amount = balance / U256::from(50);

        // Ensure we don't send zero
        if amount.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!("Balance too low to send 2% (balance: {})", balance),
                tx_hash: None,
            });
        }

        // 5. Get token info for display
        let decimals = contract.decimals().call().await.unwrap_or(6);
        let symbol = match contract.symbol().call().await {
            Ok(s) => s,
            Err(_) => token_name.to_string(),
        };

        let amount_display = ethers::utils::format_units(amount, decimals as u32)
            .unwrap_or_else(|_| "unknown".to_string());

        println!(
            "Sending 2% of {} balance ({} {}) to {:?}...",
            symbol, amount_display, symbol, dest
        );

        // 6. Send - TIP-20 transfer
        let tx = contract.transfer(dest, amount).gas_price(bumped_gas_price);
        let pending_tx = tx.send().await?;
        let tx_hash = format!("{:?}", pending_tx.tx_hash());

        Ok(TaskResult {
            success: true,
            message: format!(
                "Sent 2% of {} ({} {}) to {:?}. Tx: {}",
                symbol, amount_display, symbol, dest, tx_hash
            ),
            tx_hash: Some(tx_hash),
        })
    }
}
