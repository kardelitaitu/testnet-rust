use super::{SepoliaTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// AAVE faucet contract on Sepolia — mints test ERC20 tokens
const AAVE_FAUCET: &str = "0xC959483DBa39aa9E78757139af0e9a2EDEb3f42D";

/// USDT on Sepolia
const USDT: &str = "0xaa8e23fb1079ea71e0a56f48a2aa51851d8433d0";

/// ABI: mint(address token, address to, uint256 amount)
const FAUCET_ABI: &str = r#"[
    {"name":"mint","type":"function","inputs":[{"name":"token","type":"address"},{"name":"to","type":"address"},{"name":"amount","type":"uint256"}],"outputs":[],"stateMutability":"nonpayable"}
]"#;

pub struct AaveUsdtFaucetTask;

#[async_trait]
impl SepoliaTask for AaveUsdtFaucetTask {
    fn name(&self) -> &str {
        "10_aaveUsdtFaucet"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let faucet_addr: Address = AAVE_FAUCET.parse()?;
        let usdt_addr: Address = USDT.parse()?;
        // 10,000 USDT (6 decimals) = 10,000,000,000 raw
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
            .method::<(Address, Address, U256), H256>("mint", (usdt_addr, address, amount))?
            .gas(80_000)
            .gas_price(max_fee);
        let mint_tx = mint_call.send().await.context("Failed to send mint tx")?;

        let tx_hash = mint_tx.tx_hash();

        let receipt = mint_tx
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));
        Ok(TaskResult {
            success,
            message: format!(
                "Minted 10,000 USDT from AAVE faucet (tx: {:?})",
                tx_hash
            ),
        })
    }
}
