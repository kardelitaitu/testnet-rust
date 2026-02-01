use crate::tasks::{TaskContext, TempoTask};
use crate::utils::contract_compiler::ContractCompiler;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use core_logic::traits::TaskResult;
use ethers::prelude::*;
use ethers::types::Bytes;
use std::path::Path;
use std::str::FromStr;
use tracing::info;

use crate::utils::gas_manager::GasManager;

#[derive(Clone)]
pub struct DeployContractTask;

#[async_trait]
impl TempoTask for DeployContractTask {
    fn name(&self) -> &str {
        "01_deploy_contract"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        info!("Running Deploy Contract Task...");

        // 1. Compile Contract
        let contract_path = Path::new("chains/tempo/contracts/Counter.sol");
        let (_abi_str, bin_str) = ContractCompiler::compile(contract_path)
            .map_err(|e| anyhow!("Failed to compile contract: {}", e))?;

        info!("Contract compiled successfully (mocks used if compiler disabled).");

        // 2. Prepare Transaction
        let bytecode = Bytes::from_str(&bin_str)?;

        // Calculate Gas
        let current_gas = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas = GasManager::bump_fees(current_gas);
        info!("Gas Price: {} (Bumped: {})", current_gas, bumped_gas);

        // Manual deployment transaction
        let tx = TransactionRequest::new()
            .data(bytecode)
            // .to(None) // Implicit for creation
            .chain_id(ctx.config.chain_id)
            .gas_price(bumped_gas)
            .from(ctx.wallet.address());

        // Combine provider and wallet for signing
        let client = SignerMiddleware::new(
            (*ctx.provider).clone(),
            ctx.wallet.clone().with_chain_id(ctx.config.chain_id),
        );

        // 3. Deploy
        info!("Sending deployment transaction...");
        let pending_tx = client.send_transaction(tx, None).await?;
        let tx_hash = pending_tx.tx_hash();
        info!("Transaction submitted: {:?}", tx_hash);

        // NOTE: We do not wait for the receipt to speed up execution.
        // The task is considered successful if the transaction enters the mempool.

        Ok(TaskResult {
            success: true,
            message: format!("Tx: {:?}", tx_hash),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
