use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use tracing::debug;

pub struct CheckCodeTask;

impl CheckCodeTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for CheckCodeTask {
    fn name(&self) -> &str {
        "check_code"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let addr: Address = "0x4200000000000000000000000000000000000017".parse()?;
        let code = provider.get_code(addr, None).await?;
        
        debug!("Code at {:?}: {} bytes", addr, code.len());
        
        Ok(TaskResult {
            success: true,
            message: format!("Code len: {}", code.len()),
            tx_hash: None,
        })
    }
}
