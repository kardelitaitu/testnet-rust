use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_multiple_random_addresses;
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IMulticallStable,
    r#"[
        struct CallStable { address target; bytes callData; }
        function aggregate(CallStable[] calls) external payable returns (uint256 blockNumber, bytes[] returnData)
    ]"#
);

ethers::contract::abigen!(
    IERC20Stable,
    r#"[
        function approve(address spender, uint256 amount) returns (bool)
        function transferFrom(address from, address to, uint256 amount) returns (bool)
        function symbol() view returns (string)
    ]"#
);

pub struct MultiSendDisperseStableTask;

#[async_trait]
impl TempoTask for MultiSendDisperseStableTask {
    fn name(&self) -> &str {
        "29_multi_send_disperse_stable"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let multicall_addr = Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11")?;
        let token_addr = Address::from_str("0x20c0000000000000000000000000000000000000")?; // PathUSD

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let multicall = IMulticallStable::new(multicall_addr, client.clone());
        let token = IERC20Stable::new(token_addr, client.clone());

        let wallet_addr = ctx.wallet.address();

        let target_count = rand::thread_rng().gen_range(5..10);

        println!("Dispersing Stablecoins to {} recipients...", target_count);

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        // 1. Approve
        token
            .approve(multicall_addr, U256::max_value())
            .gas_price(bumped_gas_price)
            .send()
            .await?
            .await?;

        // 2. Prepare Calls
        let recipients = get_multiple_random_addresses(target_count)?;
        let mut calls = vec![];
        for recipient in recipients {
            let amount = U256::from(100);
            let data = token.encode("transferFrom", (wallet_addr, recipient, amount))?;
            calls.push(CallStable {
                target: token_addr,
                call_data: data,
            });
        }

        // 3. Execute
        let tx = multicall.aggregate(calls).gas_price(bumped_gas_price);
        let pending = tx.send().await?;
        let receipt = pending.await?.context("Multicall failed")?;

        Ok(TaskResult {
            success: true,
            message: format!("Dispersed stablecoins to {} recipients.", target_count),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
