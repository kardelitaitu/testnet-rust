use super::{confirm_with_retry, SepoliaTask, TaskContext, TaskResult};
use crate::utils::calc::calc_pct_rounded;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// sOverl... token (C+ staking vault shares) on Sepolia
const SOVL_TOKEN: &str = "0x753937137Eb92871A6F3517514d4f1Ee860e3FDF";
/// sC+ farm pool on Sepolia (MasterChef-style deposit(uint256,uint256))
const SCPLUS_POOL: &str = "0x88Fe18C721c9380f80592Cb1496C50C7Ea97ABeB";

/// Pool ID to deposit into
const POOL_ID: u64 = 0;

/// Combined ABI: ERC-20 (balanceOf, allowance, approve) + pool deposit(uint256,uint256)
const POOL_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":true,"inputs":[{"name":"_owner","type":"address"},{"name":"_spender","type":"address"}],"name":"allowance","outputs":[{"name":"","type":"uint256"}],"type":"function"},
    {"constant":false,"inputs":[{"name":"spender","type":"address"},{"name":"value","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"type":"function"},
    {"name":"deposit","type":"function","outputs":[],"inputs":[{"name":"_pid","type":"uint256"},{"name":"_amount","type":"uint256"}]}
]"#;

async fn get_sovl_balance(provider: &Provider<Http>, wallet: Address) -> Result<U256> {
    let addr: Address = SOVL_TOKEN.parse()?;
    let contract = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(POOL_ABI)?,
        Arc::new(provider.clone()),
    );
    Ok(contract.method::<_, U256>("balanceOf", wallet)?.call().await?)
}

async fn get_sovl_allowance(provider: &Provider<Http>, wallet: Address, spender: Address) -> Result<U256> {
    let addr: Address = SOVL_TOKEN.parse()?;
    let contract = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(POOL_ABI)?,
        Arc::new(provider.clone()),
    );
    Ok(contract
        .method::<_, U256>("allowance", (wallet, spender))?
        .call()
        .await?)
}

pub struct AddLiquidityScplusTask;

#[async_trait]
impl SepoliaTask for AddLiquidityScplusTask {
    fn name(&self) -> &str {
        "28_addLiquidityScplus"
    }

    fn weight(&self) -> u32 {
        3
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let sovl_addr: Address = SOVL_TOKEN.parse()?;
        let pool_addr: Address = SCPLUS_POOL.parse()?;

        // --- 1. Check sOverl... balance ---
        let sovl_balance = get_sovl_balance(provider, address).await?;

        // --- 2. Calculate 1% of sOverl balance, round to nearest whole ---
        let deposit_amount = calc_pct_rounded(sovl_balance.as_u128(), 1, 100, 18);
        let dec18: u128 = 1_000_000_000_000_000_000;
        let whole_sovl = deposit_amount / dec18;

        if whole_sovl == 0 {
            return Ok(TaskResult {
                success: false,
                message: "1% of sOverl balance rounds to 0, nothing to deposit".to_string(),
            });
        }

        // --- 3. Check allowance, approve if needed ---
        let allowance = get_sovl_allowance(provider, address, pool_addr).await?;

        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        if allowance < U256::from(deposit_amount) {
            let sovl_contract = Contract::new(
                sovl_addr,
                serde_json::from_str::<ethers::abi::Abi>(POOL_ABI)?,
                Arc::new(middleware.clone()),
            );

            // Use max uint256 for unlimited approval
            let approve_call = sovl_contract
                .method::<_, H256>("approve", (pool_addr, U256::MAX))?
                .gas(50_000);
            let approve_tx = approve_call.send().await?;

            let _ = approve_tx.confirmations(1).interval(Duration::from_millis(500)).await?;
        }

        // --- 4. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 5. Execute deposit(uint256 _pid, uint256 _amount) on pool contract ---
        let pool_contract = Contract::new(
            pool_addr,
            serde_json::from_str::<ethers::abi::Abi>(POOL_ABI)?,
            Arc::new(middleware.clone()),
        );

        let deposit_call = pool_contract
            .method::<(U256, U256), H256>("deposit", (U256::from(POOL_ID), U256::from(deposit_amount)))?
            .gas(150_000)
            .gas_price(max_fee);
        let deposit_tx = deposit_call.send().await.context("Failed to send deposit tx")?;

        let tx_hash = deposit_tx.tx_hash();

        let receipt = confirm_with_retry(tx_hash, provider).await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));
        Ok(TaskResult {
            success,
            message: if success {
                format!(
                    "Deposited {} sOverl into sC+ pool {} (tx: {:?})",
                    whole_sovl, POOL_ID, tx_hash
                )
            } else {
                format!(
                    "Failed to deposit sOverl into sC+ pool {} - receipt not confirmed (tx: {:?})",
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
        let task = AddLiquidityScplusTask;
        assert_eq!(task.name(), "28_addLiquidityScplus");
    }

    #[test]
    fn test_pool_id_is_zero() {
        assert_eq!(POOL_ID, 0);
    }

    #[test]
    fn test_weight_is_three() {
        let task = AddLiquidityScplusTask;
        assert_eq!(task.weight(), 3);
    }
}
