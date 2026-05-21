use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use tokio::time::{timeout, Duration};

pub struct NonceRepairTask;

#[async_trait]
impl Task<TaskContext> for NonceRepairTask {
    fn name(&self) -> &str {
        "60_nonceRepair"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let confirmed_nonce = provider
            .get_transaction_count(address, Some(BlockId::Number(BlockNumber::Latest)))
            .await
            .context("Failed to get confirmed nonce")?;
        let pending_nonce = provider
            .get_transaction_count(address, Some(BlockId::Number(BlockNumber::Pending)))
            .await
            .context("Failed to get pending nonce")?;

        eprintln!("[t60] address={:?}", address);
        eprintln!(
            "[t60] confirmed_nonce={} pending_nonce={}",
            confirmed_nonce, pending_nonce
        );

        if pending_nonce <= confirmed_nonce {
            return Ok(TaskResult {
                success: true,
                message: "No pending transaction".into(),
                tx_hash: None,
            });
        }

        let mut gas_price = U256::from(2_000_000_000u64);
        let gas_limit = U256::from(21_000u64);
        let client = SignerMiddleware::new(provider.clone(), wallet.clone());

        let target_nonce = confirmed_nonce;
        eprintln!(
            "[t60] repairing target_nonce={} up to pending_nonce={}",
            target_nonce, pending_nonce
        );

        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let tx = TransactionRequest::new()
                .to(address)
                .value(U256::zero())
                .gas(gas_limit)
                .gas_price(gas_price)
                .nonce(target_nonce)
                .from(address);

            eprintln!(
                "[t60] attempt={} nonce={} gas_price={}",
                attempt, target_nonce, gas_price
            );
            let pending = match client.send_transaction(tx, None).await {
                Ok(pending) => pending,
                Err(e) => {
                    eprintln!(
                        "[t60] nonce={} submit failed; bumping gas and retrying: {}",
                        target_nonce, e
                    );
                    gas_price *= U256::from(2u64);
                    if attempt >= 6 {
                        return Err(e).context(format!(
                            "Repair submission failed for nonce {}",
                            target_nonce
                        ));
                    }
                    continue;
                }
            };
            let tx_hash = pending.tx_hash();

            match timeout(Duration::from_secs(20), pending).await {
                Ok(Ok(Some(receipt))) => {
                    eprintln!(
                        "[t60] nonce={} mined tx_hash={:?}",
                        target_nonce, receipt.transaction_hash
                    );
                    return Ok(TaskResult {
                        success: receipt.status == Some(U64::from(1)),
                        message: format!(
                            "Repaired nonce {} for {:?} with tx {:?}",
                            target_nonce, address, receipt.transaction_hash
                        ),
                        tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
                    });
                }
                Err(_) => {
                    eprintln!(
                        "[t60] nonce={} receipt wait timed out; bumping gas",
                        target_nonce
                    );
                    gas_price *= U256::from(2u64);
                    if attempt >= 6 {
                        return Ok(TaskResult {
                            success: false,
                            message: format!(
                                "Nonce {} still pending after {} attempts; last tx {:?}",
                                target_nonce, attempt, tx_hash
                            ),
                            tx_hash: Some(format!("{:?}", tx_hash)),
                        });
                    }
                }
                Ok(Ok(None)) => {
                    eprintln!(
                        "[t60] nonce={} submitted but receipt unavailable; bumping gas",
                        target_nonce
                    );
                    gas_price *= U256::from(2u64);
                    if attempt >= 6 {
                        return Ok(TaskResult {
                            success: false,
                            message: format!(
                                "Nonce {} submitted but still unavailable after {} attempts; last tx {:?}",
                                target_nonce, attempt, tx_hash
                            ),
                            tx_hash: Some(format!("{:?}", tx_hash)),
                        });
                    }
                }
                Ok(Err(e)) => {
                    return Err(e)
                        .context(format!("Repair receipt failed for nonce {}", target_nonce));
                }
            }
        }
    }
}
