use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

pub struct MulticallTask;

impl MulticallTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for MulticallTask {
    fn name(&self) -> &str {
        "16_multicall"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let multicall_address: Address = "0xcA11bde05977b3631167028862bE2a173976CA11"
            .parse()
            .context("Invalid Multicall address")?;

        let usdc_address: Address = "0x8a93d247134d91e0de6f96547cb0204e5be8e5d8"
            .parse()
            .context("Invalid USDC address")?;

        let weth_address: Address = "0x4200000000000000000000000000000000000006"
            .parse()
            .context("Invalid WETH address")?;

        let (max_fee, _) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;

        let multicall_abi_json = r#"[
            {"type":"function","name":"aggregate((address,bytes)[])","stateMutability":"payable","inputs":[{"name":"calls","type":"tuple[]","components":[{"name":"target","type":"address"},{"name":"callData","type":"bytes"}]}],"outputs":[{"name":"blockNumber","type":"uint256"},{"name":"returnData","type":"bytes[]"}]},
            {"type":"function","name":"getBlockNumber","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]},
            {"type":"function","name":"getEthBalance","stateMutability":"view","inputs":[{"name":"addr","type":"address"}],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;

        let erc20_abi_json = r#"[
            {"type":"function","name":"balanceOf(address)","stateMutability":"view","inputs":[{"name":"account","type":"address"}],"outputs":[{"name":"","type":"uint256"}]},
            {"type":"function","name":"symbol()","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"string"}]}
        ]"#;

        let multicall_abi: abi::Abi = serde_json::from_str(multicall_abi_json)?;
        let erc20_abi: abi::Abi = serde_json::from_str(erc20_abi_json)?;

        let multicall_contract =
            Contract::new(multicall_address, multicall_abi, Arc::new(provider.clone()));
        let usdc_contract =
            Contract::new(usdc_address, erc20_abi.clone(), Arc::new(provider.clone()));
        let weth_contract = Contract::new(weth_address, erc20_abi, Arc::new(provider.clone()));

        let usdc_data = usdc_contract.encode("balanceOf", address)?;
        let weth_data = weth_contract.encode("balanceOf", address)?;
        let eth_balance_data = multicall_contract.encode("getEthBalance", address)?;

        // Individual call structures
        let calls = vec![
            (usdc_address, usdc_data.clone()),
            (weth_address, weth_data.clone()),
            (multicall_address, eth_balance_data.clone()),
        ];

        // Correct aggregate call encoding
        let data = multicall_contract.encode("aggregate", (calls,))?;

        let tx = Eip1559TransactionRequest::new()
            .to(multicall_address)
            .data(data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(max_fee)
            .from(address);

        use ethers::middleware::SignerMiddleware;
        let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.clone()));
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        let success = receipt.status == Some(U64::from(1));

        if success {
            debug!("✅ Multicall transaction successful!");
        }

        Ok(TaskResult {
            success,
            message: format!(
                "Multicall aggregation executed successfully for {:?}",
                address
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
