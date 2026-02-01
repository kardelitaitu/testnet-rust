use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct RoleBasedAccessTask;

impl RoleBasedAccessTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for RoleBasedAccessTask {
    fn name(&self) -> &str {
        "34_roleBasedAccess"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let governance_address: Address = "0x4200000000000000000000000000000000000042"
            .parse()
            .context("Invalid GovernanceToken address")?;

        let mut messages = Vec::new();
        messages.push("Governance Token Analysis".to_string());
        messages.push("Token: 0x4200...0042".to_string());
        messages.push("".to_string());

        let code = provider.get_code(governance_address, None).await?;
        messages.push(format!("Contract code: {} bytes", code.len()));

        let abi_json = r#"[
            {"type":"function","name":"name()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"string"}]},
            {"type":"function","name":"symbol()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"string"}]},
            {"type":"function","name":"decimals()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint8"}]},
            {"type":"function","name":"totalSupply()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]},
            {"type":"function","name":"balanceOf(address)","stateMutability":"view","inputs":[{"name":"account","type":"address"}],"outputs":[{"name":"","type":"uint256"}]},
            {"type":"function","name":"allowance(address,address)","stateMutability":"view","inputs":[{"name":"owner","type":"address"},{"name":"spender","type":"address"}],"outputs":[{"name":"","type":"uint256"}]},
            {"type":"function","name":"transfer(address,uint256)","stateMutability":"nonpayable","inputs":[{"name":"to","type":"address"},{"name":"amount","type":"uint256"}],"outputs":[{"name":"","type":"bool"}]}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(abi_json)?;
        let contract = Contract::new(governance_address, abi, Arc::new(provider.clone()));

        match contract.method::<_, String>("name", ())?.call().await {
            Ok(name) => messages.push(format!("Name: {}", name)),
            Err(e) => messages.push(format!("name() error: {:?}", e)),
        }

        match contract.method::<_, String>("symbol", ())?.call().await {
            Ok(symbol) => messages.push(format!("Symbol: {}", symbol)),
            Err(e) => messages.push(format!("symbol() error: {:?}", e)),
        }

        match contract.method::<_, u8>("decimals", ())?.call().await {
            Ok(dec) => messages.push(format!("Decimals: {}", dec)),
            Err(e) => messages.push(format!("decimals() error: {:?}", e)),
        }

        match contract.method::<_, U256>("totalSupply", ())?.call().await {
            Ok(supply) => messages.push(format!("Total Supply: {:?}", supply)),
            Err(e) => messages.push(format!("totalSupply() error: {:?}", e)),
        }

        let balance: U256 = contract
            .method("balanceOf", address)?
            .call()
            .await
            .context("Failed to get balance")?;
        messages.push(format!("Your balance: {:?}", balance));

        let allowance: U256 = contract
            .method("allowance", (address, address))?
            .call()
            .await
            .context("Failed to get allowance")?;
        messages.push(format!("Your allowance for self: {:?}", allowance));

        messages.push("".to_string());
        messages.push("Note: Role-based access (hasRole, DEFAULT_ADMIN_ROLE)".to_string());
        messages.push("      not available on this governance token contract.".to_string());

        Ok(TaskResult {
            success: true,
            message: messages.join("\n"),
            tx_hash: None,
        })
    }
}
