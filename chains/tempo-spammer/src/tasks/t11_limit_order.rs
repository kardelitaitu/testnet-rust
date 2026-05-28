//! Limit Order Task
//!
//! Places a limit order on the Stablecoin DEX.
//! Buys AlphaUSD with PathUSD (BID) or sells AlphaUSD for PathUSD (ASK).
//!
//! Workflow:
//! 1. Check PathUSD and AlphaUSD balances
//! 2. Randomly choose BUY or SELL
//! 3. If BUY: use 1% of PathUSD balance, token = AlphaUSD
//! 4. If SELL: use random 500-1000 AlphaUSD, token = AlphaUSD
//! 5. Place limit order with approval if needed

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use rand::prelude::SliceRandom;
use std::str::FromStr;

const DEX_ADDRESS: &str = "0xdec0000000000000000000000000000000000000";
const PATHUSD_ADDRESS: &str = "0x20c0000000000000000000000000000000000000";
const ALPHAUSD_ADDRESS: &str = "0x20c0000000000000000000000000000000000001";

const PLACE_SELECTOR: [u8; 4] = [0x63, 0x81, 0x31, 0x25];

const SYSTEM_TOKENS: &[(&str, &str)] = &[
    ("AlphaUSD", "0x20c0000000000000000000000000000000000001"),
    ("BetaUSD", "0x20c0000000000000000000000000000000000002"),
    ("ThetaUSD", "0x20c0000000000000000000000000000000000003"),
];

#[derive(Debug, Clone, Default)]
pub struct LimitOrderTask;

impl LimitOrderTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for LimitOrderTask {
    fn name(&self) -> &'static str {
        "11_limit_order"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        let dex_addr = Address::from_str(DEX_ADDRESS).context("Invalid DEX address")?;
        let pathusd_addr = Address::from_str(PATHUSD_ADDRESS).context("Invalid PathUSD address")?;

        let decimals = TempoTokens::get_token_decimals(client, pathusd_addr).await?;
        let pathusd_balance = TempoTokens::get_token_balance(client, pathusd_addr, address).await?;

        // Get a random system token (AlphaUSD, BetaUSD, or ThetaUSD)
        let (token_name, token_addr) = SYSTEM_TOKENS
            .choose(&mut rand::thread_rng())
            .map(|(n, a)| (*n, Address::from_str(a).unwrap()))
            .unwrap();

        let token_balance = TempoTokens::get_token_balance(client, token_addr, address).await?;

        let mut rng = rand::rngs::OsRng;
        let is_bid = rng.gen_bool(0.5);

        let amount_u128: u128;
        let token_symbol: &str;

        if is_bid {
            // BUY: use 1% of PathUSD balance to buy the system token
            if pathusd_balance < U256::from(1000) * U256::from(10_u64.pow(decimals as u32)) {
                return Ok(TaskResult {
                    success: false,
                    message: "Insufficient PathUSD balance for BUY order (need 1% balance)".to_string(),
                    tx_hash: None,
                });
            }
            let amount_wei = pathusd_balance / U256::from(100);
            amount_u128 = amount_wei.try_into().unwrap_or(0);
            token_symbol = token_name;
            tracing::debug!(
                "Placing Limit BUY order: {} PathUSD (1% of balance) for {} @ Tick -20",
                TempoTokens::format_amount(amount_wei, decimals),
                token_name
            );

            // Approve PathUSD for DEX
            let approve_calldata = build_approve_calldata(dex_addr, U256::MAX);

            // Send approval with retry logic (Simplified)
            let mut attempt = 0;
            loop {
                let nonce = match client.get_pending_nonce(&ctx.config.rpc_url).await {
                    Ok(n) => n,
                    Err(_) => break, // Skip approval if nonce fetch fails, try order anyway
                };

                let approve_tx = TransactionRequest::default()
                    .to(pathusd_addr)
                    .input(TransactionInput::from(approve_calldata.clone()))
                    .from(address)
                    .nonce(nonce);

                match client.provider.send_transaction(approve_tx).await {
                    Ok(_) => break, // Approval sent (fire and forget)
                    Err(e) => {
                        let err = e.to_string().to_lowercase();
                        if (err.contains("nonce") || err.contains("known")) && attempt < 2 {
                            client.reset_nonce_cache().await;
                            attempt += 1;
                            continue;
                        }
                        break;
                    },
                }
            }

            // println!("PathUSD approved for DEX");
        } else {
            // SELL: use random 500-1000 of the system token for PathUSD
            if token_balance < U256::from(500) * U256::from(10_u64.pow(decimals as u32)) {
                return Ok(TaskResult {
                    success: false,
                    message: format!(
                        "Insufficient {} balance. Need 500+, have {}",
                        token_name,
                        TempoTokens::format_amount(token_balance, decimals)
                    ),
                    tx_hash: None,
                });
            }
            let amount_base = rng.gen_range(500..1001);
            let amount_raw = U256::from(amount_base) * U256::from(10_u64.pow(decimals as u32));
            amount_u128 = amount_raw.try_into().unwrap_or(0);
            token_symbol = token_name;
            // println!(
            //     "Placing Limit SELL order: {} {} for PathUSD @ Tick +20",
            //     amount_base, token_name
            // );

            // Approve system token for DEX
            let approve_calldata = build_approve_calldata(dex_addr, U256::MAX);

            // Send approval with retry logic (Simplified)
            let mut attempt = 0;
            loop {
                let nonce = match client.get_pending_nonce(&ctx.config.rpc_url).await {
                    Ok(n) => n,
                    Err(_) => break,
                };

                let approve_tx = TransactionRequest::default()
                    .to(token_addr)
                    .input(TransactionInput::from(approve_calldata.clone()))
                    .from(address)
                    .nonce(nonce);

                match client.provider.send_transaction(approve_tx).await {
                    Ok(_) => break,
                    Err(e) => {
                        let err = e.to_string().to_lowercase();
                        if (err.contains("nonce") || err.contains("known")) && attempt < 2 {
                            client.reset_nonce_cache().await;
                            attempt += 1;
                            continue;
                        }
                        break;
                    },
                }
            }

            // println!("{} approved for DEX", token_name);
        }

