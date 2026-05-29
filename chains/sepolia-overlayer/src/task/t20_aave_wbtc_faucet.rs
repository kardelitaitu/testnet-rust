use super::{confirm_with_retry, SepoliaTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;

/// AAVE faucet contract on Sepolia — mints test ERC20 tokens
const AAVE_FAUCET: &str = "0xC959483DBa39aa9E78757139af0e9a2EDEb3f42D";

/// WBTC on Sepolia
const WBTC: &str = "0x29f2d40b0605204364af54ec677bd022da425d03";

/// ABI: mint(address token, address to, uint256 amount)
const FAUCET_ABI: &str = r#"[
    {"name":"mint","type":"function","inputs":[{"name":"token","type":"address"},{"name":"to","type":"address"},{"name":"amount","type":"uint256"}],"outputs":[],"stateMutability":"nonpayable"}
]"#;

pub struct AaveWbtcFaucetTask;

#[async_trait]
impl SepoliaTask for AaveWbtcFaucetTask {
    fn name(&self) -> &str {
        "20_aaveWbtcFaucet"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let faucet_addr: Address = AAVE_FAUCET.parse()?;
        let wbtc_addr: Address = WBTC.parse()?;
        // 1 WBTC (8 decimals) = 100,000,000 raw
        let amount = U256::from(1u64) * U256::from(10u64).pow(8.into());

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
            .method::<(Address, Address, U256), H256>("mint", (wbtc_addr, address, amount))?
            .gas(80_000)
            .gas_price(max_fee);
        let mint_tx = mint_call.send().await.context("Failed to send mint tx")?;

        let tx_hash = mint_tx.tx_hash();

        let receipt = confirm_with_retry(tx_hash, provider).await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));
        Ok(TaskResult {
            success,
            message: if success {
                format!("Minted 1 WBTC from AAVE faucet (tx: {:?})", tx_hash)
            } else {
                format!(
                    "Failed to mint WBTC from AAVE faucet - receipt not confirmed (tx: {:?})",
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
        let task = AaveWbtcFaucetTask;
        assert_eq!(task.name(), "20_aaveWbtcFaucet");
    }
}
