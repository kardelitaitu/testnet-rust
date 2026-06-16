use super::{confirm_with_retry, SepoliaTask, TaskContext, TaskResult};
use crate::utils::calc::calc_pct_rounded;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;

/// Liquidity pool contract on Sepolia (MasterChef-style deposit/withdraw)
const LIQUIDITY_POOL: &str = "0xDa11726d1d66c8c5c7224529f7be58f22b808952";

/// Pool ID to withdraw from
const POOL_ID: u64 = 0;

/// ABI: userInfo (check staked balance) + withdraw(uint256,uint256)
const POOL_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"","type":"uint256"},{"name":"","type":"address"}],"name":"userInfo","outputs":[{"name":"amount","type":"uint256"},{"name":"rewardDebt","type":"int256"}],"type":"function"},
    {"name":"withdraw","type":"function","outputs":[],"inputs":[{"name":"_pid","type":"uint256"},{"name":"_amount","type":"uint256"}]}
]"#;

async fn get_staked_balance(provider: &Provider<Http>, wallet: Address, pool: Address) -> Result<U256> {
    let contract = Contract::new(
        pool,
        serde_json::from_str::<ethers::abi::Abi>(POOL_ABI)?,
        Arc::new(provider.clone()),
    );
    let (amount, _reward_debt): (U256, ethers::types::I256) = contract
        .method::<(U256, Address), (U256, ethers::types::I256)>("userInfo", (U256::from(POOL_ID), wallet))?
        .call()
        .await?;
    Ok(amount)
}

pub struct RemoveLiquidityTplusTask;

#[async_trait]
impl SepoliaTask for RemoveLiquidityTplusTask {
    fn name(&self) -> &str {
        "25_removeLiquidityTplus"
    }

    fn weight(&self) -> u32 {
        3
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let pool_addr: Address = LIQUIDITY_POOL.parse()?;

        // --- 1. Check staked T+ balance in pool ---
        let staked_balance = get_staked_balance(provider, address, pool_addr).await?;

        // --- 2. Calculate 5% of staked balance, round to nearest whole T+ ---
        let withdraw_amount = calc_pct_rounded(staked_balance.as_u128(), 5, 100, 18);
        let dec18: u128 = 1_000_000_000_000_000_000;
        let whole_tplus = withdraw_amount / dec18;

        if whole_tplus == 0 {
            return Ok(TaskResult {
                success: false,
                message: "5% of staked T+ rounds to 0, nothing to withdraw".to_string(),
            });
        }

        // --- 3. No approval needed — withdraw() burns LP shares from caller ---

        // --- 4. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 5. Execute withdraw(uint256 _pid, uint256 _amount) on pool contract ---
        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        let pool_contract = Contract::new(
            pool_addr,
            serde_json::from_str::<ethers::abi::Abi>(POOL_ABI)?,
            Arc::new(middleware.clone()),
        );

        let withdraw_call = pool_contract
            .method::<(U256, U256), H256>("withdraw", (U256::from(POOL_ID), U256::from(withdraw_amount)))?
            .gas(150_000)
            .gas_price(max_fee);
        let withdraw_tx = withdraw_call.send().await.context("Failed to send withdraw tx")?;

        let tx_hash = withdraw_tx.tx_hash();

        let receipt = confirm_with_retry(tx_hash, provider).await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));
        Ok(TaskResult {
            success,
            message: if success {
                format!(
                    "Withdrew {} T+ from pool {} (tx: {:?})",
                    whole_tplus, POOL_ID, tx_hash
                )
            } else {
                format!(
                    "Failed to withdraw T+ from pool {} - receipt not confirmed (tx: {:?})",
                    POOL_ID, tx_hash
                )
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_is_correct() {
        let task = RemoveLiquidityTplusTask;
        assert_eq!(task.name(), "25_removeLiquidityTplus");
    }

    #[test]
    fn test_pool_id_is_zero() {
        assert_eq!(POOL_ID, 0);
    }

    #[test]
    fn test_weight_is_three() {
        let task = RemoveLiquidityTplusTask;
        assert_eq!(task.weight(), 3);
    }
}
