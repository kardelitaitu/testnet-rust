use super::{SepoliaTask, TaskContext, TaskResult};
use crate::utils::calc::calc_pct_rounded;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// USDT+ (T+) on Sepolia — the overlayer contract we redeem from
const USDT_PLUS: &str = "0xe20534a32f9162488a90026f268a74fbe28d272d";
/// USDT on Sepolia — the token we receive back
const USDT: &str = "0xaa8e23fb1079ea71e0a56f48a2aa51851d8433d0";

/// Minimal ABI: balanceOf, decimals, and redeem((address,address,address,uint256,uint256))
const REDEEM_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"},
    {"name":"redeem","type":"function","outputs":[],"inputs":[{"name":"order_","type":"tuple","components":[{"name":"benefactor","type":"address"},{"name":"beneficiary","type":"address"},{"name":"collateral","type":"address"},{"name":"collateralAmount","type":"uint256"},{"name":"overlayerWrapAmount","type":"uint256"}]}]}
]"#;

async fn get_tplus_balance(provider: &Provider<Http>, wallet: Address) -> Result<U256> {
    let addr: Address = USDT_PLUS.parse()?;
    let contract = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(REDEEM_ABI)?,
        Arc::new(provider.clone()),
    );
    Ok(contract
        .method::<_, U256>("balanceOf", wallet)?
        .call()
        .await?)
}

pub struct RedeemUsdtPlusTask;

#[async_trait]
impl SepoliaTask for RedeemUsdtPlusTask {
    fn name(&self) -> &str {
        "04_redeemUsdtPlus"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let usdt_addr: Address = USDT.parse()?;
        let usdt_plus_addr: Address = USDT_PLUS.parse()?;

        // --- 1. Check T+ balance ---
        let tplus_balance = get_tplus_balance(provider, address).await?;

        // --- 2. Calculate 5% of T+ balance, round down to nearest whole T+ ---
        // T+ has 18 decimals, USDT has 6 decimals
        // 1 T+ = 10^18, 1 USDT = 10^6
        let redeem_overlayer = calc_pct_rounded(tplus_balance.as_u128(), 5, 100, 18);
        let dec18: u128 = 1_000_000_000_000_000_000;
        let whole_tplus = redeem_overlayer / dec18;
        let collateral_amount = whole_tplus * 1_000_000u128;

        if whole_tplus == 0 {
            return Ok(TaskResult {
                success: false,
                message: "2% of T+ balance rounds to 0, nothing to redeem".to_string(),
            });
        }

        // --- 3. No approval needed — redeem() burns T+ directly from caller ---

        // --- 4. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 5. Execute redeem ---
        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        let usdt_plus_contract = Contract::new(
            usdt_plus_addr,
            serde_json::from_str::<ethers::abi::Abi>(REDEEM_ABI)?,
            Arc::new(middleware.clone()),
        );

        // Same 5-tuple order struct as mint: (benefactor, beneficiary, collateral, collateralAmount, overlayerWrapAmount)
        let order = (
            address,                       // benefactor — gets USDT back
            address,                       // beneficiary (same wallet)
            usdt_addr,                     // collateral = USDT
            U256::from(collateral_amount), // collateralAmount (6 decimals)
            U256::from(redeem_overlayer),  // overlayerWrapAmount (18 decimals)
        );

        let redeem_call = usdt_plus_contract
            .method::<((Address, Address, Address, U256, U256),), H256>("redeem", (order,))?
            .gas(250_000)
            .gas_price(max_fee);
        let redeem_tx = redeem_call
            .send()
            .await
            .context("Failed to send redeem tx")?;

        let tx_hash = redeem_tx.tx_hash();

        let receipt = redeem_tx
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));
        Ok(TaskResult {
            success,
            message: format!(
                "Redeemed {} T+ → {} USDT (tx: {:?})",
                whole_tplus, whole_tplus, tx_hash
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_is_correct() {
        let task = RedeemUsdtPlusTask;
        assert_eq!(task.name(), "04_redeemUsdtPlus");
    }
}
