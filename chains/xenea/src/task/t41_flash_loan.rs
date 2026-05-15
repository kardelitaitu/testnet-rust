use crate::task::{Task, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct FlashLoanTestTask;

impl FlashLoanTestTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for FlashLoanTestTask {
    fn name(&self) -> &str {
        "41_flashLoanTest"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let _address = wallet.address();

        const AAVE_V3_POOL: &str = "0x87870Bca3F3f6335e32cdC0d59b7b238621D6576";
        const WETH: &str = "0x4200000000000000000000000000000000000006";

        let pool_address: Address = AAVE_V3_POOL.parse()?;
        let _weth_address: Address = WETH.parse()?;

        let mut summary_parts = Vec::new();

        let pool_code_len = provider.get_code(pool_address, None).await?.len();
        summary_parts.push(format!("Aave V3: {} bytes", pool_code_len));

        // Check availability strictly if code exists
        if pool_code_len > 0 {
            let lending_abi = r#"[
                {"type":"function","name":"getReserveData(address)","stateMutability":"view","inputs":[{"name":"asset","type":"address"}],"outputs":[{"name":"","type":"tuple","components":[{"name":"aTokenAddress","type":"address"},{"name":"stableDebtTokenAddress","type":"address"},{"name":"variableDebtTokenAddress","type":"address"},{"name":"interestRateStrategyAddress","type":"address"},{"name":"currentStableDebt","type":"uint128"},{"name":"currentVariableDebt","type":"uint128"},{"name":"lastUpdateTimestamp","type":"uint128"},{"name":"liquidityIndex","type":"uint128"},{"name":"variableBorrowIndex","type":"uint128"},{"name":"lastUpdateTimestamp","type":"uint128"}]}]},
                {"type":"function","name":"getReservesList()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"address[]"}]}
             ]"#;
            let abi: abi::Abi = serde_json::from_str(lending_abi)?;
            let pool = Contract::new(pool_address, abi, Arc::new(provider.clone()));
            match pool
                .method::<_, Vec<Address>>("getReservesList", ())?
                .call()
                .await
            {
                Ok(reserves) => summary_parts.push(format!("Reserves: {}", reserves.len())),
                Err(_) => summary_parts.push("Reserves: N/A".to_string()),
            }
        } else {
            summary_parts.push("Not deployed".to_string());
        }

        Ok(TaskResult {
            success: true,
            message: format!("Flash Loan Check: {}", summary_parts.join(" | ")),
            tx_hash: None,
        })
    }
}
