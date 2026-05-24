use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

#[derive(Default)]
pub struct WethWrapTask;

impl WethWrapTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for WethWrapTask {
    fn name(&self) -> &str {
        "09_wrapNative"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let weth_address: Address = "0x4200000000000000000000000000000000000006"
            .parse()
            .context("Invalid WETH address")?;

        let balance = provider.get_balance(address, None).await?;
        let amount_wei = balance / 10; // 10%
        let amount_eth = ethers::utils::format_units(amount_wei, "ether")
            .unwrap_or_else(|_| amount_wei.to_string());

        let weth_code_size = provider.get_code(weth_address, None).await?.len();
        if weth_code_size == 0 {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "WETH contract does not exist at {:?}. Task 09 cannot run on this chain.",
                    weth_address
                ),
                tx_hash: None,
            });
        }

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;

        let abi_json = r#"[
            {"type":"function","name":"deposit","stateMutability":"payable","inputs":[],"outputs":[]},
            {"type":"function","name":"withdraw","stateMutability":"nonpayable","inputs":[{"name":"wad","type":"uint256"}],"outputs":[]}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(abi_json)?;
        let contract = Contract::new(weth_address, abi, Arc::new(provider.clone()));

        let data = contract.encode("deposit", ())?;

        let tx = TransactionRequest::new()
            .to(weth_address)
            .data(data)
            .value(amount_wei)
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
            message: format!("Wrapped {} native to WETH at {:?}", amount_eth, weth_address),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}

