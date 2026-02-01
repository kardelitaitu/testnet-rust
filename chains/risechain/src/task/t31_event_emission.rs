use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;

pub struct EventEmissionTask;

impl EventEmissionTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for EventEmissionTask {
    fn name(&self) -> &str {
        "31_eventEmission"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let mut rng = OsRng;
        let amount_wei: u64 = rng.gen_range(10_000_000_000_000u64..100_000_000_000_000u64);
        let amount_eth = ethers::utils::format_units(amount_wei, "ether")
            .unwrap_or_else(|_| amount_wei.to_string());

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_SEND_MEME;

        let weth_address: Address = "0x4200000000000000000000000000000000000006"
            .parse()
            .context("Invalid WETH address")?;

        let weth_abi_json = r#"[
            {"type":"function","name":"deposit()","stateMutability":"payable","inputs":[],"outputs":[]},
            {"type":"function","name":"withdraw(uint256)","stateMutability":"nonpayable","inputs":[{"name":"wad","type":"uint256"}],"outputs":[]},
            {"type":"event","name":"Deposit(address indexed,uint256)","inputs":[{"name":"dst","type":"address","indexed":true},{"name":"wad","type":"uint256"}],"anonymous":false},
            {"type":"event","name":"Withdrawal(address indexed,uint256)","inputs":[{"name":"src","type":"address","indexed":true},{"name":"wad","type":"uint256"}],"anonymous":false}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(weth_abi_json)?;
        let contract = Contract::new(weth_address, abi, Arc::new(provider.clone()));

        let data = contract.encode("deposit", ())?;

        let tx = Eip1559TransactionRequest::new()
            .to(weth_address)
            .data(data)
            .value(amount_wei)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let client = std::sync::Arc::new(SignerMiddleware::new(
            std::sync::Arc::new(provider.clone()),
            wallet.clone(),
        ));
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        let deposit_topic = ethers::utils::keccak256("Deposit(address,uint256)");

        let mut events_found = 0;
        let mut verified_events = 0;
        for log in &receipt.logs {
            if log.topics.len() >= 2 && log.topics[0] == ethers::types::TxHash(deposit_topic) {
                events_found += 1;
                // Verify indexed topic contains sender address
                if log.topics.len() >= 2 {
                    let event_sender = Address::from_slice(&log.topics[1].as_fixed_bytes()[12..]);
                    if event_sender == address {
                        verified_events += 1;
                    }
                }
            }
        }

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)),
            message: format!(
                "Deposited {} ETH. Events emitted: {} (verified sender match: {})",
                amount_eth, events_found, verified_events
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
