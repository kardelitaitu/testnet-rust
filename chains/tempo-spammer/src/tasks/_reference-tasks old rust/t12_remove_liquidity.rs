use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::str::FromStr;

ethers::contract::abigen!(
    IStablecoinDexWithdraw,
    r#"[
        function withdraw(address token, uint128 amount) external
        function balanceOf(address user, address token) view returns (uint128)
    ]"#
);

pub struct RemoveLiquidityTask;

#[async_trait]
impl TempoTask for RemoveLiquidityTask {
    fn name(&self) -> &str {
        "12_remove_liquidity"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let dex_address = Address::from_str("0x10c0000000000000000000000000000000000000")?;
        let token_address = Address::from_str("0x20c0000000000000000000000000000000000001")?; // AlphaUSD

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let dex = IStablecoinDexWithdraw::new(dex_address, client.clone());

        let wallet_addr = ctx.wallet.address();

        // 1. Check DEX Balance
        let dex_balance = dex
            .balance_of(wallet_addr, token_address)
            .call()
            .await
            .unwrap_or_default();

        if dex_balance == 0 {
            return Ok(TaskResult {
                success: false,
                message: "No DEX balance found for AlphaUSD to withdraw (Remove Liquidity)."
                    .to_string(),
                tx_hash: None,
            });
        }

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // 2. Withdraw
        println!("Withdrawing {} AlphaUSD from DEX...", dex_balance);
        let tx_withdraw = dex
            .withdraw(token_address, dex_balance)
            .gas_price(bumped_gas_price);
        let pending_withdraw = tx_withdraw.send().await?;
        let receipt = pending_withdraw
            .await?
            .context("Remove Liquidity (Withdraw) failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!("Removed Liquidity (Withdrew {}). Tx: {}", dex_balance, hash),
            tx_hash: Some(hash),
        })
    }
}
