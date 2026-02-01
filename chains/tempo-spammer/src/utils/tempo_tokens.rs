//! Tempo Token Utilities
//!
//! Shared utilities for working with system tokens (PathUSD, AlphaUSD, BetaUSD, ThetaUSD)
//! and created tokens from the database.

use crate::TempoClient;
use alloy::primitives::{Address, U256};
use alloy::rpc::types::TransactionRequest;
use anyhow::Result;
use rand::Rng;
use std::str::FromStr;

pub struct TokenInfo {
    pub symbol: String,
    pub address: Address,
    pub is_system: bool,
}

impl TokenInfo {
    pub fn new(symbol: &str, address: &str, is_system: bool) -> Self {
        Self {
            symbol: symbol.to_string(),
            address: Address::from_str(address).unwrap_or_else(|_| Address::ZERO),
            is_system,
        }
    }
}

pub struct TempoTokens;

impl TempoTokens {
    pub const SYSTEM_TOKENS: &[(&str, &str)] = &[
        ("PathUSD", "0x20c0000000000000000000000000000000000000"),
        ("AlphaUSD", "0x20c0000000000000000000000000000000000001"),
        ("BetaUSD", "0x20c0000000000000000000000000000000000002"),
        ("ThetaUSD", "0x20c0000000000000000000000000000000000003"),
    ];

    pub fn get_system_tokens() -> Vec<TokenInfo> {
        Self::SYSTEM_TOKENS
            .iter()
            .map(|(symbol, addr)| TokenInfo::new(symbol, addr, true))
            .collect()
    }

    pub fn get_random_system_token() -> TokenInfo {
        let idx = rand::thread_rng().gen_range(0..Self::SYSTEM_TOKENS.len());
        let (symbol, addr) = Self::SYSTEM_TOKENS[idx];
        TokenInfo::new(symbol, addr, true)
    }

    pub fn get_path_usd_address() -> Address {
        Address::from_str(Self::SYSTEM_TOKENS[0].1).unwrap_or_else(|_| Address::ZERO)
    }

    pub fn get_random_memo() -> String {
        const WORDS: &[&str] = &[
            "happy", "bright", "ocean", "swift", "calm", "brave", "gentle", "wild", "sweet",
            "clear", "warm", "cool", "fresh", "peace", "dream", "hope", "joy", "love", "grace",
            "faith", "luck", "joy", "harmony", "serenity", "sunset", "sunrise", "mountain",
            "river", "forest", "sky", "star", "moon",
        ];

        let word_count = rand::thread_rng().gen_range(2..4);
        let mut words = Vec::new();
        for _ in 0..word_count {
            let idx = rand::thread_rng().gen_range(0..WORDS.len());
            words.push(WORDS[idx]);
        }

        let digit_count = rand::thread_rng().gen_range(3..6);
        let min_num = 10_u64.pow(digit_count - 1);
        let max_num = 10_u64.pow(digit_count) - 1;
        let number = rand::thread_rng().gen_range(min_num..=max_num);

        format!("{} {}", words.join(" "), number)
    }

    pub async fn get_token_balance(
        client: &crate::TempoClient,
        token: Address,
        wallet: Address,
    ) -> Result<U256> {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&[0x70, 0xa0, 0x82, 0x31]);
        calldata.extend_from_slice(&[0u8; 12]);
        calldata.extend_from_slice(wallet.as_slice());

        let query = TransactionRequest::default()
            .to(token)
            .input(calldata.into());

        if let Ok(data) = client.provider.call(query).await {
            let bytes = data.as_ref();
            if !bytes.is_empty() {
                return Ok(U256::from_be_slice(bytes));
            }
        }
        Ok(U256::ZERO)
    }

    pub async fn get_token_decimals(client: &crate::TempoClient, token: Address) -> Result<u8> {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&[0x31, 0x3f, 0x13, 0xa0]);

        let query = TransactionRequest::default()
            .to(token)
            .input(calldata.into());

        if let Ok(data) = client.provider.call(query).await {
            let bytes = data.as_ref();
            if !bytes.is_empty() && !bytes.is_empty() {
                return Ok(bytes[bytes.len() - 1]);
            }
        }
        Ok(6) // Default to 6 decimals
    }

    pub fn format_amount(amount: U256, decimals: u8) -> String {
        let divisor = U256::from(10_u64.pow(decimals as u32));
        let whole = amount / divisor;
        format!("{}", whole)
    }

    pub fn format_amount_u128(amount: u128, decimals: u8) -> String {
        let divisor = 10_u128.pow(decimals as u32);
        let whole = amount / divisor;
        format!("{}", whole)
    }
}
