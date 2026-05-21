use super::{SepoliaTask, TaskContext, TaskResult};
use crate::utils::calc::calc_eighty_pct_6dec;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// USDT+ (T+) on Sepolia
const USDT_PLUS: &str = "0xe20534a32f9162488a90026f268a74fbe28d272d";
/// USDT on Sepolia (payment token)
const USDT: &str = "0xaa8e23fb1079ea71e0a56f48a2aa51851d8433d0";

/// Full ABI: ERC-20 approve + USDT+ mint((address,address,address,uint256,uint256))
const MINT_ABI: &str = r#"[
    {"constant":false,"inputs":[{"name":"spender","type":"address"},{"name":"amount","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"type":"function"},
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"},
    {"constant":true,"inputs":[{"name":"_owner","type":"address"},{"name":"_spender","type":"address"}],"name":"allowance","outputs":[{"name":"","type":"uint256"}],"type":"function"},
    {"name":"mint","type":"function","outputs":[],"inputs":[{"name":"order","type":"tuple","components":[{"name":"benefactor","type":"address"},{"name":"beneficiary","type":"address"},{"name":"collateral","type":"address"},{"name":"collateralAmount","type":"uint256"},{"name":"overlayerWrapAmount","type":"uint256"}]}]}
]"#;

async fn get_usdt_balance(provider: &Provider<Http>, wallet: Address) -> Result<U256> {
    let addr: Address = USDT.parse()?;
    let contract = Contract::new(addr, serde_json::from_str::<ethers::abi::Abi>(MINT_ABI)?, Arc::new(provider.clone()));
    Ok(contract.method::<_, U256>("balanceOf", wallet)?.call().await?)
}

async fn get_usdt_allowance(provider: &Provider<Http>, wallet: Address, spender: Address) -> Result<U256> {
    let addr: Address = USDT.parse()?;
    let contract = Contract::new(addr, serde_json::from_str::<ethers::abi::Abi>(MINT_ABI)?, Arc::new(provider.clone()));
    Ok(contract.method::<_, U256>("allowance", (wallet, spender))?.call().await?)
}

pub struct MintUsdtPlusTask;

#[async_trait]
impl SepoliaTask for MintUsdtPlusTask {
    fn name(&self) -> &str {
        "02_mintUsdtPlus"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let usdt_addr: Address = USDT.parse()?;
        let usdt_plus_addr: Address = USDT_PLUS.parse()?;
        // Check USDT balance - we need at least some USDT to mint T+
        let usdt_balance = get_usdt_balance(provider, address).await?;

        // Calculate 80% of USDT balance, rounded to nearest whole USDT
        let mint_amount = calc_eighty_pct_6dec(usdt_balance.as_u128());
        let required = U256::from(mint_amount);
        let whole_usdt = mint_amount / 1_000_000u128;

        if whole_usdt == 0 {
            return Ok(TaskResult {
                success: false,
                message: "1% of USDT balance rounds to 0, nothing to mint".to_string(),
            });
        }

        // Check allowance
        let allowance = get_usdt_allowance(provider, address, usdt_plus_addr).await?;

        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        if allowance < required {
            let usdt_contract = Contract::new(
                usdt_addr,
                serde_json::from_str::<ethers::abi::Abi>(MINT_ABI)?,
                Arc::new(middleware.clone()),
            );

            let approve_call = usdt_contract
                .method::<_, H256>("approve", (usdt_plus_addr, required))?
                .gas(50000);
            let approve_tx = approve_call.send().await?;

            let _ = approve_tx
                .confirmations(1)
                .interval(Duration::from_millis(500))
                .await?;
        }

        // Get gas fees from GasManager
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // Mint T+: call mint((benefactor, beneficiary, collateral, collateralAmount, overlayerWrapAmount))
        let usdt_plus_contract = Contract::new(
            usdt_plus_addr,
            serde_json::from_str::<ethers::abi::Abi>(MINT_ABI)?,
            Arc::new(middleware.clone()),
        );

        let overlayer_wrap = U256::from(whole_usdt) * U256::from(10u128.pow(18)); // T+ amount (18 decimals)

        // Pass the struct as a 5-element tuple wrapped in a 1-tuple
        let order = (
            address,              // benefactor
            address,              // beneficiary (same wallet)
            usdt_addr,            // collateral = USDT
            U256::from(mint_amount), // collateralAmount = 5 USDT
            overlayer_wrap,       // overlayerWrapAmount = 5 T+
        );

        let mint_call = usdt_plus_contract
            .method::<((Address, Address, Address, U256, U256),), H256>("mint", (order,))?
            .gas(200_000)
            .gas_price(max_fee);
        let mint_tx = mint_call.send().await
            .context("Failed to send mint tx")?;

        let tx_hash = mint_tx.tx_hash();

        let receipt = mint_tx
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));
        Ok(TaskResult {
            success,
            message: format!(
                "Minted {} T+ (tx: {:?})",
                whole_usdt, tx_hash
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_is_correct() {
        let task = MintUsdtPlusTask;
        assert_eq!(task.name(), "02_mintUsdtPlus");
    }
}
