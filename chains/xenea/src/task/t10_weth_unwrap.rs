use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct WethUnwrapTask;

impl WethUnwrapTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for WethUnwrapTask {
    fn name(&self) -> &str {
        "10_unwrapNative"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let weth_address: Address = "0x4200000000000000000000000000000000000006"
            .parse()
            .context("Invalid WETH address")?;

        let abi_json = r#"[
            {"type":"function","name":"deposit","stateMutability":"payable","inputs":[],"outputs":[]},
            {"type":"function","name":"withdraw","stateMutability":"nonpayable","inputs":[{"name":"wad","type":"uint256"}],"outputs":[]},
            {"type":"function","name":"balanceOf","stateMutability":"view","inputs":[{"name":"owner","type":"address"}],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(abi_json)?;
        let contract = Contract::new(weth_address, abi, Arc::new(provider.clone()));

        // Check WETH balance
        let balance: U256 = contract
            .method::<_, U256>("balanceOf", address)?
            .call()
            .await
            .context("Failed to get WETH balance")?;

        if balance.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: "No WETH to unwrap".to_string(),
                tx_hash: None,
            });
        }

        let amount_wei: U256 = balance * 70 / 100;
        let amount_eth = ethers::utils::format_units(amount_wei, "ether")
            .unwrap_or_else(|_| amount_wei.to_string());

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;

        let data = contract.encode("withdraw", amount_wei)?;

        let tx = TransactionRequest::new()
            .to(weth_address)
            .data(data)
            .gas(gas_limit)
            .gas_price(gas_price)
            
            .from(address);

        use ethers::middleware::SignerMiddleware;
        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)),
            message: format!("Unwrapped {} WETH to native at {:?}", amount_eth, weth_address),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}

