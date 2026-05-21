use super::{SepoliaTask, TaskContext, TaskResult};
use crate::utils::calc::calc_pct_rounded;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// USDT+ (T+) on Sepolia — the token we stake
const USDT_PLUS: &str = "0xe20534a32f9162488a90026f268a74fbe28d272d";
/// Staking contract on Sepolia
const STAKING_CONTRACT: &str = "0x079a4Bf1Cbd0E4ce15391340cB46efA6396aBc82";

/// Combined ABI: ERC-20 (balanceOf, allowance, approve) + staking (deposit)
const STAKE_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":true,"inputs":[{"name":"_owner","type":"address"},{"name":"_spender","type":"address"}],"name":"allowance","outputs":[{"name":"","type":"uint256"}],"type":"function"},
    {"constant":false,"inputs":[{"name":"spender","type":"address"},{"name":"value","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"type":"function"},
    {"name":"deposit","type":"function","outputs":[{"name":"","type":"uint256"}],"inputs":[{"name":"assets_","type":"uint256"},{"name":"receiver_","type":"address"}]}
]"#;

async fn get_tplus_balance(provider: &Provider<Http>, wallet: Address) -> Result<U256> {
    let addr: Address = USDT_PLUS.parse()?;
    let contract = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(STAKE_ABI)?,
        Arc::new(provider.clone()),
    );
    Ok(contract
        .method::<_, U256>("balanceOf", wallet)?
        .call()
        .await?)
}

async fn get_tplus_allowance(
    provider: &Provider<Http>,
    wallet: Address,
    spender: Address,
) -> Result<U256> {
    let addr: Address = USDT_PLUS.parse()?;
    let contract = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(STAKE_ABI)?,
        Arc::new(provider.clone()),
    );
    Ok(contract
        .method::<_, U256>("allowance", (wallet, spender))?
        .call()
        .await?)
}

pub struct StakeUsdtPlusTask;

#[async_trait]
impl SepoliaTask for StakeUsdtPlusTask {
    fn name(&self) -> &str {
        "06_stakeUsdtPlus"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let tplus_addr: Address = USDT_PLUS.parse()?;
        let staking_addr: Address = STAKING_CONTRACT.parse()?;

        // --- 1. Check T+ balance ---
        let tplus_balance = get_tplus_balance(provider, address).await?;

        // --- 2. Calculate 5% of T+ balance, round to nearest whole T+ ---
        let stake_amount = calc_pct_rounded(tplus_balance.as_u128(), 5, 100, 18);
        let dec18: u128 = 1_000_000_000_000_000_000;
        let whole_tplus = stake_amount / dec18;

        if whole_tplus == 0 {
            return Ok(TaskResult {
                success: false,
                message: "2% of T+ balance rounds to 0, nothing to stake".to_string(),
            });
        }

        // --- 3. Check allowance, approve if needed ---
        let allowance = get_tplus_allowance(provider, address, staking_addr).await?;

        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        if allowance < U256::from(stake_amount) {
            let tplus_contract = Contract::new(
                tplus_addr,
                serde_json::from_str::<ethers::abi::Abi>(STAKE_ABI)?,
                Arc::new(middleware.clone()),
            );

            // Use max uint256 for unlimited approval
            let approve_call = tplus_contract
                .method::<_, H256>("approve", (staking_addr, U256::MAX))?
                .gas(50_000);
            let approve_tx = approve_call.send().await?;

            let _ = approve_tx
                .confirmations(1)
                .interval(Duration::from_millis(500))
                .await?;
        }

        // --- 4. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 5. Execute deposit on staking contract ---
        let staking_contract = Contract::new(
            staking_addr,
            serde_json::from_str::<ethers::abi::Abi>(STAKE_ABI)?,
            Arc::new(middleware.clone()),
        );

        let deposit_call = staking_contract
            .method::<(U256, Address), H256>("deposit", (U256::from(stake_amount), address))?
            .gas(150_000)
            .gas_price(max_fee);
        let deposit_tx = deposit_call
            .send()
            .await
            .context("Failed to send deposit tx")?;

        let tx_hash = deposit_tx.tx_hash();

        let receipt = deposit_tx
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));
        Ok(TaskResult {
            success,
            message: format!(
                "Staked {} T+ in staking contract (tx: {:?})",
                whole_tplus, tx_hash
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_is_correct() {
        let task = StakeUsdtPlusTask;
        assert_eq!(task.name(), "06_stakeUsdtPlus");
    }
}
