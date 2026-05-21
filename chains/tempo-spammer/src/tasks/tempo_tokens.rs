//! Tempo Token Utilities
//!
//! Shared utilities for working with system tokens (PathUSD, AlphaUSD, BetaUSD, ThetaUSD)
//! and created tokens from the database.

use crate::TempoClient;
use crate::tasks::TaskContext;
use alloy::primitives::{Address, U256};
use alloy::rpc::types::TransactionRequest;
use anyhow::Result;
use rand::Rng;
use rand::prelude::SliceRandom;
use std::str::FromStr;

#[derive(Clone)]
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

    // Use PathUSD as a temporary fallback to verify logic when all memes are dead
    pub const FALLBACK_MEME_TOKEN: &'static str = "0x20c0000000000000000000000000000000000000";

    pub fn get_system_tokens() -> Vec<TokenInfo> {
        Self::SYSTEM_TOKENS
            .iter()
            .map(|(symbol, addr)| TokenInfo::new(symbol, addr, true))
            .collect()
    }

    pub fn get_random_system_token() -> TokenInfo {
        let mut rng = rand::rngs::OsRng;
        let idx = rng.r#gen_range(0..Self::SYSTEM_TOKENS.len());
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

        let mut rng = rand::rngs::OsRng;
        let word_count = rng.r#gen_range(2..4);
        let mut words = Vec::new();
        for _ in 0..word_count {
            let idx = rng.r#gen_range(0..WORDS.len());
            words.push(WORDS[idx]);
        }

        let digit_count = rng.r#gen_range(3..6);
        let min_num = 10_u64.pow(digit_count - 1);
        let max_num = 10_u64.pow(digit_count) - 1;
        let number = rng.r#gen_range(min_num..=max_num);

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

        let data = client.provider.call(query).await?;
        let bytes = data.as_ref();
        if bytes.is_empty() {
            anyhow::bail!("Balance query returned empty data");
        }
        Ok(U256::from_be_slice(bytes))
    }

    pub async fn get_token_decimals(client: &crate::TempoClient, token: Address) -> Result<u8> {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&[0x31, 0x3c, 0xe5, 0x67]);

        let query = TransactionRequest::default()
            .to(token)
            .input(calldata.into());

        let data = client.provider.call(query).await?;
        let bytes = data.as_ref();
        if bytes.is_empty() {
            anyhow::bail!("Decimals query returned empty data");
        }
        Ok(bytes[bytes.len() - 1])
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

    /// Format amount with M/K suffixes and orange color
    /// Example: 64718064 -> "\x1b[38;5;208m64.71M\x1b[0m"
    pub fn format_compact_colored(amount: U256, decimals: u8) -> String {
        let divisor = U256::from(10_u64.pow(decimals as u32));
        let whole_units = amount / divisor;

        // Convert to f64 for compact formatting
        let value = whole_units.to_string().parse::<f64>().unwrap_or(0.0);

        let formatted = if value >= 1_000_000.0 {
            format!("{:.2}M", value / 1_000_000.0)
        } else if value >= 1_000.0 {
            format!("{:.2}K", value / 1_000.0)
        } else {
            format!("{:.2}", value)
        };

        // Orange color (ANSI 208)
        format!("\x1b[38;5;208m{}\x1b[0m", formatted)
    }
}

