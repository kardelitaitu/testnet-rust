use super::{SepoliaTask, TaskContext, TaskResult};
use crate::utils::calc::calc_pct_rounded;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// USDC+ (C+) on Sepolia — the token we stake
const USDC_PLUS: &str = "0xe815718d44694ec4637cb775c468d87f6e15b538";
/// C+ Staking vault on Sepolia (ERC-4626)
const STAKING_VAULT: &str = "0x753937137Eb92871A6F3517514d4f1Ee860e3FDF";

/// Combined ABI: ERC-20 (balanceOf, allowance, approve) + staking (deposit)
const STAKE_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":true,"inputs":[{"name":"_owner","type":"address"},{"name":"_spender","type":"address"}],"name":"allowance","outputs":[{"name":"","type":"uint256"}],"type":"function"},
    {"constant":false,"inputs":[{"name":"spender","type":"address"},{"name":"value","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"type":"function"},
    {"name":"deposit","type":"function","outputs":[{"name":"","type":"uint256"}],"inputs":[{"name":"assets_","type":"uint256"},{"name":"receiver_","type":"address"}]}
]"#;

async fn get_cplus_balance(provider: &Provider<Http>, wallet: Address) -> Result<U256> {
    let addr: Address = USDC_PLUS.parse()?;
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

async fn get_cplus_allowance(
    provider: &Provider<Http>,
    wallet: Address,
    spender: Address,
) -> Result<U256> {
    let addr: Address = USDC_PLUS.parse()?;
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

pub struct StakeUsdcPlusTask;

#[async_trait]
impl SepoliaTask for StakeUsdcPlusTask {
    fn name(&self) -> &str {
        "07_stakeUsdcPlus"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let cplus_addr: Address = USDC_PLUS.parse()?;
        let vault_addr: Address = STAKING_VAULT.parse()?;

        // --- 1. Check C+ balance ---
        let cplus_balance = get_cplus_balance(provider, address).await?;

        // --- 2. Calculate 10% of C+ balance, round to nearest whole C+ ---
        let stake_amount = calc_pct_rounded(cplus_balance.as_u128(), 10, 100, 18);
        let dec18: u128 = 1_000_000_000_000_000_000;
        let whole_cplus = stake_amount / dec18;

        if whole_cplus == 0 {
            return Ok(TaskResult {
                success: false,
                message: "10% of C+ balance rounds to 0, nothing to stake".to_string(),
            });
        }

        // --- 3. Check allowance, approve if needed ---
        let allowance = get_cplus_allowance(provider, address, vault_addr).await?;

        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        if allowance < U256::from(stake_amount) {
            let cplus_contract = Contract::new(
                cplus_addr,
                serde_json::from_str::<ethers::abi::Abi>(STAKE_ABI)?,
                Arc::new(middleware.clone()),
            );

            // Use max uint256 for unlimited approval
            let approve_call = cplus_contract
                .method::<_, H256>("approve", (vault_addr, U256::MAX))?
                .gas(50_000);
            let approve_tx = approve_call.send().await?;

            let _ = approve_tx
                .confirmations(1)
                .interval(Duration::from_millis(500))
                .await?;
        }

        // --- 4. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 5. Execute deposit on C+ staking vault ---
        let vault_contract = Contract::new(
            vault_addr,
            serde_json::from_str::<ethers::abi::Abi>(STAKE_ABI)?,
            Arc::new(middleware.clone()),
        );

        let deposit_call = vault_contract
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
                "Staked {} C+ in staking vault (tx: {:?})",
                whole_cplus, tx_hash
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_is_correct() {
        let task = StakeUsdcPlusTask;
        assert_eq!(task.name(), "07_stakeUsdcPlus");
    }
}
