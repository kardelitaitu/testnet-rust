//! Utility functions for tempo-alloy client

use alloy::primitives::Address;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::str::FromStr;

/// Configuration for the client
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    pub chain_id: u64,
    pub worker_count: Option<u64>,
}

/// Load configuration from file
pub fn load_config(path: &str) -> Result<Config> {
    let content = fs::read_to_string(path).context("Failed to read config")?;
    toml::from_str(&content).context("Failed to parse config")
}

/// Get a random address from address.txt
pub fn get_random_address() -> Result<Address> {
    let path = Path::new("config/address.txt");
    if !path.exists() {
        // Return a random address if file doesn't exist
        return Ok(Address::random());
    }

    let content = fs::read_to_string(path).context("Failed to read address.txt")?;
    let addresses: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    if addresses.is_empty() {
        return Ok(Address::random());
    }

    let random_address = addresses
        .choose(&mut rand::thread_rng())
        .unwrap_or(&addresses[0]);

    Address::from_str(random_address.trim()).context("Invalid address")
}

/// Load proxy list from file
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

pub fn load_proxies(path: &str) -> Result<Vec<ProxyConfig>> {
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path).context("Failed to read proxies.txt")?;

    let proxies: Vec<ProxyConfig> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.split(':').collect();
            match parts.len() {
                1 => Some(ProxyConfig {
                    url: parts[0].to_string(),
                    username: None,
                    password: None,
                }),
                3 => Some(ProxyConfig {
                    url: parts[0].to_string(),
                    username: Some(parts[1].to_string()),
                    password: Some(parts[2].to_string()),
                }),
                _ => None,
            }
        })
        .collect();

    Ok(proxies)
}
