use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct TestCreate2Task;

impl TestCreate2Task {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for TestCreate2Task {
    fn name(&self) -> &str {
        "56_testCreate2"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;

        let create2_address: Address = "0x13b0D85CcB8bf860b6b79AF3029fCA081AE9beF2"
            .parse()
            .context("Invalid Create2Deployer address")?;

        let code = provider.get_code(create2_address, None).await?;

        let mut messages = Vec::new();
        messages.push(format!("CREATE2 Deployer Analysis"));
        messages.push(format!("Address: {:?}", create2_address));
        messages.push(format!("Code length: {} bytes", code.len()));

        // Extract function selectors from bytecode (PUSH4 instructions followed by PUSH1 0x00 EQ)
        let selectors: Vec<String> = code
            .chunks(5)
            .filter(|chunk| chunk[0] == 0x63) // PUSH4
            .filter(|chunk| {
                // Look for PUSH4 followed by PUSH1(0x00) and EQ (0x14)
                chunk.len() >= 5 && chunk[4] == 0x14
            })
            .map(|chunk| {
                format!(
                    "0x{:02x}{:02x}{:02x}{:02x}",
                    chunk[1], chunk[2], chunk[3], chunk[4]
                )
            })
            .filter(|s| s != "0x00000000")
            .collect();

        messages.push(format!("\nFound {} unique selectors:", selectors.len()));
        for (i, selector) in selectors.iter().take(20).enumerate() {
            messages.push(format!("  {}. {}", i + 1, selector));
        }

        // Known function selectors to check
        let known_selectors = vec![
            ("0x4c1e5d23", "deploy(uint256,bytes32,bytes) - OpenZeppelin"),
            ("0x162e9498", "create2(bytes,bytes32)"),
            ("0x3c5b18e5", "deploy2(uint256,bytes)"),
            ("0x5c60da1b", "owner() / hasCode(address)"),
            ("0x8da5cb5b", "admin()"),
            ("0xf2fde38b", "owner()"),
            ("0x5a0b5d93", "computeAddress(bytes32,bytes32)"),
            ("0x677ba5bf", "computeAddress2(bytes32,bytes,address)"),
            ("0x094e2f5d", "initialize(address)"),
            ("0x4ce35d68", "getCreationCode(bytes)"),
            ("0x5b5c6f45", "initCodeHash(bytes32)"),
        ];

        messages.push(format!("\nKnown selector matches:"));
        for (selector, name) in &known_selectors {
            let found = selectors.iter().any(|s| s == selector);
            messages.push(format!(
                "  {} {} - {}",
                if found { "✓" } else { "✗" },
                selector,
                name
            ));
        }

        // Try to call some read methods to understand the contract state
        messages.push(format!("\nContract state:"));

        // Try owner()
        let owner_call = TransactionRequest::new()
            .to(create2_address)
            .data(hex::decode("f2fde38b").unwrap()); // owner() selector

        match provider.call(&owner_call.into(), None).await {
            Ok(result) => {
                if result.is_empty() || result == vec![0u8; 32] {
                    messages.push(format!("  owner() = 0x000... (zero address)"));
                } else {
                    let owner_addr = Address::from_slice(&result[12..32]);
                    messages.push(format!("  owner() = {:?}", owner_addr));
                }
            }
            Err(e) => messages.push(format!("  owner() call failed: {:?}", e)),
        }

        // Try to check if contract has any ETH
        let balance = provider.get_balance(create2_address, None).await?;
        let balance_eth =
            ethers::utils::format_units(balance, "ether").unwrap_or_else(|_| balance.to_string());
        messages.push(format!("  ETH balance: {} ETH", balance_eth));

        Ok(TaskResult {
            success: true,
            message: messages.join("\n"),
            tx_hash: None,
        })
    }
}
