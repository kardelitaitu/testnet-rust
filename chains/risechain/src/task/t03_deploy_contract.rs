use crate::contracts::COUNTER_BYTECODE;
use crate::task::{Task, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;

pub struct DeployContractTask;

#[async_trait]
impl Task<TaskContext> for DeployContractTask {
    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet_addr = format!("{:?}", ctx.wallet.address());
        let chain_id = ctx.config.chain_id;

        // Create transaction with logic to deploy contract
        // Data = Bytecode
        let bytecode = ethers::utils::hex::decode(COUNTER_BYTECODE)?;
        // Check balance
        let balance = ctx.provider.get_balance(ctx.wallet.address(), None).await?;
        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;

        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;
        // gas_limit * max_fee
        let required = gas_limit * max_fee;
        if balance < required {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient funds: have {} wei, want {} wei",
                    balance, required
                ),
                tx_hash: None,
            });
        }

        let tx = Eip1559TransactionRequest::new()
            .from(ctx.wallet.address())
            .data(Bytes::from(bytecode))
            .gas(gas_limit) // Optimized gas limit for Counter
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee);

        // Sign and send
        use ethers::middleware::SignerMiddleware;
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await?;

        let receipt = pending_tx.await?;

        match receipt {
            Some(r) => {
                if let Some(contract_addr) = r.contract_address {
                    let addr_str = format!("{:?}", contract_addr);

                    // Log to DB
                    if let Some(db) = &ctx.db {
                        let _ = db
                            .log_counter_contract_creation(&wallet_addr, &addr_str, chain_id)
                            .await;
                    }

                    Ok(TaskResult {
                        success: true,
                        message: format!("Deployed Counter at {}", addr_str),
                        tx_hash: Some(format!("{:?}", r.transaction_hash)),
                    })
                } else {
                    Ok(TaskResult {
                        success: false,
                        message: "No contract address in receipt".into(),
                        tx_hash: Some(format!("{:?}", r.transaction_hash)),
                    })
                }
            }
            None => Ok(TaskResult {
                success: false,
                message: "Transaction dropped".into(),
                tx_hash: None,
            }),
        }
    }

    fn name(&self) -> &str {
        "03_deployContract"
    }
}
