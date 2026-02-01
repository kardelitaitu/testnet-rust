use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct ReadOracleTask;

impl ReadOracleTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for ReadOracleTask {
    fn name(&self) -> &str {
        "17_readOracle"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;

        let oracles = [
            ("ETH", "0x7114E2537851e727678DE5a96C8eE5d0Ca14f03D"),
            ("USDC", "0x50524C5bDa18aE25C600a8b81449B9CeAeB50471"),
            ("USDT", "0x9190159b1bb78482Dca6EBaDf03ab744de0c0197"),
            ("BTC", "0xadDAEd879D549E5DBfaf3e35470C20D8C50fDed0"),
        ];

        let abi_json = r#"[
            {"type":"function","name":"latestAnswer","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"int256"}]},
            {"type":"function","name":"latest_answer","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"int256"}]},
            {"type":"function","name":"latestRoundData","stateMutability":"view","inputs":[],"outputs":[{"name":"roundId","type":"uint80"},{"name":"answer","type":"int256"},{"name":"startedAt","type":"uint256"},{"name":"updatedAt","type":"uint256"},{"name":"answeredInRound","type":"uint80"}]}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(abi_json)?;

        let mut results = Vec::new();

        for (name, address_str) in &oracles {
            let address: Address = address_str.parse().context("Invalid oracle address")?;
            let contract = Contract::new(address, abi.clone(), Arc::new(provider.clone()));

            let mut price: Option<I256> = None;
            let mut error_msg = "unknown";

            if let Ok(method) = contract.method::<_, I256>("latestAnswer", ()) {
                match method.call().await {
                    Ok(p) => {
                        price = Some(p);
                    }
                    Err(_) => {
                        error_msg = "latestAnswer call failed";
                    }
                }
            }

            if price.is_none() {
                if let Ok(method) = contract.method::<_, I256>("latest_answer", ()) {
                    match method.call().await {
                        Ok(p) => {
                            price = Some(p);
                        }
                        Err(_) => {
                            error_msg = "latest_answer call failed";
                        }
                    }
                }
            }

            if price.is_none() {
                if let Ok(method) =
                    contract.method::<_, (u64, I256, u64, u64, u64)>("latestRoundData", ())
                {
                    match method.call().await {
                        Ok((_, p, _, _, _)) => {
                            price = Some(p);
                        }
                        Err(_) => {
                            error_msg = "latestRoundData call failed";
                        }
                    }
                }
            }

            match price {
                Some(p) => {
                    let price_i128 = p.as_i128();
                    let formatted_price = ethers::utils::format_units(U256::from(price_i128), 8u32)
                        .unwrap_or_else(|_| p.to_string());
                    results.push(format!("{}: ${}", name, formatted_price));
                }
                None => {
                    results.push(format!("{}: ERROR ({})", name, error_msg));
                }
            }
        }

        let message = format!("Oracle prices: {}", results.join(", "));

        Ok(TaskResult {
            success: true,
            message,
            tx_hash: None,
        })
    }
}
