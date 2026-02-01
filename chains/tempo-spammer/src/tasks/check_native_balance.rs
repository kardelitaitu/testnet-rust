use crate::tasks::prelude::*;
use crate::tasks::tempo_tokens::TempoTokens;
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct CheckNativeBalanceTask;

#[async_trait]
impl TempoTask for CheckNativeBalanceTask {
    fn name(&self) -> &'static str {
        "check_native_balance"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        let native = client.provider.get_balance(address).await?;
        println!("Native Balance (eth_getBalance): {}", native);

        let system_tokens = TempoTokens::get_system_tokens();
        for token in system_tokens {
            let bal = TempoTokens::get_token_balance(client, token.address, address).await?;
            println!("Balance for {}: {} (raw: {:x})", token.symbol, bal, bal);
        }

        Ok(TaskResult {
            success: true,
            message: "Diagnostics complete".to_string(),
            tx_hash: None,
        })
    }
}
