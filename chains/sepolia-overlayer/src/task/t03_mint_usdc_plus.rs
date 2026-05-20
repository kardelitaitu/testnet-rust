use super::{SepoliaTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// USDC+ (C+) on Sepolia
const USDC_PLUS: &str = "0xe815718d44694ec4637cb775c468d87f6e15b538";
/// USDC on Sepolia (payment token)
const USDC: &str = "0x94a9d9ac8a22534e3faca9f4e7f2e2cf85d5e4c8";

/// Full ABI: ERC-20 approve + USDC+ mint((address,address,address,uint256,uint256))
const MINT_ABI: &str = r#"[
    {"constant":false,"inputs":[{"name":"spender","type":"address"},{"name":"amount","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"type":"function"},
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"},
    {"constant":true,"inputs":[{"name":"_owner","type":"address"},{"name":"_spender","type":"address"}],"name":"allowance","outputs":[{"name":"","type":"uint256"}],"type":"function"},
    {"name":"mint","type":"function","outputs":[],"inputs":[{"name":"order","type":"tuple","components":[{"name":"benefactor","type":"address"},{"name":"beneficiary","type":"address"},{"name":"collateral","type":"address"},{"name":"collateralAmount","type":"uint256"},{"name":"overlayerWrapAmount","type":"uint256"}]}]}
]"#;

async fn get_usdc_balance(provider: &Provider<Http>, wallet: Address) -> Result<U256> {
    let addr: Address = USDC.parse()?;
    let contract = Contract::new(addr, serde_json::from_str::<ethers::abi::Abi>(MINT_ABI)?, Arc::new(provider.clone()));
    Ok(contract.method::<_, U256>("balanceOf", wallet)?.call().await?)
}

async fn get_usdc_allowance(provider: &Provider<Http>, wallet: Address, spender: Address) -> Result<U256> {
    let addr: Address = USDC.parse()?;
    let contract = Contract::new(addr, serde_json::from_str::<ethers::abi::Abi>(MINT_ABI)?, Arc::new(provider.clone()));
    Ok(contract.method::<_, U256>("allowance", (wallet, spender))?.call().await?)
}

pub struct MintUsdcPlusTask;

#[async_trait]
impl SepoliaTask for MintUsdcPlusTask {
    fn name(&self) -> &str {
        "03_mintUsdcPlus"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let usdc_addr: Address = USDC.parse()?;
        let usdc_plus_addr: Address = USDC_PLUS.parse()?;
        // Check USDC balance
        let usdc_balance = get_usdc_balance(provider, address).await?;

        // Calculate 80% of USDC balance, rounded to nearest whole USDC
        let pct_raw = usdc_balance.as_u128() * 80 / 100;
        let rounding = 500_000u128;
        let whole_usdc = (pct_raw + rounding) / 1_000_000u128;
        let mint_amount = whole_usdc * 1_000_000u128;
        let required = U256::from(mint_amount);

        if whole_usdc == 0 {
            return Ok(TaskResult {
                success: false,
                message: "1% of USDC balance rounds to 0, nothing to mint".to_string(),
            });
        }

        // Check allowance
        let allowance = get_usdc_allowance(provider, address, usdc_plus_addr).await?;

        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        if allowance < required {
            let usdc_contract = Contract::new(
                usdc_addr,
                serde_json::from_str::<ethers::abi::Abi>(MINT_ABI)?,
                Arc::new(middleware.clone()),
            );

            let approve_call = usdc_contract
                .method::<_, H256>("approve", (usdc_plus_addr, required))?
                .gas(50000);
            let approve_tx = approve_call.send().await?;

            let _ = approve_tx
                .confirmations(1)
                .interval(Duration::from_millis(500))
                .await?;
        }

        // Get gas fees from GasManager
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // Mint C+
        let usdc_plus_contract = Contract::new(
            usdc_plus_addr,
            serde_json::from_str::<ethers::abi::Abi>(MINT_ABI)?,
            Arc::new(middleware.clone()),
        );

        let overlayer_wrap = U256::from(whole_usdc) * U256::from(10u128.pow(18)); // C+ amount (18 decimals)

        let order = (
            address,              // benefactor
            address,              // beneficiary
            usdc_addr,            // collateral = USDC
            U256::from(mint_amount),
            overlayer_wrap,
        );

        let mint_call = usdc_plus_contract
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
                "Minted {} C+ (tx: {:?})",
                whole_usdc, tx_hash
            ),
        })
    }
}
