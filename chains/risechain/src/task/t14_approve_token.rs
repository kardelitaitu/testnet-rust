use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

pub struct ApproveTokenTask;

impl ApproveTokenTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for ApproveTokenTask {
    fn name(&self) -> &str {
        "14_approveToken"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // Get random spender from address cache
        let spender = AddressCache::get_random().context("Failed to get random address")?;

        let amount = 1_000_000_000_000_000_000_000_000_000_000u128;
        let amount_formatted =
            ethers::utils::format_units(amount, 18u32).unwrap_or_else(|_| amount.to_string());

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;

        // 1. ETH Balance Check
        let balance = provider.get_balance(address, None).await?;
        let estimated_cost = gas_limit * max_fee;
        if balance < estimated_cost {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient ETH for gas: need {} Wei, have {} Wei",
                    estimated_cost, balance
                ),
                tx_hash: None,
            });
        }

        let abi_json = r#"[
            {"type":"function","name":"approve(address,uint256)","stateMutability":"nonpayable","inputs":[{"name":"spender","type":"address"},{"name":"amount","type":"uint256"}],"outputs":[{"name":"","type":"bool"}]},
            {"type":"function","name":"allowance(address,address)","stateMutability":"view","inputs":[{"name":"owner","type":"address"},{"name":"spender","type":"address"}],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(abi_json)?;
        let token_address: Address = "0x8a93d247134d91e0de6f96547cb0204e5be8e5d8"
            .parse()
            .context("Invalid token address")?;

        let contract = Contract::new(token_address, abi, Arc::new(provider.clone()));

        // 2. Encode and Send
        let data = contract.encode("approve", (spender, amount))?;

        let tx = Eip1559TransactionRequest::new()
            .to(token_address)
            .data(data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        use ethers::middleware::SignerMiddleware;
        let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.clone()));
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        let success = receipt.status == Some(U64::from(1));

        // 3. On-Chain Verification
        let mut final_message = format!("Approved {} tokens for {:?}", amount_formatted, spender);
        if success {
            let allowance: U256 = contract
                .method("allowance", (address, spender))?
                .call()
                .await
                .unwrap_or(U256::zero());

            debug!(
                "🔍 Verified on-chain: Allowance for {:?} is now {}",
                spender, allowance
            );
            final_message = format!(
                "Approve Success: Allowance for {:?} is now {}",
                spender, allowance
            );
        }

        Ok(TaskResult {
            success,
            message: final_message,
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
