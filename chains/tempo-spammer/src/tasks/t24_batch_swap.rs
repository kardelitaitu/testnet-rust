//! Batch Swap Task
//!
//! Executes multiple swaps on the Stablecoin DEX.
//!
//! Workflow:
//! 1. Execute 2-3 swaps between PathUSD and AlphaUSD
//! 2. Quote before swapping (liquidity check)
//! 3. Approve if necessary
//! 4. Execute swap

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy_sol_types::{SolCall, sol};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use std::str::FromStr;

const DEX_ADDRESS: &str = "0xdec0000000000000000000000000000000000000";
const PATHUSD_ADDRESS: &str = "0x20c0000000000000000000000000000000000000";
const ALPHAUSD_ADDRESS: &str = "0x20c0000000000000000000000000000000000001";

sol!(
    interface IStablecoinDEX {
        function quoteSwapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn) view returns (uint128 amountOut);
        function swapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn, uint128 minAmountOut) returns (uint128 amountOut);
    }

    interface IERC20 {
        function approve(address spender, uint256 amount) returns (bool);
        function allowance(address owner, address spender) view returns (uint256);
    }
);

#[derive(Debug, Clone, Default)]
pub struct BatchSwapTask;

impl BatchSwapTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for BatchSwapTask {
    fn name(&self) -> &'static str {
        "24_batch_swap"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        let dex_addr = Address::from_str(DEX_ADDRESS).context("Invalid DEX")?;
        let pathusd_addr = Address::from_str(PATHUSD_ADDRESS).context("Invalid PathUSD")?;
        let alphausd_addr = Address::from_str(ALPHAUSD_ADDRESS).context("Invalid AlphaUSD")?;

        let mut rng = rand::rngs::OsRng;
        let count = rng.gen_range(3..=7);
        let amount_per_swap = U256::from(1000) * U256::from(10_u128.pow(18)); // 1000 tokens (assumed 18 decimals)

        tracing::debug!(
            "🚀 Optimistic Pipelining Swaps: Sending {} swaps between Path/Alpha...",
            count
        );

        // 1. Preparation: Get Nonce and check allowances once
        let mut current_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let start_nonce = current_nonce;

        let mut burst_txs = Vec::new();

        // Check/Add Approvals for both tokens
        for token_addr in [pathusd_addr, alphausd_addr] {
            let allowance_call = IERC20::allowanceCall {
                owner: address,
                spender: dex_addr,
            };
            let current_allowance = if let Ok(data) = client
                .provider
                .call(
                    TransactionRequest::default()
                        .to(token_addr)
                        .input(allowance_call.abi_encode().into()),
                )
                .await
            {
                IERC20::allowanceCall::abi_decode_returns(&data).unwrap_or(U256::ZERO)
            } else {
                U256::ZERO
            };

            if current_allowance < amount_per_swap * U256::from(count) {
                let approve_call = IERC20::approveCall {
                    spender: dex_addr,
                    amount: U256::MAX,
                };
                let approve_tx = TransactionRequest::default()
                    .to(token_addr)
                    .input(approve_call.abi_encode().into())
                    .from(address)
                    .nonce(current_nonce)
                    .gas_limit(100_000);
                burst_txs.push(approve_tx);
                current_nonce += 1;
            }
        }

        // 2. Prepare Swaps
        for i in 0..count {
            let (token_in, token_out) = if i % 2 == 0 {
                (pathusd_addr, alphausd_addr)
            } else {
                (alphausd_addr, pathusd_addr)
            };

            // Using 1:1 quote assumption with 10% slippage for speed
            let amount_in_u128 = u128::try_from(amount_per_swap).unwrap_or(0);
            let min_out = amount_in_u128 * 90 / 100;

            let swap_call = IStablecoinDEX::swapExactAmountInCall {
                tokenIn: token_in,
                tokenOut: token_out,
                amountIn: amount_in_u128,
                minAmountOut: min_out,
            };

            let swap_tx = TransactionRequest::default()
                .to(dex_addr)
                .input(swap_call.abi_encode().into())
                .from(address)
                .nonce(current_nonce)
                .gas_limit(500_000);

            burst_txs.push(swap_tx);
            current_nonce += 1;
        }

        // 3. Fire all transactions sequentially (Submission is fast, no need to wait for blocks)
        let tx_count = burst_txs.len();
        tracing::debug!(
            "Submitting {} Transactions (Nonces {}..{})",
            tx_count,
            start_nonce,
            current_nonce - 1
        );

        let mut last_submitted_nonce = start_nonce.wrapping_sub(1);
        let mut success_hashes = Vec::new();

        let mut first_error = None;

        for (idx, tx) in burst_txs.iter().enumerate() {
            let tx_nonce = start_nonce + idx as u64;
            match client.provider.send_transaction(tx.clone()).await {
                Ok(pending) => {
                    success_hashes.push(*pending.tx_hash());
                    last_submitted_nonce = tx_nonce;
                }
                Err(e) => {
                    tracing::error!("Failed to submit transaction at nonce {}: {}", tx_nonce, e);
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                    break; // CRITICAL: Stop on first failure
                }
            }
        }

        // 4. Update Nonce Manager with next nonce after last successful submission
        if let Some(manager) = &client.nonce_manager {
            let next_nonce = last_submitted_nonce.wrapping_add(1);
            manager.set(address, next_nonce).await;
        }

        if success_hashes.is_empty() {
            if let Some(err) = first_error {
                return Err(anyhow::anyhow!(err));
            }
            anyhow::bail!("Failed to submit any transactions in batch.");
        }

        Ok(TaskResult {
            success: true,
            message: format!(
                "Pipelined {}/{} transactions ({} swaps)",
                success_hashes.len(),
                tx_count,
                count
            ),
            tx_hash: Some(format!("{:?}", success_hashes.last().unwrap())),
        })
    }
}
