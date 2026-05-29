use super::{confirm_with_retry, SepoliaTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;

/// AAVE faucet contract on Sepolia — mints test ERC20 tokens
const AAVE_FAUCET: &str = "0xC959483DBa39aa9E78757139af0e9a2EDEb3f42D";

/// USDC on Sepolia
const USDC: &str = "0x94a9d9ac8a22534e3faca9f4e7f2e2cf85d5e4c8";

/// ABI: mint(address token, address to, uint256 amount)
const FAUCET_ABI: &str = r#"[
    {"name":"mint","type":"function","inputs":[{"name":"token","type":"address"},{"name":"to","type":"address"},{"name":"amount","type":"uint256"}],"outputs":[],"stateMutability":"nonpayable"}
]"#;

pub struct AaveUsdcFaucetTask;

#[async_trait]
impl SepoliaTask for AaveUsdcFaucetTask {
    fn name(&self) -> &str {
        "11_aaveUsdcFaucet"
    }

    fn weight(&self) -> u32 {
        5
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let faucet_addr: Address = AAVE_FAUCET.parse()?;
        let usdc_addr: Address = USDC.parse()?;
        // 10,000 USDC (6 decimals) = 10,000,000,000 raw
        let amount = U256::from(10_000u64) * U256::from(10u64).pow(6.into());

        // --- 1. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 2. Execute mint on AAVE faucet ---
        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        let faucet_contract = Contract::new(
            faucet_addr,
            serde_json::from_str::<ethers::abi::Abi>(FAUCET_ABI)?,
            Arc::new(middleware.clone()),
        );

        // mint(address token, address to, uint256 amount)
        let mint_call = faucet_contract
            .method::<(Address, Address, U256), H256>("mint", (usdc_addr, address, amount))?
            .gas(80_000)
            .gas_price(max_fee);
        let mint_tx = mint_call.send().await.context("Failed to send mint tx")?;

        let tx_hash = mint_tx.tx_hash();

        let receipt = confirm_with_retry(tx_hash, provider).await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));
        Ok(TaskResult {
            success,
            message: if success {
                format!("Minted 10,000 USDC from AAVE faucet (tx: {:?})", tx_hash)
            } else {
                format!(
                    "Failed to mint USDC from AAVE faucet - receipt not confirmed (tx: {:?})",
                    tx_hash
                )
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_is_correct() {
        let task = AaveUsdcFaucetTask;
        assert_eq!(task.name(), "11_aaveUsdcFaucet");
    }
}
