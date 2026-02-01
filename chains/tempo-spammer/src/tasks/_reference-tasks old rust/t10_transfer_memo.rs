use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_random_address;
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    ITIP20Memo,
    r#"[
        function transferWithMemo(address to, uint256 amount, bytes32 memo) returns (bool)
        function balanceOf(address owner) view returns (uint256)
        function decimals() view returns (uint8)
        function symbol() view returns (string)
    ]"#
);

pub struct TransferMemoTask;

#[async_trait]
impl TempoTask for TransferMemoTask {
    fn name(&self) -> &str {
        "10_transfer_memo"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        // 1. Choose Token (PathUSD or AlphaUSD)
        let token_addr = Address::from_str("0x20c0000000000000000000000000000000000000")?; // PathUSD

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let token = ITIP20Memo::new(token_addr, client.clone());

        let address = ctx.wallet.address();
        let balance = token.balance_of(address).call().await.unwrap_or_default();

        if balance.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: "No balance in PathUSD to perform transferWithMemo.".to_string(),
                tx_hash: None,
            });
        }

        let decimals = token.decimals().call().await.unwrap_or(18);
        let amount_base = rand::thread_rng().gen_range(10..50);
        let mut amount_wei = U256::from(amount_base) * U256::exp10(decimals as usize);

        if amount_wei > balance {
            amount_wei = balance / 2;
        }

        // 2. Generate Memo (32 bytes max)
        let memo_text = format!("TempoTask_{}", rand::thread_rng().gen_range(1000..9999));
        let mut memo_bytes = [0u8; 32];
        let bytes = memo_text.as_bytes();
        let len = bytes.len().min(32);
        memo_bytes[..len].copy_from_slice(&bytes[..len]);

        let recipient = get_random_address()?;
        let symbol = token
            .symbol()
            .call()
            .await
            .unwrap_or_else(|_| "???".to_string());

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        println!(
            "Transferring {} {} with memo '{}' to {:?}",
            amount_base, symbol, memo_text, recipient
        );

        let tx = token
            .transfer_with_memo(recipient, amount_wei, memo_bytes)
            .gas_price(bumped_gas_price);
        let pending = tx.send().await?;
        let receipt = pending.await?.context("TransferWithMemo failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!(
                "Sent {} {} with memo '{}'. Tx: {}",
                amount_base, symbol, memo_text, hash
            ),
            tx_hash: Some(hash),
        })
    }
}
