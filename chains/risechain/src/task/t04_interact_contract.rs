use crate::contracts::COUNTER_ABI;
use crate::task::{Task, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use std::sync::Arc;

pub struct InteractContractTask;

#[async_trait]
impl Task<TaskContext> for InteractContractTask {
    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet_addr = format!("{:?}", ctx.wallet.address());
        let chain_id = ctx.config.chain_id;

        // 1. Get contracts from DB
        let contracts = if let Some(db) = &ctx.db {
            db.get_deployed_counter_contracts(&wallet_addr, chain_id)
                .await?
        } else {
            return Ok(TaskResult {
                success: false,
                message: "DB not available".into(),
                tx_hash: None,
            });
        };

        if contracts.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No contracts found to interact with".into(),
                tx_hash: None,
            });
        }

        // 2. Pick random contract
        let contract_addr_str = {
            let mut rng = OsRng;
            contracts.choose(&mut rng).unwrap().clone()
        };

        let contract_addr = contract_addr_str.parse::<Address>()?;

        // 3. Interact (increment)
        let client = Arc::new(SignerMiddleware::new(
            ctx.provider.clone(),
            ctx.wallet.clone(),
        ));
        let abi: abi::Abi = serde_json::from_str(COUNTER_ABI)?;
        let contract = Contract::new(contract_addr, abi, client);

        // Check balance
        let balance = ctx.provider.get_balance(ctx.wallet.address(), None).await?;
        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;

        // Use Specific Limit for Counter
        let gas_limit = crate::utils::gas::GasManager::LIMIT_COUNTER_INTERACT;
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

        // Call increment
        // Call increment using Eip1559TransactionRequest
        let data = contract.encode("increment", ())?;

        let tx = Eip1559TransactionRequest::new()
            .to(contract_addr)
            .data(data)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee);

        // Use client to send
        let client_signer = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let pending_tx = client_signer.send_transaction(tx, None).await?;
        let receipt = pending_tx.await?;

        match receipt {
            Some(r) => Ok(TaskResult {
                success: r.status == Some(U64::from(1)),
                message: format!("Called increment() on {}", contract_addr_str),
                tx_hash: Some(format!("{:?}", r.transaction_hash)),
            }),
            None => Ok(TaskResult {
                success: false,
                message: "Transaction dropped".into(),
                tx_hash: None,
            }),
        }
    }

    fn name(&self) -> &str {
        "04_interactContract"
    }
}
