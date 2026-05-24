use super::{SepoliaTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// USDC+ (C+) on Sepolia — the overlayer contract we redeem from
const USDC_PLUS: &str = "0xe815718d44694ec4637cb775c468d87f6e15b538";
/// aEthUSDC (aUSDC) on Sepolia AAVE — the collateral token we receive
/// Confirmed from tx: 0x66c8daa2273ca5dc144db2dbb66ebf46a1609e8ff9d2cdb68be373f985a46b6d
const AUSDC: &str = "0x16da4541ad1807f4443d92d26044c1147406eb80";

/// Minimal ABI: balanceOf, decimals, and redeem((address,address,address,uint256,uint256))
const REDEEM_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"},
    {"name":"redeem","type":"function","outputs":[],"inputs":[{"name":"order_","type":"tuple","components":[{"name":"benefactor","type":"address"},{"name":"beneficiary","type":"address"},{"name":"collateral","type":"address"},{"name":"collateralAmount","type":"uint256"},{"name":"overlayerWrapAmount","type":"uint256"}]}]}
]"#;

async fn get_ausdc_balance(provider: &Provider<Http>, contract_addr: Address) -> Result<U256> {
    let addr: Address = AUSDC.parse()?;
    let token = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(REDEEM_ABI)?,
        Arc::new(provider.clone()),
    );
    Ok(token
        .method::<_, U256>("balanceOf", contract_addr)?
        .call()
        .await?)
}

pub struct RedeemToAusdcTask;

#[async_trait]
impl SepoliaTask for RedeemToAusdcTask {
    fn name(&self) -> &str {
        "22_redeemToAusdc"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let usdc_plus_addr: Address = USDC_PLUS.parse()?;
        let ausdc_addr: Address = AUSDC.parse()?;

        // --- 1. Check contract's aUSDC liquidity and take 1% ---
        let contract_ausdc = get_ausdc_balance(provider, usdc_plus_addr).await?;
        let contract_ausdc_6dec = contract_ausdc.as_u128(); // aUSDC has 6 decimals

        if contract_ausdc_6dec == 0 {
            return Ok(TaskResult {
                success: false,
                message: "Overlayer contract has 0 aUSDC liquidity, nothing to redeem".to_string(),
            });
        }

        // collateralAmount is 1% of contract's aUSDC, in 6-dec raw units (floor)
        let collateral_amount = contract_ausdc_6dec / 100u128;

        // Recalculate overlayerWrapAmount from capped collateralAmount
        // 1 C+ (18-dec) = 1 USDC (6-dec), so overlayer = collateral * 10^12
        let capped_redeem = collateral_amount * 1_000_000_000_000u128;
        let redeem_display = collateral_amount as f64 / 1_000_000.0;

        // --- 4. No approval needed — redeem() burns C+ directly from caller ---

        // --- 5. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 6. Execute redeem ---
        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        let usdc_plus_contract = Contract::new(
            usdc_plus_addr,
            serde_json::from_str::<ethers::abi::Abi>(REDEEM_ABI)?,
            Arc::new(middleware.clone()),
        );

        let order = (
            address,                       // benefactor — gets aUSDC back
            address,                       // beneficiary (same wallet)
            ausdc_addr,                    // collateral = aEthUSDC (aUSDC)
            U256::from(collateral_amount), // collateralAmount (6 decimals)
            U256::from(capped_redeem),     // overlayerWrapAmount (18 decimals)
        );

        let redeem_call = usdc_plus_contract
            .method::<((Address, Address, Address, U256, U256),), H256>("redeem", (order,))?
            .gas(300_000)
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
                "Redeemed {:.6} C+ → {:.6} aUSDC (tx: {:?})",
                redeem_display, redeem_display, tx_hash
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_is_correct() {
        let task = RedeemToAusdcTask;
        assert_eq!(task.name(), "22_redeemToAusdc");
    }
}
