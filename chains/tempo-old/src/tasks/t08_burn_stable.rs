use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    ITIP20Burnable,
    r#"[
        function burn(uint256 amount)
        function balanceOf(address owner) view returns (uint256)
        function decimals() view returns (uint8)
        function symbol() view returns (string)
    ]"#
);

pub struct BurnStableTask;

#[async_trait]
impl TempoTask for BurnStableTask {
    fn name(&self) -> &str {
        "08_burn_stable"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        // 1. Get User Assets from DB
        let assets = if let Some(db) = ctx.db.as_ref() {
            db.get_assets_by_type(&format!("{:?}", ctx.wallet.address()), "stablecoin")
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };

        if assets.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No created stablecoins found in DB for this wallet to burn.".to_string(),
                tx_hash: None,
            });
        }

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let address = ctx.wallet.address();

        // 2. Find a token with balance
        let mut target_token = None;
        for addr_str in assets {
            let addr = Address::from_str(&addr_str)?;
            let contract = ITIP20Burnable::new(addr, client.clone());
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
                return Ok(TaskResult {
                    success: false,
                    message: "No tokens with positive balance found to burn.".to_string(),
                    tx_hash: None,
                });
            }
        };

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // 3. Burn Random Amount (1 - 5)
        let decimals = token.decimals().call().await.unwrap_or(18);

        let amount_base = {
            let mut rng = rand::thread_rng();
            rng.gen_range(1..5)
        };
        let mut amount_wei = U256::from(amount_base) * U256::exp10(decimals as usize);

        // Ensure we don't burn more than balance
        if amount_wei > balance {
            amount_wei = balance;
        }

        let symbol = token
            .symbol()
            .call()
            .await
            .unwrap_or_else(|_| "???".to_string());
        println!("Burning {} {} from {:?}", amount_base, symbol, address);

        let tx_burn = token.burn(amount_wei).gas_price(bumped_gas_price);
        let pending_burn = tx_burn.send().await?;
        let receipt = pending_burn.await?.context("Burn failed")?;

        let hash = format!("{:?}", receipt.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!(
                "Burned {} {} from {:?}. Tx: {}",
                amount_base, symbol, address, hash
            ),
            tx_hash: Some(hash),
        })
    }
}
