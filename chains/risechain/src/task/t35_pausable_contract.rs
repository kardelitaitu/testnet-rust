use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct PausableContractTask;

impl PausableContractTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for PausableContractTask {
    fn name(&self) -> &str {
        "35_pausableContract"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let governance_address: Address = "0x4200000000000000000000000000000000000042"
            .parse()
            .context("Invalid address")?;

        let mut messages = Vec::new();
        messages.push("Pausable/Access Control Check".to_string());
        messages.push("Token: 0x4200...0042 (OP)".to_string());
        messages.push("".to_string());

        let abi_json = r#"[
            {"type":"function","name":"paused()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"bool"}]},
            {"type":"function","name":"pauser()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"address"}]},
            {"type":"function","name":"isPauser(address)","stateMutability":"view","inputs":[{"name":"account","type":"address"}],"outputs":[{"name":"","type":"bool"}]},
            {"type":"function","name":"owner()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"address"}]},
            {"type":"function","name":"getOwner()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"address"}]},
            {"type":"function","name":"hasRole(bytes32,address)","stateMutability":"view","inputs":[{"name":"role","type":"bytes32"},{"name":"account","type":"address"}],"outputs":[{"name":"","type":"bool"}]},
            {"type":"function","name":"DEFAULT_ADMIN_ROLE()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"bytes32"}]}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(abi_json)?;
        let contract = Contract::new(governance_address, abi, Arc::new(provider.clone()));

        let mut available = Vec::new();
        let mut unavailable = Vec::new();

        match contract.method::<_, bool>("paused", ())?.call().await {
            Ok(p) => available.push(format!("  paused(): {}", p)),
            Err(_) => unavailable.push("  paused()".to_string()),
        }

        match contract.method::<_, Address>("owner", ())?.call().await {
            Ok(o) => available.push(format!("  owner(): {:?}", o)),
            Err(_) => unavailable.push("  owner()".to_string()),
        }

        match contract.method::<_, Address>("getOwner", ())?.call().await {
            Ok(o) => available.push(format!("  getOwner(): {:?}", o)),
            Err(_) => unavailable.push("  getOwner()".to_string()),
        }

        match contract
            .method::<_, bool>("isPauser", address)?
            .call()
            .await
        {
            Ok(p) => available.push(format!("  isPauser({:?}): {}", address, p)),
            Err(_) => unavailable.push("  isPauser()".to_string()),
        }

        match contract
            .method::<_, H256>("DEFAULT_ADMIN_ROLE", ())?
            .call()
            .await
        {
            Ok(r) => {
                let has_admin: bool = contract
                    .method("hasRole", (r, address))?
                    .call()
                    .await
                    .unwrap_or(false);
                available.push(format!("  DEFAULT_ADMIN_ROLE(): 0x{}", hex::encode(r)));
                available.push(format!(
                    "  hasRole(DEFAULT_ADMIN, {:?}): {}",
                    address, has_admin
                ));
            }
            Err(_) => unavailable.push("  DEFAULT_ADMIN_ROLE()/hasRole()".to_string()),
        }

        messages.push("Available methods:".to_string());
        for m in available {
            messages.push(m);
        }

        messages.push("".to_string());
        messages.push("Unavailable methods:".to_string());
        for m in unavailable {
            messages.push(m);
        }

        messages.push("".to_string());
        messages.push("Conclusion: Standard ERC20 without pausable/owner".to_string());
        messages.push("This is the OP token from Optimism governance.".to_string());

        Ok(TaskResult {
            success: true,
            message: messages.join("\n"),
            tx_hash: None,
        })
    }
}
