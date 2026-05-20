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
    /// Per-worker concurrent request limit (prevents burst patterns)
    #[serde(default = "default_worker_semaphore")]
    pub worker_semaphore: usize,
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
    /// Nonce management configuration
    #[serde(default)]
    pub nonce: NonceConfig,
}

fn default_connection_semaphore() -> usize {
    500
}

fn default_worker_semaphore() -> usize {
    5
}

/// Configuration for nonce management
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NonceConfig {
    /// Base cooldown between wallet reuse in milliseconds (default: 1500ms)
    #[serde(default = "default_nonce_base_cooldown_ms")]
    pub base_cooldown_ms: u64,
    /// Minimum cooldown for fast recovery in milliseconds (default: 500ms)
    #[serde(default = "default_nonce_min_cooldown_ms")]
    pub min_cooldown_ms: u64,
    /// Whether to double cooldown on repeated errors (default: true)
    #[serde(default = "default_nonce_adaptive_backoff")]
    pub adaptive_backoff: bool,
    /// Use pending tx count instead of confirmed count (default: true)
    #[serde(default = "default_nonce_use_pending_count")]
    pub use_pending_count: bool,
    /// Use per-wallet isolated managers (default: true)
    #[serde(default = "default_nonce_per_wallet")]
    pub per_wallet: bool,
    /// Number of nonce manager shards (default: 16)
    #[serde(default = "default_nonce_shard_count")]
    pub shard_count: usize,
    /// Max retries on nonce errors (default: 5)
    #[serde(default = "default_nonce_retry_max")]
    pub retry_max: u32,
    /// Initial backoff delay for retries in milliseconds (default: 100ms)
    #[serde(default = "default_nonce_retry_initial_ms")]
    pub retry_initial_ms: u64,
    /// Maximum backoff delay for retries in milliseconds (default: 2000ms)
    #[serde(default = "default_nonce_retry_max_ms")]
    pub retry_max_ms: u64,
}

impl Default for NonceConfig {
    fn default() -> Self {
        Self {
            base_cooldown_ms: 1500,
            min_cooldown_ms: 500,
            adaptive_backoff: true,
            use_pending_count: true,
            per_wallet: true,
            shard_count: 16,
            retry_max: 5,
            retry_initial_ms: 100,
            retry_max_ms: 2000,
        }
    }
}

fn default_nonce_base_cooldown_ms() -> u64 {
    1500
}

fn default_nonce_min_cooldown_ms() -> u64 {
    500
}

fn default_nonce_adaptive_backoff() -> bool {
    true
}

fn default_nonce_use_pending_count() -> bool {
    true
}

fn default_nonce_per_wallet() -> bool {
    true
}

fn default_nonce_shard_count() -> usize {
    16
}

fn default_nonce_retry_max() -> u32 {
    5
}

fn default_nonce_retry_initial_ms() -> u64 {
    100
}

