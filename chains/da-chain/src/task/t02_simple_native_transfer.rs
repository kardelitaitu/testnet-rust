use super::{DaChainTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::fs;
use std::time::Duration;

fn get_random_recipient() -> Result<Address> {
    let content =
        fs::read_to_string("chains/da-chain/address.txt").context("Failed to read address.txt")?;

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

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        println!("[DEBUG] Starting task 02_simpleNativeTransfer");
        let wallet_address = ctx.wallet.address();
        println!("[DEBUG] Wallet address: {:?}", wallet_address);

        // Get random recipient from address.txt
        println!("[DEBUG] Reading recipient from address.txt...");
        let recipient = get_random_recipient()?;
        println!("[DEBUG] Recipient: {:?}", recipient);

        // Get nonce
        println!("[DEBUG] Getting nonce...");
        let mut nonce = ctx
            .provider
            .get_transaction_count(wallet_address, None)
            .await?;
        println!("[DEBUG] Initial nonce (confirmed): {}", nonce);

        // Check for pending transactions with higher nonce
        let pending_nonce = ctx
            .provider
            .get_transaction_count(wallet_address, Some(BlockId::Number(BlockNumber::Pending)))
            .await?;
        println!("[DEBUG] Pending nonce: {}", pending_nonce);

        if pending_nonce > nonce {
            println!(
                "[WARNING] Pending transactions detected ({} pending)",
                pending_nonce - nonce
            );
            println!("[INFO] Waiting for pending transactions to clear...");
            // Wait for pending transactions
            tokio::time::sleep(Duration::from_secs(30)).await;
            let new_nonce = ctx
                .provider
                .get_transaction_count(wallet_address, None)
                .await?;
            println!("[DEBUG] Updated nonce after waiting: {}", new_nonce);
            // Use the new nonce
            nonce = new_nonce;
        }

        // Get wallet balance
        println!("[DEBUG] Getting balance...");
        let balance = ctx.provider.get_balance(wallet_address, None).await?;
        println!(
            "[DEBUG] Balance: {} DACC",
            ethers::utils::format_ether(balance)
        );

        if balance.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: "Balance is zero, skipping transfer".to_string(),
            });
        }

        // Calculate 0.5% - 1.0% of balance
        let mut rng = StdRng::from_entropy();
        let percentage: f64 = rng.gen_range(0.005..=0.01); // 0.5% to 1.0%
        println!("[DEBUG] Transfer percentage: {:.2}%", percentage * 100.0);
        let amount = balance.as_u128() as f64 * percentage;
        let amount_u256 = ethers::utils::parse_ether(format!("{:.18}", amount / 1e18))?;
        println!(
            "[DEBUG] Calculated amount: {} DACC",
            ethers::utils::format_ether(amount_u256)
        );

        // Ensure minimum amount (0.0001 DACC)
        let min_amount = ethers::utils::parse_ether("0.0001")?;
        let final_amount = if amount_u256 < min_amount {
            println!("[DEBUG] Using minimum amount");
            min_amount
        } else {
            amount_u256
        };
        println!(
            "[DEBUG] Final transfer amount: {} DACC",
            ethers::utils::format_ether(final_amount)
        );

        // Get automatic gas fees (EIP-1559 if supported, else legacy)
        println!("[DEBUG] Getting gas fees...");
        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        println!(
            "[DEBUG] Gas fees - max: {} wei, priority: {} wei",
            max_fee, priority_fee
        );

        // Check if balance is sufficient for amount + gas
        let gas_cost = U256::from(21000) * max_fee;
        if balance < final_amount + gas_cost {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient balance: have {} DACC, need {} DACC for transfer + gas",
                    ethers::utils::format_ether(balance),
                    ethers::utils::format_ether(final_amount + gas_cost)
                ),
            });
        }

        // Check if chain supports EIP-1559
        println!("[DEBUG] Checking EIP-1559 support...");
        let supports_eip1559 = ctx
            .provider
            .get_block(BlockNumber::Latest)
            .await?
            .and_then(|b| b.base_fee_per_gas)
            .is_some();
        println!("[DEBUG] EIP-1559 support: {}", supports_eip1559);

        let percentage_str = format!("{:.2}%", percentage * 100.0);

        if supports_eip1559 {
            // Use EIP-1559 transaction
            println!("[DEBUG] Creating EIP-1559 transaction...");
            let middleware = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet);
            let tx = Eip1559TransactionRequest::new()
                .to(recipient)
                .value(final_amount)
                .nonce(nonce)
                .gas(21000)
                .max_fee_per_gas(max_fee)
                .max_priority_fee_per_gas(priority_fee)
                .from(wallet_address);

            println!("[DEBUG] Sending EIP-1559 transaction...");
            let pending_tx = middleware.send_transaction(tx, None).await?;
            let tx_hash = pending_tx.tx_hash();
            println!("[DEBUG] Transaction sent: {:?}", tx_hash);

            let receipt = pending_tx
                .confirmations(1)
                .interval(Duration::from_millis(500))
                .await?;

            Ok(TaskResult {
                success: receipt.is_some(),
                message: format!(
                    "Transferred {} DACC ({} of balance), tx: {:?}",
                    ethers::utils::format_ether(final_amount),
                    percentage_str,
                    tx_hash
                ),
            })
        } else {
            // Use legacy transaction
            let middleware = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet);
            let tx = TransactionRequest::new()
                .to(recipient)
                .value(final_amount)
                .nonce(nonce)
                .gas_price(max_fee)
                .gas(21000);

            let pending_tx = middleware.send_transaction(tx, None).await?;
            let tx_hash = pending_tx.tx_hash();

            let receipt = pending_tx
                .confirmations(1)
                .interval(Duration::from_millis(500))
                .await?;

            Ok(TaskResult {
                success: receipt.is_some(),
                message: format!(
                    "Transferred {} DACC ({} of balance), tx: {:?}",
                    ethers::utils::format_ether(final_amount),
                    percentage_str,
                    tx_hash
                ),
            })
        }
    }
}
