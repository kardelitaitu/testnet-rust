use super::{DaChainTask, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::time::Duration;

pub struct SelfTransferTask;

#[async_trait]
impl DaChainTask for SelfTransferTask {
    fn name(&self) -> &str {
        "05_selfTransfer"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet_address = ctx.wallet.address();
        
        let nonce = ctx
            .provider
            .get_transaction_count(wallet_address, None)
            .await?;
        
        let gas_price = ctx.gas_manager.get_gas_price().await?;
        
        // Send minimal amount to self
        let tx = TransactionRequest::new()
            .to(wallet_address)
            .value(ethers::utils::parse_ether(0.00001)?)
            .nonce(nonce)
            .gas_price(gas_price)
            .gas(21000);
        
        // Connect wallet to provider using SignerMiddleware
        let middleware = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet);
        let pending_tx = middleware.send_transaction(tx, None).await?;
        let tx_hash = pending_tx.tx_hash();
        
        let receipt = pending_tx
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;
        
        Ok(TaskResult {
            success: receipt.is_some(),
            message: format!("Self-transfer complete, tx: {:?}", tx_hash),
        })
    }
}
