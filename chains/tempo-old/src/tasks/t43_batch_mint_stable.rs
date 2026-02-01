use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::address_reader::get_multiple_random_addresses;
use crate::utils::gas_manager::GasManager;
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;

ethers::contract::abigen!(
    IStableMintBatch,
    r#"[
        function mint(address to, uint256 amount)
    ]"#
);

pub struct BatchMintStableTask;

#[async_trait]
impl TempoTask for BatchMintStableTask {
    fn name(&self) -> &str {
        "43_batch_mint_stable"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let stable_addr = Address::from_str("0x20c0000000000000000000000000000000000000")?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);
        let stable = IStableMintBatch::new(stable_addr, client.clone());

        let count = rand::thread_rng().gen_range(3..7);
        let recipients = get_multiple_random_addresses(count)?;

        println!("Batch minting stable tokens to {} recipients...", count);

        let mut last_hash = String::new();
        for recipient in recipients {
            let amount = U256::from(1000);

            let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
            let bumped_gas_price = GasManager::bump_fees(gas_price);

            if let Ok(pending) = stable
                .mint(recipient, amount)
                .gas_price(bumped_gas_price)
                .send()
                .await
            {
                if let Ok(Some(receipt)) = pending.await {
                    last_hash = format!("{:?}", receipt.transaction_hash);
                }
            }
        }

        Ok(TaskResult {
            success: true,
            message: format!("Batch minted stable tokens to {} recipients.", count),
            tx_hash: if last_hash.is_empty() {
                None
            } else {
                Some(last_hash)
            },
        })
    }
}