        let tick: i16 = if is_bid { -20 } else { 20 };

        let place_calldata = build_place_calldata(token_addr, amount_u128, is_bid, tick);

        // Send place order with retry logic
        let mut attempt = 0;
        let max_retries = 3;

        loop {
            let nonce = match client.get_pending_nonce(&ctx.config.rpc_url).await {
                Ok(n) => n,
                Err(e) => {
                    return Ok(TaskResult {
                        success: false,
                        message: format!("Nonce error: {}", e),
                        tx_hash: None,
                    });
                },
            };

            let tx = TransactionRequest::default()
                .to(dex_addr)
                .input(TransactionInput::from(place_calldata.clone()))
                .from(address)
                .nonce(nonce)
                .max_fee_per_gas(150_000_000_000u128)
                .max_priority_fee_per_gas(1_500_000_000u128);

            match client.provider.send_transaction(tx).await {
                Ok(pending) => {
                    let tx_hash = *pending.tx_hash();
                    // Fire and forget
                    return Ok(TaskResult {
                        success: true,
                        message: format!(
                            "Placed Limit Order ({}): {} {} {} PathUSD @ Tick {}. Tx: {:?}",
                            if is_bid { "BUY" } else { "SELL" },
                            amount_u128 / 1_000_000,
                            token_symbol,
                            if is_bid { "<-" } else { "->" },
                            if is_bid { "-20" } else { "+20" },
                            tx_hash
                        ),
                        tx_hash: Some(format!("{:?}", tx_hash)),
                    });
                },
                Err(e) => {
                    let err = e.to_string().to_lowercase();
                    if (err.contains("nonce") || err.contains("known")) && attempt < max_retries {
                        client.reset_nonce_cache().await;
                        attempt += 1;
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        continue;
                    }

                    return Ok(TaskResult {
                        success: false,
                        message: format!("Limit order reverted: {}", e),
                        tx_hash: None,
                    });
                },
            }
        }
    }
}

fn build_approve_calldata(spender: Address, amount: U256) -> Vec<u8> {
    let mut calldata = Vec::new();
    calldata.extend_from_slice(&[0x09, 0x5e, 0xa7, 0xb3]);
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(spender.as_slice());
    calldata.extend_from_slice(&amount.to_be_bytes::<32>());
    calldata
}

fn build_place_calldata(token: Address, amount: u128, is_bid: bool, tick: i16) -> Vec<u8> {
    let mut calldata = Vec::new();
    calldata.extend_from_slice(&PLACE_SELECTOR);
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(token.as_slice());
    let amount_bytes: [u8; 16] = amount.to_be_bytes();
    calldata.extend_from_slice(&[0u8; 16]);
    calldata.extend_from_slice(&amount_bytes);
    let is_bid_byte: u8 = if is_bid { 1 } else { 0 };
    calldata.extend_from_slice(&[0u8; 31]);
    calldata.extend_from_slice(&[is_bid_byte]);
    let tick_bytes: [u8; 2] = tick.to_be_bytes();
    calldata.extend_from_slice(&[0u8; 30]);
    calldata.extend_from_slice(&tick_bytes);
    calldata
}
