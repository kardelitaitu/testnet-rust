use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_random_address;
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IERC20Transfer,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function balanceOf(address owner) view returns (uint256)
        function decimals() view returns (uint8)
        function symbol() view returns (string)
    ]"#
);

pub struct TransferTokenTask;

#[async_trait]
impl TempoTask for TransferTokenTask {
    fn name(&self) -> &str {
        "09_transfer_token"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        // 1. Collect all possible tokens
        let mut possible_tokens = vec![
            Address::from_str("0x20c0000000000000000000000000000000000000")?, // PathUSD
            Address::from_str("0x20c0000000000000000000000000000000000001")?, // AlphaUSD
        ];

        // Add User Assets from DB
        if let Some(db) = ctx.db.as_ref() {
            if let Ok(assets) = db
                .get_assets_by_type(&format!("{:?}", ctx.wallet.address()), "stablecoin")
                .await
            {
                for addr_str in assets {
                    if let Ok(addr) = Address::from_str(&addr_str) {
                        possible_tokens.push(addr);
                    }
                }
            }
        }

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let address = ctx.wallet.address();

        // 2. Find a token with balance
        let mut target_token = None;

        let token_subset = {
            let mut rng = rand::thread_rng();
            let mut subset = vec![];
            for _ in 0..5 {
                let idx = rng.gen_range(0..possible_tokens.len());
                subset.push(possible_tokens[idx]);
            }
            subset
        };

        for addr in token_subset {
            let contract = IERC20Transfer::new(addr, client.clone());
            if let Ok(balance) = contract.balance_of(address).call().await {
                if !balance.is_zero() {
                    target_token = Some((contract, balance));
                    break;
                }
            }
        }

        let (token, balance) = match target_token {
            Some(t) => t,
            None => {
                // If checking random failed, try a guaranteed one (PathUSD)
                let addr = Address::from_str("0x20c0000000000000000000000000000000000000")?;
                let contract = IERC20Transfer::new(addr, client.clone());
                let bal = contract.balance_of(address).call().await?;
                if bal.is_zero() {
                    return Ok(TaskResult {
                        success: false,
                        message: "No tokens with positive balance found to transfer.".to_string(),
                        tx_hash: None,
                    });
                }
                (contract, bal)
            }
        };

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // 3. Transfer Random Amount
        let decimals = token.decimals().call().await.unwrap_or(18);
        let amount_base = {
            let mut rng = rand::thread_rng();
            rng.gen_range(10..50)
        };
        let mut amount_wei = U256::from(amount_base) * U256::exp10(decimals as usize);

        if amount_wei > balance {
            amount_wei = balance / 2;
        }

        if amount_wei.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: "Amount to transfer is zero.".to_string(),
                tx_hash: None,
            });
        }

        let recipient = get_random_address()?;
        let symbol = token
            .symbol()
            .call()
            .await
            .unwrap_or_else(|_| "???".to_string());
        println!("Transferring {} {} to {:?}", amount_base, symbol, recipient);

        let tx = token
            .transfer(recipient, amount_wei)
            .gas_price(bumped_gas_price);
        let pending = tx.send().await?;
        let receipt = pending.await?.context("Transfer failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!(
                "Transferred {} {} to {:?}. Tx: {}",
                amount_base, symbol, recipient, hash
            ),
            tx_hash: Some(hash),
        })
    }
}
