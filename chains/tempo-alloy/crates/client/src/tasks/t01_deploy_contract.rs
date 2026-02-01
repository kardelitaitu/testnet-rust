//! Deploy Contract Task

use crate::tasks::{prelude::*, GasManager};
use alloy::primitives::{Bytes, U256};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::str::FromStr;

/// Deploy a simple Solidity contract
#[derive(Debug, Clone, Default)]
pub struct DeployContractTask;

impl DeployContractTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for DeployContractTask {
    fn name(&self) -> &'static str {
        "01_deploy_contract"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        // Simple counter bytecode (compiled from Counter.sol)
        // This is a minimal contract that stores a uint256 value
        let bytecode = Bytes::from_str(
            "0x608060405234801561001057600080fd5b5061012a806100206000396000f3fe6080604052348015600f57600080fd5b506004361060325760003560e01c8063368b8772146037578063d826f88a146068575b600080fd5b606660048036038101906062919060ba565b600055565b60005460749060d6565b60405180910390f35b600080fd5b609e8160eb565b811460a857600080fd5b50565b600081359050610bc565b600080fd5b600080fd5b6000601f19601f83011690549093919060d6565b6040519080825280601f01601f19166020018201604052801561010e57816000f55b50505056",
        )
        .map_err(|e| anyhow!("Invalid bytecode: {}", e))?;

        // Build deployment transaction
        let gas_price = ctx.gas_manager.estimate_gas(&ctx.client).await?;
        let bumped_gas_price = ctx.gas_manager.bump_fees(gas_price, 20);

        let tx = alloy::rpc::types::TransactionRequest::default()
            .with_input(bytecode)
            .with_max_fee_per_gas(bumped_gas_price)
            .from(ctx.address());

        // Send transaction
        let pending = ctx
            .client
            .provider
            .send_transaction(tx)
            .await
            .context("Failed to send deployment transaction")?;

        let tx_hash = *pending.tx_hash();

        // Log to database if available
        if let Some(db) = &ctx.db {
            db.log_counter_contract_creation(
                &ctx.address().to_string(),
                &format!("{:?}", tx_hash),
                ctx.chain_id(),
            )
            .await?;
        }

        Ok(TaskResult {
            success: true,
            message: format!("Contract deployed: {:?}", tx_hash),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
