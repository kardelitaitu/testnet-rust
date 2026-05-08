use super::{DaChainTask, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::time::Duration;

pub struct DaChainDeployContractTask;

#[async_trait]
impl DaChainTask for DaChainDeployContractTask {
    fn name(&self) -> &str {
        "03_deployContract"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        // Simple storage contract bytecode
        let bytecode_hex = "608060405234801561001057600080fd5b50610150806100206000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c80633fb5c1cb14602d575b600080fd5b60336049565b6040518082815260200191505060405180910390f35b600060208284031215606057600080fd5b503591905056fea2646970667358221220f80b7d4dabe2d1be3a7b7e3132a8d2c62a339f75d6ad7a44e06e1e29f2f552164736f6c63430008120033";
        let bytecode = Bytes::from(hex::decode(bytecode_hex)?);
        
        let nonce = ctx
            .provider
            .get_transaction_count(ctx.wallet.address(), None)
            .await?;
        
        let gas_price = ctx.gas_manager.get_gas_price().await?;
        
        let tx = TransactionRequest::new()
            .data(bytecode)
            .nonce(nonce)
            .gas_price(gas_price)
            .gas(200000);
        
        // Connect wallet to provider using SignerMiddleware
        let middleware = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet);
        let pending_tx = middleware.send_transaction(tx, None).await?;
        let tx_hash = pending_tx.tx_hash();
        
        let receipt = pending_tx
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;
        
        let contract_address = receipt
            .as_ref()
            .and_then(|r| r.contract_address)
            .map(|a| format!("{:?}", a))
            .unwrap_or_else(|| "Unknown".to_string());
        
        Ok(TaskResult {
            success: receipt.is_some(),
            message: format!("Contract deployed to: {}, tx: {:?}", contract_address, tx_hash),
        })
    }
}
