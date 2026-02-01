use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;

pub struct BlockHashUsageTask;

impl BlockHashUsageTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for BlockHashUsageTask {
    fn name(&self) -> &str {
        "55_blockHashUsage"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;

        let current_block = provider
            .get_block_number()
            .await
            .context("Failed to get current block")?;

        let target_block_num = if current_block.as_u64() > 256 {
            current_block.as_u64() - 256
        } else {
            current_block.as_u64()
        };

        let target_block = provider
            .get_block(target_block_num)
            .await
            .context("Failed to get target block")?;
        let block_hash = target_block.and_then(|b| b.hash);

        let latest_block = provider
            .get_block(BlockNumber::Latest)
            .await
            .context("Failed to get latest block")?;
        let parent_hash = if let Some(block) = latest_block {
            block.parent_hash
        } else {
            TxHash::zero()
        };

        let random_number = if !parent_hash.is_zero() {
            let random_bytes = &parent_hash.as_fixed_bytes()[0..8];
            u64::from_be_bytes([
                random_bytes[0],
                random_bytes[1],
                random_bytes[2],
                random_bytes[3],
                random_bytes[4],
                random_bytes[5],
                random_bytes[6],
                random_bytes[7],
            ])
        } else {
            0
        };

        Ok(TaskResult {
            success: true,
            message: format!(
                "Block Hash Usage: Current block: {}, Block {} hash available: {}, Random value from parent: {}",
                current_block, target_block_num, block_hash.is_some(), random_number
            ),
            tx_hash: None,
        })
    }
}
