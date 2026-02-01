//! Wallet Activity Task
//!
//! Reports wallet activity statistics including transaction count and PathUSD balance.
//!
//! Workflow:
//! 1. Get transaction count (nonce)
//! 2. Get PathUSD balance
//! 3. Report results

use crate::TempoClient;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::U256;
use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct WalletActivityTask;

impl WalletActivityTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for WalletActivityTask {
    fn name(&self) -> &'static str {
        "20_wallet_activity"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        let tx_count = client
            .provider
            .get_transaction_count(address)
            .await
            .unwrap_or_default();

        // Get PathUSD balance
        let pathusd_addr = crate::tasks::tempo_tokens::TempoTokens::get_path_usd_address();
        let decimals =
            crate::tasks::tempo_tokens::TempoTokens::get_token_decimals(client, pathusd_addr)
                .await
                .unwrap_or(6);

        let balance_raw = crate::tasks::tempo_tokens::TempoTokens::get_token_balance(
            client,
            pathusd_addr,
            address,
        )
        .await
        .unwrap_or(U256::ZERO);

        // Format balance with compact notation and orange color
        let balance_formatted =
            crate::tasks::tempo_tokens::TempoTokens::format_compact_colored(balance_raw, decimals);

        Ok(TaskResult {
            success: true,
            message: format!(
                "Wallet Activity: {} on-chain tx, {} pathUSD",
                tx_count, balance_formatted
            ),
            tx_hash: None,
        })
    }
}
