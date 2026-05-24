use crate::task::{Task, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct UniswapV2SwapTask;

impl UniswapV2SwapTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for UniswapV2SwapTask {
    fn name(&self) -> &str {
        "39_uniswapV2Swap"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        const ROUTER_ADDR: &str = "0x4a2E7A3aF895509874DB31808a86d5871D6ec6fE";
        const WETH_ADDR: &str = "0x4200000000000000000000000000000000000006";

        let router_address: Address = ROUTER_ADDR.parse()?;
        let weth_address: Address = WETH_ADDR.parse()?;

        let gas_price = U256::from(1_100_000_000u64);
        let swap_gas = 500_000u64;
        let estimated_gas = U256::from(swap_gas) * gas_price;

        // 1. Balance check
        let balance = provider.get_balance(address, None).await?;
        if balance <= estimated_gas {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient TXENE for gas: need {} Wei, have {} Wei",
                    estimated_gas, balance
                ),
                tx_hash: None,
            });
        }

        // 2. Swap 1% of balance (after reserving gas)
        let available = balance - estimated_gas;
        let amount_in = available / U256::from(100u64);

        // 3. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );
        let nonce = nonce_manager.next().await?;

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let deadline = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
            + 1800;

        // 4. swapExactETHForTokens (fire-and-forget)
        let router_abi = r#"[
            {"inputs":[{"internalType":"uint256","name":"amountOutMin","type":"uint256"},{"internalType":"address[]","name":"path","type":"address[]"},{"internalType":"address","name":"to","type":"address"},{"internalType":"uint256","name":"deadline","type":"uint256"}],"name":"swapExactETHForTokens","outputs":[{"internalType":"uint256[]","name":"amounts","type":"uint256[]"}],"stateMutability":"payable","type":"function"}
        ]"#;
        let router_abi_parsed: abi::Abi = serde_json::from_str(router_abi)?;
        let router_contract = Contract::new(
            router_address,
            router_abi_parsed,
            Arc::new(provider.clone()),
        );

        let path = vec![weth_address];
        let swap_data = router_contract.encode(
            "swapExactETHForTokens",
            (U256::zero(), path, address, U256::from(deadline)),
        )?;

        let swap_tx = TransactionRequest::new()
            .to(router_address)
            .data(swap_data)
            .value(amount_in)
            .gas(swap_gas)
            .gas_price(gas_price)
            .nonce(nonce)
            .from(address);

        let pending_swap = client.send_transaction(swap_tx, None).await;

        match pending_swap {
            Ok(pending) => {
                let amount_eth = ethers::utils::format_units(amount_in, 18)
                    .unwrap_or_else(|_| amount_in.to_string());
                Ok(TaskResult {
                    success: true,
                    message: format!(
                        "UniswapV2 swap submitted: {} TXENE -> WETH (tx: {:?})",
                        amount_eth,
                        pending.tx_hash()
                    ),
                    tx_hash: Some(format!("{:?}", pending.tx_hash())),
                })
            }
            Err(e) => {
                debug!("UniswapV2 swap submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit UniswapV2 swap tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
