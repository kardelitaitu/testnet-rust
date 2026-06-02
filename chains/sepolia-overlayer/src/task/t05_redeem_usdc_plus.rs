use super::{confirm_with_retry, SepoliaTask, TaskContext, TaskResult};
use crate::utils::calc::calc_pct_rounded;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;

/// USDC+ (C+) on Sepolia — the overlayer contract we redeem from
const USDC_PLUS: &str = "0xe815718d44694ec4637cb775c468d87f6e15b538";
/// USDC on Sepolia — the token we receive back
const USDC: &str = "0x94a9d9ac8a22534e3faca9f4e7f2e2cf85d5e4c8";

/// Minimal ABI: balanceOf, decimals, and redeem((address,address,address,uint256,uint256))
const REDEEM_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"},
    {"name":"redeem","type":"function","outputs":[],"inputs":[{"name":"order_","type":"tuple","components":[{"name":"benefactor","type":"address"},{"name":"beneficiary","type":"address"},{"name":"collateral","type":"address"},{"name":"collateralAmount","type":"uint256"},{"name":"overlayerWrapAmount","type":"uint256"}]}]}
]"#;

async fn get_cplus_balance(provider: &Provider<Http>, wallet: Address) -> Result<U256> {
    let addr: Address = USDC_PLUS.parse()?;
    let contract = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(REDEEM_ABI)?,
        Arc::new(provider.clone()),
    );
    Ok(contract.method::<_, U256>("balanceOf", wallet)?.call().await?)
}

pub struct RedeemUsdcPlusTask;

#[async_trait]
impl SepoliaTask for RedeemUsdcPlusTask {
    fn name(&self) -> &str {
        "05_redeemUsdcPlus"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let usdc_addr: Address = USDC.parse()?;
        let usdc_plus_addr: Address = USDC_PLUS.parse()?;

        // --- 1. Check C+ balance ---
        let cplus_balance = get_cplus_balance(provider, address).await?;

        // --- 2. Calculate 10% of C+ balance, round down to nearest whole C+ ---
        // C+ has 18 decimals, USDC has 6 decimals
        // 1 C+ = 10^18, 1 USDC = 10^6
        let redeem_overlayer = calc_pct_rounded(cplus_balance.as_u128(), 5, 100, 18);
        let dec18: u128 = 1_000_000_000_000_000_000;
        let whole_cplus = redeem_overlayer / dec18;
        let collateral_amount = whole_cplus * 1_000_000u128;

        if whole_cplus == 0 {
            return Ok(TaskResult {
                success: false,
                message: "5% of C+ balance rounds to 0, nothing to redeem".to_string(),
            });
        }

        // --- 3. No approval needed — redeem() burns C+ directly from caller ---

        // --- 4. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 5. Execute redeem ---
        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        let usdc_plus_contract = Contract::new(
            usdc_plus_addr,
            serde_json::from_str::<ethers::abi::Abi>(REDEEM_ABI)?,
            Arc::new(middleware.clone()),
        );

        // Same 5-tuple order struct as mint: (benefactor, beneficiary, collateral, collateralAmount, overlayerWrapAmount)
        let order = (
            address,                       // benefactor — gets USDC back
            address,                       // beneficiary (same wallet)
            usdc_addr,                     // collateral = USDC
            U256::from(collateral_amount), // collateralAmount (6 decimals)
            U256::from(redeem_overlayer),  // overlayerWrapAmount (18 decimals)
        );

        let redeem_call = usdc_plus_contract
            .method::<((Address, Address, Address, U256, U256),), H256>("redeem", (order,))?
            .gas(250_000)
            .gas_price(max_fee);
        let redeem_tx = redeem_call.send().await.context("Failed to send redeem tx")?;

        let tx_hash = redeem_tx.tx_hash();

        let receipt = confirm_with_retry(tx_hash, provider).await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));
        Ok(TaskResult {
            success,
            message: if success {
                format!("Redeemed {} C+ → {} USDC (tx: {:?})", whole_cplus, whole_cplus, tx_hash)
            } else {
                format!("Failed to redeem C+ - receipt not confirmed (tx: {:?})", tx_hash)
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_is_correct() {
        let task = RedeemUsdcPlusTask;
        assert_eq!(task.name(), "05_redeemUsdcPlus");
    }
}
