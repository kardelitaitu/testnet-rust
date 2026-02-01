use crate::tasks::{TaskContext, TaskResult, TempoTask};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;

pub struct WalletActivityTask;

#[async_trait]
impl TempoTask for WalletActivityTask {
    fn name(&self) -> &str {
        "20_wallet_activity"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let address = ctx.wallet.address();

        // 1. Transaction Count (Nonce)
        let tx_count = ctx
            .provider
            .get_transaction_count(address, None)
            .await
            .unwrap_or_default();

        Ok(TaskResult {
            success: true,
            message: format!(
                "Wallet Activity: {} on-chain transactions detected.",
                tx_count
            ),
            tx_hash: None,
        })
    }
}
