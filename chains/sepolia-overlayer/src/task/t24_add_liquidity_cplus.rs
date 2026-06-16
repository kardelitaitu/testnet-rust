use super::{confirm_with_retry, SepoliaTask, TaskContext, TaskResult};
use crate::utils::calc::calc_pct_rounded;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// USDC+ (C+) on Sepolia — the token we deposit as liquidity
const USDC_PLUS: &str = "0xe815718d44694ec4637cb775c468d87f6e15b538";
/// Liquidity pool contract for C+ on Sepolia (MasterChef-style deposit(uint256,uint256))
const LIQUIDITY_POOL: &str = "0x7C493D7b7bA0B281fb4Bb29d26Fa9E1754D0Bd1A";

/// Pool ID to deposit into
const POOL_ID: u64 = 0;

/// Combined ABI: ERC-20 (balanceOf, allowance, approve) + pool deposit(uint256,uint256)
const POOL_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":true,"inputs":[{"name":"_owner","type":"address"},{"name":"_spender","type":"address"}],"name":"allowance","outputs":[{"name":"","type":"uint256"}],"type":"function"},
    {"constant":false,"inputs":[{"name":"spender","type":"address"},{"name":"value","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"type":"function"},
    {"name":"deposit","type":"function","outputs":[],"inputs":[{"name":"_pid","type":"uint256"},{"name":"_amount","type":"uint256"}]}
]"#;

async fn get_cplus_balance(provider: &Provider<Http>, wallet: Address) -> Result<U256> {
    let addr: Address = USDC_PLUS.parse()?;
    let contract = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(POOL_ABI)?,
        Arc::new(provider.clone()),
    );
    Ok(contract.method::<_, U256>("balanceOf", wallet)?.call().await?)
}

async fn get_cplus_allowance(provider: &Provider<Http>, wallet: Address, spender: Address) -> Result<U256> {
    let addr: Address = USDC_PLUS.parse()?;
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

pub struct AddLiquidityCplusTask;

#[async_trait]
impl SepoliaTask for AddLiquidityCplusTask {
    fn name(&self) -> &str {
        "24_addLiquidityCplus"
    }

    fn weight(&self) -> u32 {
        3
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let cplus_addr: Address = USDC_PLUS.parse()?;
        let pool_addr: Address = LIQUIDITY_POOL.parse()?;

        // --- 1. Check C+ balance ---
        let cplus_balance = get_cplus_balance(provider, address).await?;

        // --- 2. Calculate 1% of C+ balance, round to nearest whole C+ ---
        let deposit_amount = calc_pct_rounded(cplus_balance.as_u128(), 1, 100, 18);
        let dec18: u128 = 1_000_000_000_000_000_000;
        let whole_cplus = deposit_amount / dec18;

        if whole_cplus == 0 {
            return Ok(TaskResult {
                success: false,
                message: "1% of C+ balance rounds to 0, nothing to deposit".to_string(),
            });
        }

        // --- 3. Check allowance, approve if needed ---
        let allowance = get_cplus_allowance(provider, address, pool_addr).await?;

        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        if allowance < U256::from(deposit_amount) {
            let cplus_contract = Contract::new(
                cplus_addr,
                serde_json::from_str::<ethers::abi::Abi>(POOL_ABI)?,
                Arc::new(middleware.clone()),
            );

            // Use max uint256 for unlimited approval
            let approve_call = cplus_contract
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
                    "Deposited {} C+ into pool {} (tx: {:?})",
                    whole_cplus, POOL_ID, tx_hash
                )
            } else {
                format!(
                    "Failed to deposit C+ into pool {} - receipt not confirmed (tx: {:?})",
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
        let task = AddLiquidityCplusTask;
        assert_eq!(task.name(), "24_addLiquidityCplus");
    }

    #[test]
    fn test_pool_id_is_zero() {
        assert_eq!(POOL_ID, 0);
    }

    #[test]
    fn test_weight_is_three() {
        let task = AddLiquidityCplusTask;
        assert_eq!(task.weight(), 3);
    }
}
