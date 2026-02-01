//! Add Liquidity Task
//!
//! Places limit orders on the Tempo Stablecoin DEX using native system tokens.
//! DEX: 0xdec0000000000000000000000000000000000000
//!
//! Based on successful tx: 0xd8eb5a47e8c2d5ef51e1b9f5842cd41861f1381637b0f58545ee290e274b0c56

use crate::TempoClient;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use rand::prelude::SliceRandom;
use std::str::FromStr;
use std::sync::Arc;

const STABLECOIN_DEX_ADDRESS: &str = "0xdec0000000000000000000000000000000000000";
const PATHUSD_ADDRESS: &str = "0x20C0000000000000000000000000000000000000";
const FAUCET_ADDRESS: &str = "0x4200000000000000000000000000000000000019";

const SYSTEM_TOKENS: &[(&str, &str)] = &[
    ("AlphaUSD", "0x20c0000000000000000000000000000000000001"),
    ("BetaUSD", "0x20c0000000000000000000000000000000000002"),
    ("ThetaUSD", "0x20c0000000000000000000000000000000000003"),
];

#[derive(Debug, Clone, Default)]
pub struct AddLiquidityTask;

impl AddLiquidityTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for AddLiquidityTask {
    fn name(&self) -> &'static str {
        "06_add_liquidity"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let wallet_addr_str = address.to_string();

        let dex_address =
            Address::from_str(STABLECOIN_DEX_ADDRESS).context("Invalid DEX address")?;
        let pathusd_address = Address::from_str(PATHUSD_ADDRESS).context("Invalid PathUSD")?;

        let mut tokens_with_balance: Vec<(String, Address, U256)> = Vec::new();

        for (name, addr) in SYSTEM_TOKENS {
            if let Ok(token_addr) = Address::from_str(addr) {
                let balance = get_token_balance(client, token_addr, address).await?;
                if balance > U256::ZERO {
                    tokens_with_balance.push((name.to_string(), token_addr, balance));
                    // println!("{} balance: {}", name, format_token_amount_u256(balance));
                }
            }
        }

        if tokens_with_balance.is_empty() {
            // Auto-claim from faucet when no system tokens available
            tracing::info!("No system tokens found, claiming from faucet...");

            let faucet_addr =
                Address::from_str(FAUCET_ADDRESS).context("Invalid faucet address")?;
            let mut faucet_data = hex::decode("4f9828f6000000000000000000000000").unwrap();
            faucet_data.extend_from_slice(address.as_slice());

            let faucet_tx = TransactionRequest::default()
                .to(faucet_addr)
                .input(faucet_data.into())
                .from(address);

            match client.provider.send_transaction(faucet_tx).await {
                Ok(pending) => {
                    let tx_hash = *pending.tx_hash();
                    // Wait a moment for tokens to be available
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                    // Re-check balances after claiming
                    for (name, addr) in SYSTEM_TOKENS {
                        if let Ok(token_addr) = Address::from_str(addr) {
                            let balance = get_token_balance(client, token_addr, address).await?;
                            if balance > U256::ZERO {
                                tokens_with_balance.push((name.to_string(), token_addr, balance));
                            }
                        }
                    }

                    if tokens_with_balance.is_empty() {
                        return Ok(TaskResult {
                            success: false,
                            message:
                                "Faucet claimed but tokens not yet available. Try again later."
                                    .to_string(),
                            tx_hash: Some(format!("{:?}", tx_hash)),
                        });
                    }

                    tracing::info!("Faucet claim successful, proceeding with liquidity add");
                }
                Err(e) => {
                    return Ok(TaskResult {
                        success: false,
                        message: format!("No system tokens and faucet claim failed: {:?}", e),
                        tx_hash: None,
                    });
                }
            }
        }

        let (token_name, base_token, wallet_balance) = tokens_with_balance
            .choose(&mut rand::thread_rng())
            .map(|(n, a, b)| (n.clone(), *a, *b))
            .context("Failed to select token with balance")?;

        let base_token_str = base_token.to_string();
        let pathusd_str = pathusd_address.to_string();

        // println!("Selected {} as base token", token_name);

        let pathusd_balance = get_token_balance(client, pathusd_address, address).await?;
        // println!(
        //     "PathUSD wallet balance: {}",
        //     format_token_amount_u256(pathusd_balance)
        // );