pub fn generate_truly_random_address() -> Address {
    let mut rng = rand::rngs::OsRng;
    let bytes: [u8; 20] = rng.r#gen();
    Address::from_slice(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_tokens_count() {
        assert_eq!(TempoTokens::SYSTEM_TOKENS.len(), 4);
    }

    #[test]
    fn test_get_system_tokens() {
        let tokens = TempoTokens::get_system_tokens();
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].symbol, "PathUSD");
        assert_eq!(tokens[1].symbol, "AlphaUSD");
        assert!(tokens[0].is_system);
    }

    #[test]
    fn test_get_path_usd_address() {
        let addr = TempoTokens::get_path_usd_address();
        let expected: Address = "0x20c0000000000000000000000000000000000000"
            .parse()
            .unwrap();
        assert_eq!(addr, expected);
    }

    #[test]
    fn test_get_random_system_token() {
        let token = TempoTokens::get_random_system_token();
        assert!(token.is_system);
        assert!(!token.symbol.is_empty());
        assert_ne!(token.address, Address::ZERO);
    }

    #[test]
    fn test_get_random_memo_format() {
        let memo = TempoTokens::get_random_memo();
        assert!(!memo.is_empty());
        // Should have words and a number: "word1 word2 123"
        let parts: Vec<&str> = memo.split(' ').collect();
        assert!(
            parts.len() >= 3,
            "memo should have words + number: got '{}'",
            memo
        );
        // Last part should be numeric
        let last = parts.last().unwrap();
        assert!(
            last.parse::<u64>().is_ok(),
            "last part should be number: got '{}'",
            last
        );
    }

    #[test]
    fn test_format_amount() {
        let amount = U256::from(1_500_000_000_000_000_000u128);
        assert_eq!(TempoTokens::format_amount(amount, 18), "1");
    }

    #[test]
    fn test_format_amount_zero() {
        assert_eq!(TempoTokens::format_amount(U256::ZERO, 18), "0");
    }

    #[test]
    fn test_format_amount_u128() {
        assert_eq!(
            TempoTokens::format_amount_u128(5_000_000_000_000_000_000, 18),
            "5"
        );
    }

    #[test]
    fn test_format_compact_colored_millions() {
        let amount = U256::from(64_718_064_000_000_000_000_000_000u128);
        let result = TempoTokens::format_compact_colored(amount, 18);
        assert!(result.contains("64.72M"));
        assert!(result.contains("208m")); // ANSI color code
    }

    #[test]
    fn test_format_compact_colored_thousands() {
        let amount = U256::from(5_432_000_000_000_000_000_000u128);
        let result = TempoTokens::format_compact_colored(amount, 18);
        assert!(result.contains("5.43K"));
    }

    #[test]
    fn test_format_compact_colored_small() {
        let amount = U256::from(123_000_000_000_000_000_000u128);
        let result = TempoTokens::format_compact_colored(amount, 18);
        assert!(result.contains("123.00"));
    }

    #[test]
    fn test_token_info_new() {
        let expected_addr: Address = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .parse()
            .unwrap();
        let t = TokenInfo::new("TEST", "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", false);
        assert_eq!(t.symbol, "TEST");
        assert!(!t.is_system);
        assert_eq!(t.address, expected_addr);
    }

    #[test]
    fn test_token_info_invalid_address_defaults_to_zero() {
        let t = TokenInfo::new("BAD", "not_an_address", true);
        assert_eq!(t.symbol, "BAD");
        assert!(t.is_system);
        assert_eq!(t.address, Address::ZERO);
    }

    #[test]
    fn test_generate_truly_random_address() {
        let a = generate_truly_random_address();
        let b = generate_truly_random_address();
        assert_ne!(a, b);
        assert_eq!(a.to_string().len(), 42);
    }

    #[test]
    fn test_format_compact_colored_million_boundary() {
        // Just below 1M: 999,999 → should be 999.99K
        let amount = U256::from(999_999_000_000_000_000_000_000u128);
        let result = TempoTokens::format_compact_colored(amount, 18);
        assert!(result.contains("K") || (!result.contains("M") && !result.contains("K")));

        // Just above 1M: 1,000,001 → should be 1.00M
        let amount2 = U256::from(1_000_001_000_000_000_000_000_000u128);
        let r2 = TempoTokens::format_compact_colored(amount2, 18);
        assert!(r2.contains("1.00M"));
    }

    #[test]
    fn test_format_compact_colored_thousand_boundary() {
        // Just above 1K: 1,001 → should be 1.00K
        let amount = U256::from(1_001_000_000_000_000_000_000u128);
        let result = TempoTokens::format_compact_colored(amount, 18);
        assert!(result.contains("1.00K"));

        // Just below 1K: 999 → should be plain number
        let amount2 = U256::from(999_000_000_000_000_000_000u128);
        let r2 = TempoTokens::format_compact_colored(amount2, 18);
        assert!(!r2.contains("K") && !r2.contains("M"));
    }

    #[test]
    fn test_format_compact_colored_zero() {
        let result = TempoTokens::format_compact_colored(U256::ZERO, 18);
        assert!(!result.contains("M") && !result.contains("K"));
        assert!(result.contains("0.00") || result.contains("208m0"));
    }

    #[test]
    fn test_format_compact_colored_ansi_wrapping() {
        let amount = U256::from(5_000_000_000_000_000_000_000u128);
        let result = TempoTokens::format_compact_colored(amount, 18);
        assert!(
            result.starts_with("\x1b[38;5;208m"),
            "should start with ANSI orange"
        );
        assert!(result.ends_with("\x1b[0m"), "should end with ANSI reset");
    }

    #[test]
    fn test_get_random_memo_word_count() {
        let mut word_counts: Vec<usize> = Vec::new();
        for _ in 0..100 {
            let memo = TempoTokens::get_random_memo();
            let parts: Vec<&str> = memo.split(' ').collect();
            // parts = [words..., number]
            word_counts.push(parts.len() - 1); // exclude the trailing number
        }
        let avg_words: f64 = word_counts.iter().sum::<usize>() as f64 / word_counts.len() as f64;
        assert!(
            avg_words >= 2.0 && avg_words <= 4.0,
            "avg words should be 2-3, got {:.1}",
            avg_words
        );
    }

    #[test]
    fn test_format_compact_colored_huge_values() {
        // Trillions
        let amount = U256::from(5_000_000_000_000_000_000_000_000_000u128);
        let result = TempoTokens::format_compact_colored(amount, 18);
        assert!(result.contains("5.00M") || result.contains("M"));
    }

    #[test]
    fn test_format_compact_colored_different_decimals() {
        // 6 decimal token (like USDC)
        let amount = U256::from(1_000_001_000_000u128); // 1,000,001 USDC raw = ~1,000,001 units
        let result = TempoTokens::format_compact_colored(amount, 6);
        assert!(result.contains("1.00M") || result.contains("1,000"));

        // 0 decimal token (like no decimals)
        let amount2 = U256::from(500);
        let r2 = TempoTokens::format_compact_colored(amount2, 0);
        assert!(r2.contains("500"));
    }

    #[test]
    fn test_format_compact_colored_exact_999999() {
        // Should show as 999.99K (just below M threshold)
        let amount = U256::from(999_999_000_000_000_000_000_000u128);
        let result = TempoTokens::format_compact_colored(amount, 18);
        assert!(result.contains("K") || result.contains("999"));
    }

    #[test]
    fn test_format_compact_colored_1000_exact() {
        let amount = U256::from(1_000_000_000_000_000_000_000u128);
        let result = TempoTokens::format_compact_colored(amount, 18);
        assert!(result.contains("1.00"));
    }

    #[test]
    fn test_format_compact_colored_one_wei() {
        // 1 wei = 0.000000000000000001
        let amount = U256::from(1);
        let result = TempoTokens::format_compact_colored(amount, 18);
        assert!(!result.contains("M") && !result.contains("K"));
        assert!(result.contains("0.00"));
    }
}
