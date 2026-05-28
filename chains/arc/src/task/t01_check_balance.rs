use super::{ArcTask, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

// Standard ERC-20 balanceOf interface
abigen!(
    IERC20,
    r#"[
        function balanceOf(address owner) external view returns (uint256)
    ]"#,
);

// Arc Testnet token configs with known decimals
struct TokenInfo {
    name: &'static str,
    address: Address,
    decimals: usize,
}

fn token_configs() -> Vec<TokenInfo> {
    vec![
        TokenInfo {
            name: "USDC",
            address: "0x3600000000000000000000000000000000000000".parse().unwrap(),
            decimals: 6,
        },
        TokenInfo {
            name: "EURC",
            address: "0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a".parse().unwrap(),
            decimals: 6,
        },
        TokenInfo {
            name: "cirBTC",
            address: "0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF".parse().unwrap(),
            decimals: 8,
        },
    ]
}

fn format_balance(amount: U256, decimals: usize, label: &str) -> String {
    match ethers::utils::format_units(amount, decimals) {
        Ok(formatted) => format!("{}: {}", label, formatted),
        Err(e) => format!("{}: PARSE_ERROR - {}", label, e),
    }
}

pub struct ArcCheckBalanceTask;

#[async_trait]
impl ArcTask for ArcCheckBalanceTask {
    fn name(&self) -> &str {
        "01_checkBalance"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let address = ctx.wallet.address();
        let addr_hex = format!("{:?}", address);

        // Native gas token balance (18 decimals)
        let nonce = ctx.provider.get_transaction_count(address, None).await?;
        let native_balance = ctx.provider.get_balance(address, None).await?;

        let mut lines = vec![format!(
            "Native ({}): {} | Nonce: {}",
            ctx.config.symbol,
            ethers::utils::format_ether(native_balance),
            nonce
        )];

        // ERC-20 token balances
        for token in token_configs() {
            let contract = IERC20::new(token.address, Arc::new(ctx.provider.clone()));
            match contract.balance_of(address).call().await {
                Ok(balance) => lines.push(format_balance(balance, token.decimals, token.name)),
                Err(e) => lines.push(format!("{}: BALANCE_OF_ERROR - {}", token.name, e)),
            }
        }

        let message = lines.join(" | ");
        let block_num = match ctx.provider.get_block_number().await {
            Ok(n) => n.to_string(),
            Err(_) => "???".to_string(),
        };

        // Log to DB if available
        if let Some(db) = &ctx.db {
            let _ = db
                .log_task_result(
                    &addr_hex,
                    &addr_hex,
                    self.name(),
                    true,
                    &format!("{} (B: {})", message, block_num),
                    0,
                )
                .await;
        }

        Ok(TaskResult { success: true, message })
    }
}
