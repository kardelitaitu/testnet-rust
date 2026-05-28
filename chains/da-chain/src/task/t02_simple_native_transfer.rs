use super::{DaChainTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::fs;

fn get_random_recipient() -> Result<Address> {
    let content = fs::read_to_string("chains/da-chain/address.txt").context("Failed to read address.txt")?;

    let addresses: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    if addresses.is_empty() {
        return Err(anyhow::anyhow!("No addresses found in address.txt"));
    }

    let mut rng = StdRng::from_entropy();
    let idx = rng.gen_range(0..addresses.len());
    let addr_str = addresses[idx].trim();

    Ok(addr_str.parse::<Address>()?)
}

pub struct SimpleNativeTransferTask;

#[async_trait]
impl DaChainTask for SimpleNativeTransferTask {
    fn name(&self) -> &str {
        "02_simpleNativeTransfer"
    }

    fn weight(&self) -> u32 {
        10
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        // Get random recipient
        let recipient = get_random_recipient()?;

        // Randomize amount: 0.050 to 0.100 DACC, 0.001 increment
        let mut rng = StdRng::from_entropy();
        let increments = rng.gen_range(50u64..=100u64); // 0.050 to 0.100
        let final_amount = U256::from(increments) * U256::from(10u64.pow(15)); // 0.001 DACC increments

        // Get automatic gas fees
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // Build and send legacy transaction — no manual nonce, no confirmation wait
        let middleware = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet);

        let amount_str = ethers::utils::format_ether(final_amount);

        let tx = TransactionRequest::new()
            .to(recipient)
            .value(final_amount)
            .gas_price(max_fee)
            .gas(21000);

        let pending_tx = middleware.send_transaction(tx, None).await?;
        let tx_hash = pending_tx.tx_hash();

        Ok(TaskResult {
            success: true,
            message: format!("Sent {} DACC to {:?} (tx: {:?})", amount_str, recipient, tx_hash),
        })
    }
}
