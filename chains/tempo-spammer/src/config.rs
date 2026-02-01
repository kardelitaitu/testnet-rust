//! Configuration loader for tempo-spammer

use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::str::FromStr;

#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "String")]
pub struct U128Config(u128);

impl TryFrom<String> for U128Config {
    type Error = anyhow::Error;

    fn try_from(s: String) -> Result<Self> {
        Ok(Self(u128::from_str(&s).context("Failed to parse u128")?))
    }
}

impl From<U128Config> for u128 {
    fn from(val: U128Config) -> Self {
        val.0
    }
}

/// Configuration for the tempo spammer
#[derive(Debug, Clone, Deserialize)]
pub struct TempoSpammerConfig {
    /// RPC endpoint URL
    pub rpc_url: String,
    /// Chain ID (42431 for Tempo testnet)
    pub chain_id: u64,
    /// Number of worker threads
    pub worker_count: u64,
    /// Maximum concurrent connections (semaphore limit)
    #[serde(default = "default_connection_semaphore")]
    pub connection_semaphore: usize,
    /// Default gas limit for transactions
    #[serde(deserialize_with = "deserialize_u128")]
    pub default_gas_limit: u128,
    /// Maximum fee per gas in wei
    #[serde(deserialize_with = "deserialize_u128")]
    pub max_fee_per_gas: u128,
    /// Priority fee per gas in wei
    #[serde(deserialize_with = "deserialize_u128")]
    pub priority_fee_per_gas: u128,
    /// Minimum task interval in milliseconds
    pub task_interval_min: u64,
    /// Maximum task interval in milliseconds
    pub task_interval_max: u64,
    /// Task timeout in seconds
    pub task_timeout: u64,
}

fn default_connection_semaphore() -> usize {
    500
}

fn deserialize_u128<'de, D>(deserializer: D) -> Result<u128, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct U128Visitor;

    impl<'de> serde::de::Visitor<'de> for U128Visitor {
        type Value = u128;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or integer representing a u128")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            u128::from_str(value).map_err(|_| E::custom("invalid u128"))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value as u128)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if value >= 0 {
                Ok(value as u128)
            } else {
                Err(E::custom("negative u128"))
            }
        }
    }

    deserializer.deserialize_any(U128Visitor)
}

impl TempoSpammerConfig {
    /// Load configuration from a TOML file
    ///
    /// # Arguments
    /// * `path` - Path to the config.toml file
    ///
    /// # Example
    /// ```ignore
    /// let config = TempoSpammerConfig::from_path("config/config.toml")?;
    /// ```
    pub fn from_path(path: &str) -> Result<Self> {
        let content =
            fs::read_to_string(path).context(format!("Failed to read config from {}", path))?;
        toml::from_str(&content).context("Failed to parse config TOML")
    }

    /// Get a random task interval between min and max
    pub fn random_interval(&self) -> u64 {
        let mut rng = rand::thread_rng();
        rand::Rng::gen_range(&mut rng, self.task_interval_min..=self.task_interval_max)
    }
}