fn default_nonce_retry_max_ms() -> u64 {
    2000
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

#[cfg(test)]
mod tests {
    use super::*;

    fn full_config_toml() -> &'static str {
        r#"
rpc_url = "https://rpc.example.com"
chain_id = 42431
worker_count = 5
connection_semaphore = 500
worker_semaphore = 10
default_gas_limit = "1000000"
max_fee_per_gas = "200000000000"
priority_fee_per_gas = "2000000000"
task_interval_min = 100
task_interval_max = 300
task_timeout = 20

[nonce]
base_cooldown_ms = 800
min_cooldown_ms = 300
adaptive_backoff = false
use_pending_count = true
per_wallet = true
shard_count = 32
retry_max = 3
retry_initial_ms = 50
retry_max_ms = 500
"#
    }

    #[test]
    fn test_tempo_config_full_deserialize() {
        let config: TempoSpammerConfig = toml::from_str(full_config_toml()).unwrap();
        assert_eq!(config.rpc_url, "https://rpc.example.com");
        assert_eq!(config.chain_id, 42431);
        assert_eq!(config.worker_count, 5);
        assert_eq!(config.connection_semaphore, 500);
        assert_eq!(config.worker_semaphore, 10);
        assert_eq!(config.default_gas_limit, 1_000_000);
        assert_eq!(config.max_fee_per_gas, 200_000_000_000);
        assert_eq!(config.priority_fee_per_gas, 2_000_000_000);
        assert_eq!(config.task_interval_min, 100);
        assert_eq!(config.task_interval_max, 300);
        assert_eq!(config.task_timeout, 20);
    }

    #[test]
    fn test_tempo_config_nonce_full() {
        let config: TempoSpammerConfig = toml::from_str(full_config_toml()).unwrap();
        assert_eq!(config.nonce.base_cooldown_ms, 800);
        assert_eq!(config.nonce.min_cooldown_ms, 300);
        assert!(!config.nonce.adaptive_backoff);
        assert!(config.nonce.use_pending_count);
        assert!(config.nonce.per_wallet);
        assert_eq!(config.nonce.shard_count, 32);
        assert_eq!(config.nonce.retry_max, 3);
        assert_eq!(config.nonce.retry_initial_ms, 50);
        assert_eq!(config.nonce.retry_max_ms, 500);
    }

    #[test]
    fn test_tempo_config_minimal() {
        let toml = r#"
rpc_url = "https://rpc.test.com"
chain_id = 1
worker_count = 2
default_gas_limit = "50000"
max_fee_per_gas = "100"
priority_fee_per_gas = "1"
task_interval_min = 200
task_interval_max = 500
task_timeout = 30
"#;
        let config: TempoSpammerConfig = toml::from_str(toml).unwrap();
        // Required fields
        assert_eq!(config.rpc_url, "https://rpc.test.com");
        assert_eq!(config.chain_id, 1);
        assert_eq!(config.worker_count, 2);
        assert_eq!(config.default_gas_limit, 50_000);
        // Defaults
        assert_eq!(config.connection_semaphore, 500);
        assert_eq!(config.worker_semaphore, 5);
        assert_eq!(config.nonce, NonceConfig::default());
    }

    #[test]
    fn test_tempo_config_missing_required() {
        let toml = r#"
rpc_url = "https://rpc.test.com"
chain_id = 1
"#;
        let result: Result<TempoSpammerConfig, _> = toml::from_str(toml);
        assert!(result.is_err(), "Should fail with missing required fields");
    }

    #[test]
    fn test_nonce_config_default() {
        let default = NonceConfig::default();
        assert_eq!(default.base_cooldown_ms, 1500);
        assert_eq!(default.min_cooldown_ms, 500);
        assert!(default.adaptive_backoff);
        assert!(default.use_pending_count);
        assert!(default.per_wallet);
        assert_eq!(default.shard_count, 16);
        assert_eq!(default.retry_max, 5);
        assert_eq!(default.retry_initial_ms, 100);
        assert_eq!(default.retry_max_ms, 2000);
    }

    #[test]
    fn test_u128_config_from_string() {
        let u = U128Config::try_from("1000000".to_string()).unwrap();
        let val: u128 = u.into();
        assert_eq!(val, 1_000_000);
    }

    #[test]
    fn test_u128_config_from_string_invalid() {
        let result = U128Config::try_from("not_a_number".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_u128_from_string() {
        let toml = r#"value = "50000""#;
        #[derive(Deserialize)]
        struct Test { #[serde(deserialize_with = "deserialize_u128")] value: u128 }
        let t: Test = toml::from_str(toml).unwrap();
        assert_eq!(t.value, 50_000);
    }

    #[test]
    fn test_deserialize_u128_from_int() {
        let toml = "value = 50000";
        #[derive(Deserialize)]
        struct Test { #[serde(deserialize_with = "deserialize_u128")] value: u128 }
        let t: Test = toml::from_str(toml).unwrap();
        assert_eq!(t.value, 50_000);
    }

    #[test]
    fn test_deserialize_u128_from_negative_int() {
        let toml = "value = -1";
        #[derive(Deserialize)]
        struct Test { #[serde(deserialize_with = "deserialize_u128")] value: u128 }
        let result: Result<Test, _> = toml::from_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_tempo_config_connection_defaults() {
        assert_eq!(default_connection_semaphore(), 500);
        assert_eq!(default_worker_semaphore(), 5);
    }

    #[test]
    fn test_tempo_config_u128_via_integer() {
        // Accept integer literals for u128 fields
        let toml = r#"
rpc_url = "https://rpc.test.com"
chain_id = 1
worker_count = 1
default_gas_limit = 1000000
max_fee_per_gas = 200000000000
priority_fee_per_gas = 2000000000
task_interval_min = 100
task_interval_max = 300
task_timeout = 20
"#;
        let config: TempoSpammerConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.default_gas_limit, 1_000_000);
    }
}
