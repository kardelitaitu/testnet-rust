use crate::tasks::{TaskContext, TaskResult, TempoTask};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use std::str::FromStr;

pub struct WalletAnalyticsTask;

#[async_trait]
impl TempoTask for WalletAnalyticsTask {
    fn name(&self) -> &str {
        "19_wallet_analytics"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let address = ctx.wallet.address();

        // 1. Native Balance
        let native_balance = ctx
            .provider
            .get_balance(address, None)
            .await
            .unwrap_or_default();
        let native_formatted = ethers::utils::format_ether(native_balance);

        // 2. Mock some token balance checks
        let _path_usd = Address::from_str("0x20c0000000000000000000000000000000000000").unwrap();

        let mut report = format!("Wallet Analytics for {:?}\n", address);
        report.push_str(&format!("Native Balance: {} TEMPO\n", native_formatted));

        // Add more detail if needed (scanning blocks or DB)
        if let Some(db) = ctx.db.as_ref() {
            let assets = db
                .get_assets_by_type(&format!("{:?}", address), "stablecoin")
                .await
                .unwrap_or_default();
            report.push_str(&format!("Created Assets count: {}\n", assets.len()));
        }

        Ok(TaskResult {
            success: true,
            message: report,
            tx_hash: None,
        })
    }
}
