use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
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

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;

        // 1. Native balance check
        let balance = provider.get_balance(address, None).await?;
        let estimated_cost = gas_limit * gas_price;
        if balance < estimated_cost {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient TXENE for gas: need {} Wei, have {} Wei",
                    estimated_cost, balance
                ),
                tx_hash: None,
            });
        }

        // 2. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );
        let nonce = nonce_manager.next().await?;

        let multicall_abi_json = r#"[
            {"type":"function","name":"aggregate((address,bytes)[])","stateMutability":"payable","inputs":[{"name":"calls","type":"tuple[]","components":[{"name":"target","type":"address"},{"name":"callData","type":"bytes"}]}],"outputs":[{"name":"blockNumber","type":"uint256"},{"name":"returnData","type":"bytes[]"}]},
            {"type":"function","name":"getEthBalance","stateMutability":"view","inputs":[{"name":"addr","type":"address"}],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;

        let multicall_abi: abi::Abi = serde_json::from_str(multicall_abi_json)?;
        let multicall_contract =
            Contract::new(multicall_address, multicall_abi, Arc::new(provider.clone()));

        // 3. Build native-only calls: getEthBalance for wallet + random addresses
        let mut rng = OsRng;
        let num_random = rng.gen_range(1..=3);
        let mut calls = Vec::with_capacity(1 + num_random);

        // Call for own balance
        let self_data = multicall_contract.encode("getEthBalance", address)?;
        calls.push((multicall_address, self_data));

        // Calls for random addresses
        for _ in 0..num_random {
            let random_addr = AddressCache::get_random()?;
            let data = multicall_contract.encode("getEthBalance", random_addr)?;
            calls.push((multicall_address, data));
        }

        // 4. Encode aggregate call
        let call_count = calls.len();
        let data = multicall_contract.encode("aggregate", (calls,))?;

        let tx = TransactionRequest::new()
            .to(multicall_address)
            .data(data)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .from(address);

        // 5. Send (fire-and-forget)
        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!(
                    "Multicall aggregate submitted for {} getEthBalance calls",
                    call_count
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("Multicall tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit multicall tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
