use crate::task::{Task, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;

pub struct CheckBalanceTask;

#[async_trait]
impl Task<TaskContext> for CheckBalanceTask {
    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let address = ctx.wallet.address();
        let provider = ctx.provider.clone();

        // 1. Native ETH Balance
        let balance = provider.get_balance(address, None).await?;
        let balance_eth = ethers::utils::format_units(balance, "ether")?;
        let balance_f64: f64 = balance_eth.parse().unwrap_or(0.0);

        // 2. Token Balances
        let weth_addr: Address = "0x4200000000000000000000000000000000000006".parse()?;
        let wbtc_addr: Address = "0xF32D39ff9f6Aa7a7a64d7a4F00a54826Ef791a55".parse()?;
        let rise_addr: Address = "0xd6e1afe5cA8D00A2EFC01B89997abE2De47fdfAf".parse()?;

        // We can run these concurrently or sequentially. Sequential is fine for debug tool.
        // Note: Contract::new needs Arc<Provider>
        let client = std::sync::Arc::new(provider.clone());

        // Define simple ABI
        let erc20_abi = r#"[
            {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
            {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"}
        ]"#;
        let p_abi: ethers::abi::Abi = serde_json::from_str(erc20_abi)?;

        let mut balances_str = format!("ETH: {:.5}", balance_f64);

        let tokens = vec![
            ("WETH", weth_addr),
            ("WBTC", wbtc_addr),
            ("RISE", rise_addr),
        ];

        for (name, addr) in tokens {
            let contract = Contract::new(addr, p_abi.clone(), client.clone());
            let bal: U256 = contract
                .method("balanceOf", address)?
                .call()
                .await
                .unwrap_or_default();
            let dec: u8 = contract.method("decimals", ())?.call().await.unwrap_or(18);
            let raw_fmt = ethers::utils::format_units(bal, dec as u32).unwrap_or("0".into());
            let val_f64: f64 = raw_fmt.parse().unwrap_or(0.0);
            balances_str.push_str(&format!(" | {}: {:.5}", name, val_f64));
        }

        Ok(TaskResult {
            success: true,
            message: balances_str,
            tx_hash: None,
        })
    }

    fn name(&self) -> &str {
        "01_checkBalance"
    }
}