        let dex_pathusd_balance =
            get_dex_balance(client, dex_address, pathusd_address, address).await?;
        // println!(
        //     "PathUSD DEX balance: {}",
        //     format_token_amount_u256(dex_pathusd_balance)
        // );

        let total_pathusd = pathusd_balance + dex_pathusd_balance;
        if total_pathusd < U256::from(1_000_000u64) {
            return Ok(TaskResult {
                success: false,
                message: "Insufficient PathUSD for order. Get from faucet first.".to_string(),
                tx_hash: None,
            });
        }

        let order_amount: U256 = total_pathusd / U256::from(4);
        let order_amount_u128: u128 = order_amount.try_into().unwrap_or(0);
        let order_amount_str = format!("{}", order_amount);

        if order_amount_u128 == 0 {
            return Ok(TaskResult {
                success: false,
                message: "Balance too small".to_string(),
                tx_hash: None,
            });
        }

        // println!(
        //     "Placing limit BUY order: {} {} at Tick 0",
        //     token_name,
        //     format_token_amount(order_amount_u128)
        // );

        let tick: i16 = 0;
        let is_bid: bool = true;

        let order_calldata = build_place_calldata(base_token, order_amount_u128, is_bid, tick);

        // println!("Approving PathUSD for DEX (for BUY order)...");
        approve_token(client, pathusd_address, dex_address, order_amount, address).await?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let next_order_id_before = get_next_order_id(client, dex_address).await?;
        // println!("nextOrderId before placement: {}", next_order_id_before);

        // Build and send order transaction with retry logic
        let mut attempt = 0;
        let max_retries = 3;
        let mut last_error = None;

        let (tx_hash, tx_hash_str, receipt) = loop {
            let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;

            let tx = TransactionRequest::default()
                .to(dex_address)
                .input(TransactionInput::from(order_calldata.clone()))
                .from(address)
                .nonce(nonce);

            match client.provider.send_transaction(tx).await {
                Ok(pending) => {
                    let tx_hash = *pending.tx_hash();
                    let tx_hash_str = format!("{:?}", tx_hash);

                    let receipt = pending
                        .get_receipt()
                        .await
                        .context("Failed to get receipt")?;

                    break (tx_hash, tx_hash_str, receipt);
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    attempt += 1;

                    if (err_str.contains("nonce too low") || err_str.contains("already known"))
                        && attempt < max_retries
                    {
                        tracing::warn!(
                            "Nonce error on order placement, attempt {}/{}, resetting cache...",
                            attempt,
                            max_retries
                        );
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        last_error = Some(e);
                        continue;
                    } else {
                        // Non-nonce error or max retries exceeded
                        return Ok(TaskResult {
                            success: false,
                            message: format!("Order failed: {:?}", e),
                            tx_hash: None,
                        });
                    }
                }
            }
        };

        if !receipt.inner.status() {
            return Ok(TaskResult {
                success: false,
                message: "Place order reverted".to_string(),
                tx_hash: Some(tx_hash_str),
            });
        }

        let next_order_id_after = get_next_order_id(client, dex_address).await?;
        let dex_order_id = extract_order_id_from_receipt(&receipt);
        let final_order_id = dex_order_id.unwrap_or(0);

        if let Some(db) = &ctx.db {
            if let Err(e) = db
                .log_dex_order(
                    &wallet_addr_str,
                    &final_order_id.to_string(),
                    &base_token_str,
                    &pathusd_str,
                    &order_amount_str,
                    is_bid,
                    tick,
                    &tx_hash_str,
                )
                .await
            {
                // println!("⚠️ Failed to log order to database: {:?}", e);
            }
        }

        Ok(TaskResult {
            success: true,
            message: format!(
                "Placed BUY limit order: {} {} <- PathUSD at Tick 0. Tx: {:?}",
                token_name,
                format_token_amount(order_amount_u128),
                tx_hash
            ),
            tx_hash: Some(tx_hash_str),
        })
    }
}

