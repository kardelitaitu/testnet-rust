//! Example: Parallel Transaction Sender using 2D Nonces
//!
//! This example demonstrates how to send multiple transactions in parallel
//! using Tempo's 2D nonce system without waiting for confirmations.
//!
//! Run with:
//! ```bash
//! cargo run --example parallel_send -- --rpc-url <RPC_URL> --private-key <KEY>
//! ```

use alloy::primitives::{Address, Bytes};
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use anyhow::{Context, Result};
use std::str::FromStr;
use std::sync::Arc;
use tokio;

// System token addresses on Tempo
const PATHUSD: &str = "0x20C0000000000000000000000000000000000000";
const ALPHAUSD: &str = "0x20c0000000000000000000000000000000000001";
const BETAUSD: &str = "0x20c0000000000000000000000000000000000002";
const THETAUSD: &str = "0x20c0000000000000000000000000000000000003";

/// Example 1: Simple parallel token transfers
async fn example_parallel_transfers<P: alloy::providers::Provider + Send + Sync>(
    provider: Arc<P>,
    signer: &PrivateKeySigner,
    chain_id: u64,
    recipient: Address,
    token_addr: Address,
    amount: u128,
    num_transfers: usize,
) -> Result<()> {
    use crate::utils::nonce_2d::ParallelSender;

    let sender = ParallelSender::new(provider);

    // Build transfer calldatas
    let mut calldatas = Vec::new();
    for _ in 0..num_transfers {
        let calldata = sender.manager.build_transfer_calldata(recipient, amount);
        calldatas.push(calldata);
    }

    // Send 5 transfers in parallel using nonce keys 1-5
    let hashes = sender
        .send_parallel(signer, chain_id, token_addr, calldatas, 1)
        .await?;

    println!("Sent {} transfers in parallel:", num_transfers);
    for (i, hash) in hashes.iter().enumerate() {
        println!("  Key {}: {}", i + 1, hash);
    }

    Ok(())
}

/// Example 2: Batch approve + swap workflow
async fn example_approve_swap<P: alloy::providers::Provider + Send + Sync>(
    provider: Arc<P>,
    signer: &PrivateKeySigner,
    chain_id: u64,
    token_in: Address,
    dex_addr: Address,
    amount: u128,
) -> Result<()> {
    use crate::utils::nonce_2d::ParallelSender;

    let sender = ParallelSender::new(provider);

    // Build approve calldata
    let approve_calldata = sender.manager.build_approve_calldata(dex_addr, amount);

    // Build swap calldata (simplified - use actual swap selector)
    let swap_selector: [u8; 4] = [0xf8, 0x85, 0x6c, 0x0f];
    let mut swap_calldata = Vec::new();
    swap_calldata.extend_from_slice(&swap_selector);
    // Add tokenIn, tokenOut, amountIn, minAmountOut parameters...
    let swap_calldata = Bytes::from(swap_calldata);

    // Authorize nonce key 1 for swap (key 0 for approve)
    sender.authorize_keys(signer, 1, 1).await?;

    // Send approve (key 0) and swap (key 1) in parallel
    let approve_tx = sender.manager.build_tx_with_nonce(
        token_in,
        approve_calldata,
        0, // protocol nonce
        chain_id,
        signer,
    );
    let swap_tx = sender
        .manager
        .build_tx_with_nonce(dex_addr, swap_calldata, 1, chain_id, signer);

    let (approve_result, swap_result) = tokio::join!(
        provider.send_transaction(approve_tx),
        provider.send_transaction(swap_tx)
    );

    match approve_result {
        Ok(p) => println!("Approve sent: {:?}", p.tx_hash()),
        Err(e) => println!("Approve failed: {:?}", e),
    }

    match swap_result {
        Ok(p) => println!("Swap sent: {:?}", p.tx_hash()),
        Err(e) => println!("Swap failed: {:?}", e),
    }

    Ok(())
}

/// Example 3: High-frequency trading pattern
async fn example_hft_pattern<P: alloy::providers::Provider + Send + Sync>(
    provider: Arc<P>,
    signer: &PrivateKeySigner,
    chain_id: u64,
    tokens: Vec<Address>,
    dex_addr: Address,
    amount_per_swap: u128,
    num_swaps: usize,
) -> Result<()> {
    use crate::utils::nonce_2d::ParallelSender;

    let sender = ParallelSender::new(provider);

    // Authorize enough nonce keys for all swaps
    let start_key = 1u64;
    sender
        .authorize_keys(signer, start_key, num_swaps as u64)
        .await?;

    // Build swap calldatas for different token pairs
    let mut calldatas = Vec::new();
    let mut from_token = tokens[0];
    for i in 0..num_swaps {
        let to_token = tokens[(i + 1) % tokens.len()];

        // Build swap calldata
        let mut calldata = Vec::new();
        let swap_selector: [u8; 4] = [0xf8, 0x85, 0x6c, 0x0f];
        calldata.extend_from_slice(&swap_selector);
        calldata.extend_from_slice(&[0u8; 12]); // padding
        calldata.extend_from_slice(from_token.as_slice());
        calldata.extend_from_slice(&[0u8; 12]); // padding
        calldata.extend_from_slice(to_token.as_slice());
        calldata.extend_from_slice(&U256::from(amount_per_swap).to_be_bytes::<32>());
        let min_out = amount_per_swap * 80 / 100; // 20% slippage
        calldata.extend_from_slice(&U256::from(min_out).to_be_bytes::<32>());

        calldatas.push(Bytes::from(calldata));
        from_token = to_token; // rotate tokens
    }

    // Send all swaps in parallel
    let hashes = sender
        .send_parallel(signer, chain_id, dex_addr, calldatas, start_key)
        .await?;

    println!("Executed {} swaps in parallel:", num_swaps);
    for (i, hash) in hashes.iter().enumerate() {
        println!("  Swap {}: {}", i + 1, hash);
    }

    Ok(())
}

/// Main entry point for examples
#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments (simplified)
    let rpc_url =
        std::env::var("RPC_URL").unwrap_or_else(|_| "https://rpc.moderato.tempo.xyz".to_string());
    let private_key =
        std::env::var("PRIVATE_KEY").unwrap_or_else(|_| panic!("Set PRIVATE_KEY env var"));

    let signer: PrivateKeySigner = private_key.parse().context("Invalid private key")?;
    let chain_id = signer.chain_id().unwrap_or(42431);

    let provider: Arc<_> = Arc::new(
        ProviderBuilder::new()
            .wallet(signer.clone())
            .connect(&rpc_url)
            .await
            .context("Failed to connect to RPC")?,
    );

    let recipient: Address = "0x6eacca11a74f3d0562aa7de02c4e7a397b73c636"
        .parse()
        .context("Invalid recipient")?;
    let token_addr: Address = PATHUSD.parse().context("Invalid token")?;

    // Example 1: Send 5 transfers in parallel
    example_parallel_transfers(
        provider.clone(),
        &signer,
        chain_id,
        recipient,
        token_addr,
        1_000_000, // 1M units
        5,
    )
    .await?;

    Ok(())
}
