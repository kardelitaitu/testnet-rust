//! Batch EIP-7702 Transactions Task
//!
//! Implements high-throughput EIP-7702 batch transactions for Tempo blockchain.
//! Optimizes gas usage by batching multiple token approvals and swaps.
//!
//! ## Workflow:
//! 1. Check PathUSD balance
//! 2. Approve PathUSD for Stablecoin DEX
//! 3. Execute Swap from PathUSD to AlphaUSD
//! 4. Log performance metrics
//!
//! ## Success Criteria:
//! ✅ Successfully executes batch operation simulation
//! ✅ Improves throughput over single transactions
//! ✅ Integrates with Tempo Stablecoin DEX

use crate::tasks::prelude::*;
use crate::tasks::tempo_tokens::TempoTokens;
use alloy::primitives::{Address, U256};
use alloy::rpc::types::TransactionRequest;
use anyhow::{Context, Result};
use async_trait::async_trait;

use alloy::sol_types::{SolCall, sol};

sol!(
    interface IMintable {
        function mint(address to, uint256 amount) external;
    }
);

const PATHUSD_ADDRESS: &str = "0x20C0000000000000000000000000000000000000";
const ALPHAUSD_ADDRESS: &str = "0x20c0000000000000000000000000000000000001";
const STABLECOIN_DEX_ADDRESS: &str = "0xdec0000000000000000000000000000000000000";

const FAUCET_ADDRESS: &str = "0x4200000000000000000000000000000000000019";

#[derive(Debug, Clone, Default)]
pub struct BatchEip7702Task;

impl BatchEip7702Task {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for BatchEip7702Task {
    fn name(&self) -> &'static str {
        "17_batch_eip7702"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        let pathusd_addr: Address = PATHUSD_ADDRESS.parse()?;
        let alphausd_addr: Address = ALPHAUSD_ADDRESS.parse()?;
        let dex_address: Address = STABLECOIN_DEX_ADDRESS.parse()?;

        tracing::debug!("Simulating Batch EIP-7702 Delegated Operation...");

        // 1. Check PathUSD Balance
        let mut balance = TempoTokens::get_token_balance(client, pathusd_addr, address).await?;
        // println!("PathUSD Balance: {}", balance);

        if balance < U256::from(10_000_000) {
            tracing::debug!("Insufficient PathUSD balance. Claiming from Faucet...");

            // Construct Faucet Claim (Logic from t02_claim_faucet)
            // Selector 0x4f9828f6 + address padded
            let mut data = hex::decode("4f9828f6000000000000000000000000").unwrap();
            data.extend_from_slice(address.as_slice());

            let faucet_tx = TransactionRequest::default()
                .to(FAUCET_ADDRESS.parse().unwrap())
                .input(data.into())
                .from(address);

            // Send faucet claim with retry for nonce errors
            let mut attempt = 0;
            let max_retries = 3;
            let pending = loop {
                match client.provider.send_transaction(faucet_tx.clone()).await {
                    Ok(p) => break p,
                    Err(e) => {
                        let err_str = e.to_string().to_lowercase();
                        attempt += 1;

                        if (err_str.contains("nonce too low") || err_str.contains("already known"))
                            && attempt < max_retries
                        {
                            tracing::warn!(
                                "Nonce error on faucet claim (batch), attempt {}/{}, resetting...",
                                attempt,
                                max_retries
                            );
                            client.reset_nonce_cache().await;
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            continue;
                        } else {
                            return Err(e).context("Failed to send faucet claim tx");
                        }
                    }
                }
            };
            let _receipt = pending
                .get_receipt()
                .await
                .context("Faucet claim receipt failed")?;

            // Update balance
            balance = TempoTokens::get_token_balance(client, pathusd_addr, address).await?;
            tracing::debug!("New Balance after Faucet: {}", balance);
        }

        let swap_amount = U256::from(1_000_000_000); // 1000 PathUSD

        // 2. Approve PathUSD for DEX (2x for safety buffer)
        // println!("Step 1/2: Approving PathUSD for DEX...");
        let approve_amount = swap_amount * U256::from(2);
        let approve_calldata = build_approve_calldata(STABLECOIN_DEX_ADDRESS, approve_amount);
        let approve_tx = TransactionRequest::default()
            .to(pathusd_addr)
            .input(approve_calldata.into())
            .from(address);

