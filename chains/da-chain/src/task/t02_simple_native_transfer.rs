use super::{DaChainTask, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::time::Duration;

pub struct SimpleNativeTransferTask;

#[async_trait]
impl DaChainTask for SimpleNativeTransferTask {
    fn name(&self) -> &str {
        "02_simpleNativeTransfer"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet_address = ctx.wallet.address();
        
        // Get nonce
        let nonce = ctx
            .provider
            .get_transaction_count(wallet_address, None)
            .await?;
        
        // Get gas price
        let gas_price = ctx.gas_manager.get_gas_price().await?;
        
        // Send small amount of DACC
        let amount = ethers::utils::parse_ether(0.0001)?;
        
        let tx = TransactionRequest::new()
            .to(wallet_address)
            .value(amount)
            .nonce(nonce)
            .gas_price(gas_price)
            .gas(21000);
        
        // Connect wallet to provider using SignerMiddleware
        let middleware = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet);
        let pending_tx = middleware.send_transaction(tx, None).await?;
        let tx_hash = pending_tx.tx_hash();
        
        // Wait for confirmation
        let receipt = pending_tx
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;
        
        Ok(TaskResult {
            success: receipt.is_some(),
            message: format!("Transferred 0.0001 DACC, tx: {:?}", tx_hash),
        })
    }
}
