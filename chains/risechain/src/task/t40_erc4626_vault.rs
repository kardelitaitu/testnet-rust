use crate::task::{Task, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct ERC4626VaultTask;

impl ERC4626VaultTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for ERC4626VaultTask {
    fn name(&self) -> &str {
        "40_erc4626Vault"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        const VAULT_ADDRESS: &str = "0x1d0188c4B27676D2C92F57c07D5dCF27A7e66bB7";
        const WETH: &str = "0x4200000000000000000000000000000000000006";

        let vault_address: Address = VAULT_ADDRESS.parse()?;
        let weth_address: Address = WETH.parse()?;

        let vault_code_len = provider.get_code(vault_address, None).await?.len();

        let weth_abi = r#"[
            {"type":"function","name":"balanceOf(address)","stateMutability":"view","inputs":[{"name":"","type":"address"}],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;
        let weth_abi_parsed: abi::Abi = serde_json::from_str(weth_abi)?;
        let weth = Contract::new(weth_address, weth_abi_parsed, Arc::new(provider.clone()));
        let weth_bal: U256 = weth
            .method("balanceOf", address)?
            .call()
            .await
            .unwrap_or_default();

        Ok(TaskResult {
            success: true,
            message: format!(
                "ERC4626 Check: Vault Code {} bytes | WETH Bal: {}",
                vault_code_len, weth_bal
            ),
            tx_hash: None,
        })
    }
}
