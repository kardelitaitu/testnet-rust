use crate::task::{Task, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;

pub struct XeneaCheckBalanceTask;

#[async_trait]
impl Task<TaskContext> for XeneaCheckBalanceTask {
    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let address = ctx.wallet.address();
        let provider = ctx.provider.clone();
        let balance = provider.get_balance(address, None).await?;
        let balance_native = ethers::utils::format_units(balance, "ether")?;
        let client = std::sync::Arc::new(provider.clone());

        let erc20_abi = r#"[
            {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
            {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"}
        ]"#;
        let p_abi: ethers::abi::Abi = serde_json::from_str(erc20_abi)?;

        let weth_addr: Address = "0x4200000000000000000000000000000000000006".parse()?;
        let wbtc_addr: Address = "0xF32D39ff9f6Aa7a7a64d7a4F00a54826Ef791a55".parse()?;
        let xenea_addr: Address = "0xd6e1afe5cA8D00A2EFC01B89997abE2De47fdfAf".parse()?;

        let tokens = vec![("WETH", weth_addr), ("WBTC", wbtc_addr), ("XENE", xenea_addr)];

        let mut token_line = String::new();
        for (idx, (name, addr)) in tokens.into_iter().enumerate() {
            let contract = Contract::new(addr, p_abi.clone(), client.clone());
            let bal: U256 = contract.method("balanceOf", address)?.call().await.unwrap_or_default();
            let dec: u8 = contract.method("decimals", ())?.call().await.unwrap_or(18);
            let raw_fmt = ethers::utils::format_units(bal, dec as u32).unwrap_or("0".into());
            let val_f64: f64 = raw_fmt.parse().unwrap_or(0.0);
            if idx > 0 {
                token_line.push_str(" | ");
            }
            token_line.push_str(&format!("{}: {:.5}", name, val_f64));
        }

        println!("Checked wallet address: {}", address);

        Ok(TaskResult {
            success: true,
            message: format!(
                "Checked wallet address: {} | TXENE: {}\n{}",
                address, balance_native, token_line
            ),
            tx_hash: None,
        })
    }

    fn name(&self) -> &str {
        "01_xeneaCheckBalance"
    }
}
