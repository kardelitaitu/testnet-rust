use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IInfinityName,
    r#"[
        function register(string domain, address referrer) returns (uint256)
        function isAvailable(string domain) view returns (bool)
    ]"#
);

ethers::contract::abigen!(
    IERC20Approval,
    r#"[
        function approve(address spender, uint256 amount) returns (bool)
        function allowance(address owner, address spender) view returns (uint256)
        function balanceOf(address owner) view returns (uint256)
    ]"#
);

pub struct MintDomainTask;

#[async_trait]
impl TempoTask for MintDomainTask {
    fn name(&self) -> &str {
        "15_mint_domain"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let infinity_addr = Address::from_str("0x30c0000000000000000000000000000000000000")?;
        let path_usd_addr = Address::from_str("0x20c0000000000000000000000000000000000000")?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let infinity = IInfinityName::new(infinity_addr, client.clone());
        let token = IERC20Approval::new(path_usd_addr, client.clone());

        let wallet_addr = ctx.wallet.address();

        // 1. Generate Domain
        let domain: String = (0..10)
            .map(|_| rand::thread_rng().sample(rand::distributions::Alphanumeric) as char)
            .collect();
        let domain = domain.to_lowercase();

        println!("Registering domain: {}.tempo", domain);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // 2. Approval
        let allowance = token
            .allowance(wallet_addr, infinity_addr)
            .call()
            .await
            .unwrap_or_default();
        if allowance < U256::from(1000 * 10u128.pow(6)) {
            println!("Approving PathUSD for Infinity Name Service...");
            let tx_approve = token
                .approve(infinity_addr, U256::max_value())
                .gas_price(bumped_gas_price);
            let pending_approve = tx_approve.send().await?;
            pending_approve.await?;
        }

        // 3. Register
        let tx_reg = infinity
            .register(domain.clone(), Address::zero())
            .gas_price(bumped_gas_price);
        let pending_reg = tx_reg.send().await?;
        let receipt = pending_reg.await?.context("Domain registration failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!("Registered domain {}.tempo. Tx: {}", domain, hash),
            tx_hash: Some(hash),
        })
    }
}
