use super::{DaChainTask, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;

pub struct DaChainCheckBalanceTask;

#[async_trait]
impl DaChainTask for DaChainCheckBalanceTask {
    fn name(&self) -> &str {
        "01_checkBalance"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let address = ctx.wallet.address();
        println!("[DEBUG] Checking balance for: {:?}", address);

        // Get confirmed nonce
        let nonce = ctx.provider.get_transaction_count(address, None).await?;
        println!("[DEBUG] Confirmed nonce: {}", nonce);

        // Check pending nonce
        let pending_nonce = ctx
            .provider
            .get_transaction_count(address, Some(BlockId::Number(BlockNumber::Pending)))
            .await?;
        println!("[DEBUG] Pending nonce: {}", pending_nonce);

        if pending_nonce > nonce {
            println!(
                "[WARNING] Pending transactions detected ({} pending)",
                pending_nonce - nonce
            );
        }

        let balance = ctx.provider.get_balance(address, None).await?;
        println!(
            "[DEBUG] Balance: {} DACC",
            ethers::utils::format_ether(balance)
        );

        // Get gas fees
        println!("[DEBUG] Getting gas fees...");
        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let max_fee_gwei: f64 = max_fee.as_u128() as f64 / 1e9;
        let priority_fee_gwei: f64 = priority_fee.as_u128() as f64 / 1e9;
        println!(
            "[DEBUG] Gas fees - max: {} wei ({:.2} Gwei), priority: {} wei ({:.2} Gwei)",
            max_fee, max_fee_gwei, priority_fee, priority_fee_gwei
        );

        Ok(TaskResult {
            success: true,
            message: format!(
                "Balance: {} DACC | Gas: {:.2} Gwei (max), {:.2} Gwei (priority)",
                ethers::utils::format_ether(balance),
                max_fee_gwei,
                priority_fee_gwei
            ),
        })
    }
}
