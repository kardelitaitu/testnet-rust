use super::{SepoliaTask, TaskContext, TaskResult};
use crate::utils::calc::calc_pct_rounded;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// C+ Staking vault on Sepolia — also acts as the sOverl... token (ERC-4626 vault)
const STAKING_VAULT: &str = "0x753937137Eb92871A6F3517514d4f1Ee860e3FDF";

/// ABI: balanceOf (sOverl... shares) + redeem(uint256,address,address)
const REDEEM_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"name":"redeem","type":"function","outputs":[{"name":"","type":"uint256"}],"inputs":[{"name":"shares_","type":"uint256"},{"name":"receiver_","type":"address"},{"name":"owner_","type":"address"}]}
]"#;

async fn get_sovl_balance(provider: &Provider<Http>, wallet: Address) -> Result<U256> {
    let addr: Address = STAKING_VAULT.parse()?;
    let contract = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(REDEEM_ABI)?,
        Arc::new(provider.clone()),
    );
    Ok(contract.method::<_, U256>("balanceOf", wallet)?.call().await?)
}

pub struct UnstakeCplusTask;

#[async_trait]
impl SepoliaTask for UnstakeCplusTask {
    fn name(&self) -> &str {
        "09_unstakeCplus"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let staking_addr: Address = STAKING_VAULT.parse()?;

        // --- 1. Check sOverl... (staked share) balance ---
        let sovl_balance = get_sovl_balance(provider, address).await?;

        // --- 2. Calculate 2% of sOverl... balance, round to 2 decimal places ---
        // sOverl... has 18 decimals; rounding to 2dp means rounding to 10^16 units
        const DP2_UNIT: u128 = 10_000_000_000_000_000; // 10^16 = 0.01 shares
        let mut shares_amount = calc_pct_rounded(sovl_balance.as_u128(), 2, 100, 16);
        if shares_amount < DP2_UNIT {
            shares_amount = DP2_UNIT; // minimum 0.01 shares
        }
        let shares_display = shares_amount as f64 / 1e18;

        // --- 3. No approval needed — redeem() burns sOverl... directly from caller ---

        // --- 4. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 5. Execute redeem on C+ staking vault ---
        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        let vault_contract = Contract::new(
            staking_addr,
            serde_json::from_str::<ethers::abi::Abi>(REDEEM_ABI)?,
            Arc::new(middleware.clone()),
        );

        // redeem(uint256 shares_, address receiver_, address owner_)
        let redeem_call = vault_contract
            .method::<(U256, Address, Address), H256>(
                "redeem",
                (U256::from(shares_amount), address, address),
            )?
            .gas(120_000)
            .gas_price(max_fee);
        let redeem_tx = redeem_call.send().await.context("Failed to send redeem tx")?;

        let tx_hash = redeem_tx.tx_hash();
        let receipt = redeem_tx
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));
        Ok(TaskResult {
            success,
            message: format!(
                "Redeemed {:.2} sOverl... for underlying (tx: {:?})",
                shares_display, tx_hash
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_is_correct() {
        let task = UnstakeCplusTask;
        assert_eq!(task.name(), "09_unstakeCplus");
    }
}