async fn get_token_balance(
    client: &crate::TempoClient,
    token: Address,
    wallet: Address,
) -> Result<U256> {
    let mut calldata = Vec::new();
    calldata.extend_from_slice(&[0x70, 0xa0, 0x82, 0x31]);
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(wallet.as_slice());

    let balance_data = TransactionRequest::default()
        .to(token)
        .input(calldata.into());

    if let Ok(data) = client.provider.call(balance_data).await {
        let bytes = data.as_ref();
        if !bytes.is_empty() {
            return Ok(U256::from_be_slice(bytes));
        }
    }
    Ok(U256::ZERO)
}

async fn get_dex_balance(
    client: &crate::TempoClient,
    dex: Address,
    token: Address,
    user: Address,
) -> Result<U256> {
    let mut calldata = Vec::new();
    calldata.extend_from_slice(&[0x4f, 0x83, 0x29, 0x24]);
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(user.as_slice());
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(token.as_slice());

    let balance_data = TransactionRequest::default().to(dex).input(calldata.into());

    if let Ok(data) = client.provider.call(balance_data).await {
        let bytes = data.as_ref();
        if !bytes.is_empty() {
            return Ok(U256::from_be_slice(bytes));
        }
    }
    Ok(U256::ZERO)
}

async fn approve_token(
    client: &crate::TempoClient,
    token: Address,
    spender: Address,
    amount: U256,
    wallet: Address,
) -> Result<()> {
    let mut calldata = Vec::new();
    calldata.extend_from_slice(&[0x09, 0x5e, 0xa7, 0xb3]);
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(spender.as_slice());
    calldata.extend_from_slice(&U256::MAX.to_be_bytes::<32>());

    let tx = TransactionRequest::default()
        .to(token)
        .input(calldata.into())
        .from(wallet);

    // Send with retry logic for nonce errors
    let mut attempt = 0;
    let max_retries = 3;
    let pending = loop {
        match client.provider.send_transaction(tx.clone()).await {
            Ok(p) => break p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                attempt += 1;

                if (err_str.contains("nonce too low") || err_str.contains("already known"))
                    && attempt < max_retries
                {
                    tracing::warn!(
                        "Nonce error on approval (add_liquidity), attempt {}/{}, resetting...",
                        attempt,
                        max_retries
                    );
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    continue;
                } else {
                    return Err(e).context("Failed to send approve");
                }
            }
        }
    };

    let _receipt = pending
        .get_receipt()
        .await
        .context("Failed to get receipt")?;

    // println!("Approval confirmed");
    Ok(())
}

fn build_place_calldata(token: Address, amount: u128, is_bid: bool, tick: i16) -> Vec<u8> {
    let mut calldata = Vec::with_capacity(4 + 128);
    calldata.extend_from_slice(&[0x63, 0x81, 0x31, 0x25]);
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

async fn get_next_order_id(client: &crate::TempoClient, dex: Address) -> Result<u128> {
    let mut calldata = Vec::new();
    calldata.extend_from_slice(&[0x7c, 0x97, 0x78, 0x80]);

    let query = TransactionRequest::default().to(dex).input(calldata.into());

    if let Ok(data) = client.provider.call(query).await {
        let bytes = data.as_ref();
        if !bytes.is_empty() && bytes.len() >= 16 {
            let mut id_bytes = [0u8; 16];
            let offset = bytes.len().saturating_sub(16);
            id_bytes.copy_from_slice(&bytes[offset..]);
            return Ok(u128::from_be_bytes(id_bytes));
        }
    }
    Ok(0)
}

fn extract_order_id_from_receipt(receipt: &alloy::rpc::types::TransactionReceipt) -> Option<u128> {
    let logs = receipt.logs();
    for log in logs {
        let topics = log.topics();
        if topics.len() >= 2 {
            let topic_bytes: [u8; 32] = topics[1].0;
            let mut order_id_bytes = [0u8; 16];
            order_id_bytes.copy_from_slice(&topic_bytes[16..32]);
            let order_id = u128::from_be_bytes(order_id_bytes);
            if order_id > 0 {
                return Some(order_id);
            }
        }
    }
    None
}

fn format_token_amount(amount: u128) -> String {
    let units = 1_000_000u128;
    let whole = amount / units;
    format!("{}", whole)
}

fn format_token_amount_u256(wei: U256) -> String {
    let units = U256::from(1_000_000u64);
    let whole = wei / units;
    let fractional: u128 = (wei % units).try_into().unwrap_or(0);
    format!("{}.{:06}", whole, fractional)
}