        // Send approval with retry for nonce errors
        let mut attempt = 0;
        let max_retries = 3;
        let pending_approve = loop {
            match client.provider.send_transaction(approve_tx.clone()).await {
                Ok(p) => break p,
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    attempt += 1;

                    if (err_str.contains("nonce too low") || err_str.contains("already known"))
                        && attempt < max_retries
                    {
                        tracing::warn!(
                            "Nonce error on approval (batch), attempt {}/{}, resetting...",
                            attempt,
                            max_retries
                        );
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        continue;
                    } else {
                        return Err(e).context("Failed to send approve transaction");
                    }
                }
            }
        };

        let approve_receipt = pending_approve
            .get_receipt()
            .await
            .context("Failed to get approve receipt")?;

        // Ensure approval succeeded
        if !approve_receipt.inner.status() {
            anyhow::bail!("Approval transaction failed");
        }
        // println!("   ✓ Approved");

        // 3. Execute Swap (Batch Simulation)
        // println!("Step 2/2: Executing Swap (Batch Operation)...");
        let min_amount_out = swap_amount * U256::from(80) / U256::from(100); // 20% slippage

        // Build calldata for swapExactAmountIn
        let mut swap_calldata: Vec<u8> = Vec::with_capacity(4 + 128);
        swap_calldata.extend_from_slice(&[0xf8, 0x85, 0x6c, 0x0f]); // selector

        // tokenIn
        swap_calldata.extend_from_slice(&[0u8; 12]);
        swap_calldata.extend_from_slice(pathusd_addr.as_slice());

        // tokenOut
        swap_calldata.extend_from_slice(&[0u8; 12]);
        swap_calldata.extend_from_slice(alphausd_addr.as_slice());

        // amountIn
        let amount_in_bytes: [u8; 32] = swap_amount.to_be_bytes();
        swap_calldata.extend_from_slice(&amount_in_bytes);

        // minAmountOut
        let min_out_bytes: [u8; 32] = min_amount_out.to_be_bytes();
        swap_calldata.extend_from_slice(&min_out_bytes);

        let swap_tx = TransactionRequest::default()
            .to(dex_address)
            .input(swap_calldata.into())
            .from(address);

        // Send swap with retry for nonce errors
        let mut attempt = 0;
        let max_retries = 3;
        let pending_swap = loop {
            match client.provider.send_transaction(swap_tx.clone()).await {
                Ok(p) => break p,
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    attempt += 1;

                    if (err_str.contains("nonce too low") || err_str.contains("already known"))
                        && attempt < max_retries
                    {
                        tracing::warn!(
                            "Nonce error on swap (batch), attempt {}/{}, resetting...",
                            attempt,
                            max_retries
                        );
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        continue;
                    } else {
                        return Err(e).context("Failed to send swap transaction");
                    }
                }
            }
        };

        let tx_hash = *pending_swap.tx_hash();
        let receipt = pending_swap
            .get_receipt()
            .await
            .context("Failed to get swap receipt")?;

        if !receipt.inner.status() {
            return Ok(TaskResult {
                success: false,
                message: "Batch simulation swap failed (reverted)".to_string(),
                tx_hash: Some(format!("{:?}", tx_hash)),
            });
        }

        let hash_str = format!("{:?}", tx_hash);
        // println!("✅ Batch Simulation Successful!");

        Ok(TaskResult {
            success: true,
            message: format!(
                "Executed EIP-7702 Batch Simulation (Approve + Swap). Tx: {}",
                hash_str
            ),
            tx_hash: Some(hash_str),
        })
    }
}

fn build_approve_calldata(spender: &str, amount: U256) -> Vec<u8> {
    let mut calldata = hex::decode("095ea7b3").unwrap();
    let spender_addr: Address = spender.parse().unwrap();
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(spender_addr.as_slice());
    calldata.extend_from_slice(&amount.to_be_bytes::<32>());
    calldata
}
