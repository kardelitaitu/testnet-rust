use super::{SepoliaTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// USDT+ (T+) on Sepolia — the overlayer contract we redeem from
const USDT_PLUS: &str = "0xe20534a32f9162488a90026f268a74fbe28d272d";
/// aEthUSDT (aUSDT) on Sepolia AAVE — the collateral token we receive
/// Confirmed from tx: 0xe7c4c83d2bc5def7f628d22b8e20e832fb1a635235539d3a4a88eb05b13fd82e
const AUSDT: &str = "0xaf0f6e8b0dc5c913bbf4d14c22b4e78dd14310b6";

/// Minimal ABI: balanceOf, decimals, and redeem((address,address,address,uint256,uint256))
const REDEEM_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"},
    {"name":"redeem","type":"function","outputs":[],"inputs":[{"name":"order_","type":"tuple","components":[{"name":"benefactor","type":"address"},{"name":"beneficiary","type":"address"},{"name":"collateral","type":"address"},{"name":"collateralAmount","type":"uint256"},{"name":"overlayerWrapAmount","type":"uint256"}]}]}
]"#;

async fn get_ausdt_balance(provider: &Provider<Http>, contract_addr: Address) -> Result<U256> {
    let addr: Address = AUSDT.parse()?;
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

pub struct RedeemToAusdtTask;

#[async_trait]
impl SepoliaTask for RedeemToAusdtTask {
    fn name(&self) -> &str {
        "21_redeemToAusdt"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let usdt_plus_addr: Address = USDT_PLUS.parse()?;
        let ausdt_addr: Address = AUSDT.parse()?;

        // --- 1. Check contract's aUSDT liquidity and take 1% ---
        let contract_ausdt = get_ausdt_balance(provider, usdt_plus_addr).await?;
        let contract_ausdt_6dec = contract_ausdt.as_u128(); // aUSDT has 6 decimals

        if contract_ausdt_6dec == 0 {
            return Ok(TaskResult {
                success: false,
                message: "Overlayer contract has 0 aUSDT liquidity, nothing to redeem".to_string(),
            });
        }

        // collateralAmount is 1% of contract's aUSDT, in 6-dec raw units (floor)
        let collateral_amount = contract_ausdt_6dec / 100u128;

        // Recalculate overlayerWrapAmount from capped collateralAmount
        // 1 T+ (18-dec) = 1 USDT (6-dec), so overlayer = collateral * 10^12
        let capped_redeem = collateral_amount * 1_000_000_000_000u128;
        let redeem_display = collateral_amount as f64 / 1_000_000.0;

        // --- 4. No approval needed — redeem() burns T+ directly from caller ---

        // --- 5. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 6. Execute redeem ---
        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        let usdt_plus_contract = Contract::new(
            usdt_plus_addr,
            serde_json::from_str::<ethers::abi::Abi>(REDEEM_ABI)?,
            Arc::new(middleware.clone()),
        );

        let order = (
            address,                       // benefactor — gets aUSDT back
            address,                       // beneficiary (same wallet)
            ausdt_addr,                    // collateral = aEthUSDT (aUSDT)
            U256::from(collateral_amount), // collateralAmount (6 decimals)
            U256::from(capped_redeem),     // overlayerWrapAmount (18 decimals)
        );

        let redeem_call = usdt_plus_contract
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
                "Redeemed {:.6} T+ → {:.6} aUSDT (tx: {:?})",
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
        let task = RedeemToAusdtTask;
        assert_eq!(task.name(), "21_redeemToAusdt");
    }
}
