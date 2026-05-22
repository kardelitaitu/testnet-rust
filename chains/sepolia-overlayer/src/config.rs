use std::collections::HashMap;

use anyhow::Result;
use config::{Config, File};
use core_logic::config::{ProxyConfig, SpamConfig};
use serde::Deserialize;

/// Per-task daily run limit. Task not present → default limit = 1.
pub type TaskLimits = HashMap<String, u32>;

#[derive(Debug, Deserialize, Clone)]
pub struct SepoliaConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    pub explorer: String,
    pub symbol: String,
    #[serde(default)]
    pub private_key_file: Option<String>,
    pub tps: u32,
    pub worker_amount: Option<usize>,
    pub min_delay_ms: Option<u64>,
    pub max_delay_ms: Option<u64>,
    #[serde(default)]
    pub wallet_dir: Option<String>,
    #[allow(dead_code)]
    pub proxies: Option<Vec<ProxyConfig>>,
    /// Per-task daily limits. Example: {"01_checkBalance" = 100, "10_aaveUsdtFaucet" = 5}
    #[serde(default)]
    pub task_limits: Option<TaskLimits>,
    /// Timeout for one daily task attempt, in seconds.
    #[serde(default)]
    pub task_timeout_secs: Option<u64>,
}

impl SepoliaConfig {
    pub fn load(path: &str) -> Result<Self> {
        let settings = Config::builder()
            .add_source(File::with_name(path))
            .build()?;

        settings.try_deserialize().map_err(|e| anyhow::anyhow!(e))
    }

    pub fn to_spam_config(&self) -> SpamConfig {
        SpamConfig {
            rpc_url: self.rpc_url.clone(),
            chain_id: self.chain_id,
            target_tps: self.tps,
            duration_seconds: None,
            wallet_source: core_logic::config::WalletSource::File {
                path: self.private_key_file.clone().unwrap_or_default(),
                encrypted: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Config;

    fn test_config_toml() -> &'static str {
        r#"
rpc_url = "https://ethereum-sepolia-rpc.publicnode.com"
chain_id = 11155111
explorer = "https://sepolia.etherscan.io"
symbol = "ETH"
tps = 10
worker_amount = 5
wallet_dir = "chains/sepolia-overlayer/wallets-json-sepolia-overlayer"
min_delay_ms = 5000
max_delay_ms = 15000
"#
    }

    #[test]
    fn test_sepolia_config_deserialize() {
        let settings = Config::builder()
            .add_source(config::File::from_str(
                test_config_toml(),
                config::FileFormat::Toml,
            ))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        assert_eq!(cfg.rpc_url, "https://ethereum-sepolia-rpc.publicnode.com");
        assert_eq!(cfg.chain_id, 11155111);
        assert_eq!(cfg.explorer, "https://sepolia.etherscan.io");
        assert_eq!(cfg.symbol, "ETH");
        assert_eq!(cfg.tps, 10);
        assert_eq!(cfg.worker_amount, Some(5));
        assert_eq!(
            cfg.wallet_dir.as_deref(),
            Some("chains/sepolia-overlayer/wallets-json-sepolia-overlayer")
        );
        assert_eq!(cfg.min_delay_ms, Some(5000));
        assert_eq!(cfg.max_delay_ms, Some(15000));
    }

    #[test]
    fn test_sepolia_config_minimal() {
        let toml = r#"
rpc_url = "https://rpc.com"
chain_id = 1
explorer = "https://explorer.com"
symbol = "ETH"
tps = 5
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        assert_eq!(cfg.rpc_url, "https://rpc.com");
        assert_eq!(cfg.tps, 5);
        assert!(cfg.worker_amount.is_none());
        assert!(cfg.wallet_dir.is_none());
        assert!(cfg.proxies.is_none());
    }

    #[test]
    fn test_sepolia_config_to_spam_config() {
        let settings = Config::builder()
            .add_source(config::File::from_str(
                test_config_toml(),
                config::FileFormat::Toml,
            ))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        let spam = cfg.to_spam_config();
        assert_eq!(spam.rpc_url, cfg.rpc_url);
        assert_eq!(spam.chain_id, cfg.chain_id);
        assert_eq!(spam.target_tps, cfg.tps);
    }

    #[test]
    fn test_sepolia_config_missing_required_field() {
        let toml = r#"
rpc_url = "https://rpc.com"
chain_id = 1
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let result: Result<SepoliaConfig, _> = settings.try_deserialize();
        assert!(result.is_err());
    }

    #[test]
    fn test_sepolia_config_with_proxies() {
        let toml = r#"
rpc_url = "https://rpc.com"
chain_id = 1
explorer = "https://explorer.com"
symbol = "ETH"
tps = 10
[[proxies]]
url = "http://10.0.0.1:3128"
username = "user1"
password = "pass1"
[[proxies]]
url = "http://10.0.0.2:8080"
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        assert!(cfg.proxies.is_some());
        let proxies = cfg.proxies.unwrap();
        assert_eq!(proxies.len(), 2);
        assert_eq!(proxies[0].url, "http://10.0.0.1:3128");
        assert_eq!(proxies[0].username.as_deref(), Some("user1"));
        assert_eq!(proxies[1].url, "http://10.0.0.2:8080");
        assert!(proxies[1].username.is_none());
    }

    #[test]
    fn test_sepolia_config_with_private_key_file() {
        let toml = r#"
rpc_url = "https://rpc.com"
chain_id = 1
explorer = "https://explorer.com"
symbol = "ETH"
tps = 10
private_key_file = "my_keys.json"
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        assert_eq!(cfg.private_key_file.as_deref(), Some("my_keys.json"));
        let spam = cfg.to_spam_config();
        // to_spam_config uses private_key_file for wallet_source path
        match spam.wallet_source {
            core_logic::config::WalletSource::File { path, .. } => {
                assert_eq!(path, "my_keys.json");
            }
            _ => panic!("Expected File wallet source"),
        }
    }

    #[test]
    fn test_sepolia_config_no_private_key_fallback() {
        // When private_key_file is None, to_spam_config uses empty string
        let toml = r#"
rpc_url = "https://rpc.com"
chain_id = 1
explorer = "https://explorer.com"
symbol = "ETH"
tps = 10
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        assert!(cfg.private_key_file.is_none());
        let spam = cfg.to_spam_config();
        match spam.wallet_source {
            core_logic::config::WalletSource::File { path, .. } => {
                assert_eq!(path, "", "Empty path fallback when no private_key_file");
            }
            _ => panic!("Expected File wallet source"),
        }
    }

    #[test]
    fn test_sepolia_config_with_task_limits() {
        let toml = r#"
rpc_url = "https://rpc.com"
chain_id = 1
explorer = "https://explorer.com"
symbol = "ETH"
tps = 10

[task_limits]
01_checkbalance = 20
02_mintusdtplus = 10
03_mintusdcplus = 5
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        let limits = cfg.task_limits.expect("task_limits should be Some");
        assert_eq!(limits.len(), 3);
        assert_eq!(limits.get("01_checkbalance"), Some(&20));
        assert_eq!(limits.get("02_mintusdtplus"), Some(&10));
        assert_eq!(limits.get("03_mintusdcplus"), Some(&5));
    }

    #[test]
    fn test_sepolia_config_task_limits_empty() {
        let toml = r#"
rpc_url = "https://rpc.com"
chain_id = 1
explorer = "https://explorer.com"
symbol = "ETH"
tps = 10
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        assert!(
            cfg.task_limits.is_none(),
            "task_limits should be None when no [task_limits] section"
        );
    }

    #[test]
    fn test_sepolia_config_task_limits_keys_lowercased() {
        // The `config` crate lowercases all HashMap keys automatically.
        // Keys like "01_checkBalance" become "01_checkbalance" in the HashMap.
        let toml = r#"
rpc_url = "https://rpc.com"
chain_id = 1
explorer = "https://explorer.com"
symbol = "ETH"
tps = 10

[task_limits]
01_checkBalance = 20
02_MintUsdtPlus = 10
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        let limits = cfg.task_limits.expect("task_limits should be Some");
        // Lowercased keys should exist
        assert_eq!(
            limits.get("01_checkbalance"),
            Some(&20),
            "lowercased key '01_checkbalance' should exist"
        );
        assert_eq!(
            limits.get("02_mintusdtplus"),
            Some(&10),
            "lowercased key '02_mintusdtplus' should exist"
        );
        // CamelCase keys should NOT exist
        assert!(
            limits.get("01_checkBalance").is_none(),
            "camelCase key '01_checkBalance' should NOT exist"
        );
        assert!(
            limits.get("02_MintUsdtPlus").is_none(),
            "camelCase key '02_MintUsdtPlus' should NOT exist"
        );
    }

    #[test]
    fn test_sepolia_config_task_limits_roundtrip() {
        let toml = r#"
rpc_url = "https://rpc.com"
chain_id = 1
explorer = "https://explorer.com"
symbol = "ETH"
tps = 10

[task_limits]
01_checkbalance = 20
02_mintusdtplus = 10
03_mintusdcplus = 10
04_redeemusdtplus = 5
05_redeemusdcplus = 5
06_stakeusdtplus = 5
07_stakeusdcplus = 5
08_unstaketplus = 3
09_unstakecplus = 3
10_aaveusdtfaucet = 5
11_aaveusdcfaucet = 5
12_bridgetplus = 1
13_bridgecplus = 1
14_sendrandomusdtplus = 1
15_sendrandomusdcplus = 1
16_bridgebacktplus = 1
17_bridgebackcplus = 1
18_receivetplus = 1
19_receivecplus = 1
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        let limits = cfg.task_limits.expect("task_limits should be Some");
        assert_eq!(limits.len(), 19, "all 19 task limits should be present");
        assert_eq!(limits.get("01_checkbalance"), Some(&20));
        assert_eq!(limits.get("02_mintusdtplus"), Some(&10));
        assert_eq!(limits.get("03_mintusdcplus"), Some(&10));
        assert_eq!(limits.get("04_redeemusdtplus"), Some(&5));
        assert_eq!(limits.get("05_redeemusdcplus"), Some(&5));
        assert_eq!(limits.get("06_stakeusdtplus"), Some(&5));
        assert_eq!(limits.get("07_stakeusdcplus"), Some(&5));
        assert_eq!(limits.get("08_unstaketplus"), Some(&3));
        assert_eq!(limits.get("09_unstakecplus"), Some(&3));
        assert_eq!(limits.get("10_aaveusdtfaucet"), Some(&5));
        assert_eq!(limits.get("11_aaveusdcfaucet"), Some(&5));
        assert_eq!(limits.get("12_bridgetplus"), Some(&1));
        assert_eq!(limits.get("13_bridgecplus"), Some(&1));
        assert_eq!(limits.get("14_sendrandomusdtplus"), Some(&1));
        assert_eq!(limits.get("15_sendrandomusdcplus"), Some(&1));
        assert_eq!(limits.get("16_bridgebacktplus"), Some(&1));
        assert_eq!(limits.get("17_bridgebackcplus"), Some(&1));
        assert_eq!(
            limits.get("18_receivetplus"),
            Some(&1),
            "t18 receiveTplus should have limit 1"
        );
        assert_eq!(
            limits.get("19_receivecplus"),
            Some(&1),
            "t19 receiveCplus should have limit 1"
        );
    }

    /// Verifies that the new t18 and t19 task limits can be resolved via
    /// case-insensitive lookup (simulating how get_task_limit works at runtime).
    #[test]
    fn test_sepolia_config_task_limits_18_19_case_insensitive() {
        let toml = r#"
rpc_url = "https://rpc.com"
chain_id = 1
explorer = "https://explorer.com"
symbol = "ETH"
tps = 10

[task_limits]
18_receivetplus = 2
19_receivecplus = 3
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        let limits = cfg.task_limits.expect("task_limits should be Some");

        // TOML lowercases keys, so we look up the lowercased versions.
        // Simulate the runtime get_task_limit pattern (eq_ignore_ascii_case).
        let t18_val = limits
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("18_receiveTplus"))
            .map(|(_, &v)| v);
        let t19_val = limits
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("19_receiveCplus"))
            .map(|(_, &v)| v);

        assert_eq!(
            t18_val,
            Some(2),
            "t18 should resolve via case-insensitive match"
        );
        assert_eq!(
            t19_val,
            Some(3),
            "t19 should resolve via case-insensitive match"
        );
    }

    #[test]
    fn test_sepolia_config_with_proxies_and_task_limits() {
        let toml = r#"
rpc_url = "https://rpc.com"
chain_id = 1
explorer = "https://explorer.com"
symbol = "ETH"
tps = 10

[[proxies]]
url = "http://10.0.0.1:3128"
username = "user1"
password = "pass1"

[[proxies]]
url = "http://10.0.0.2:8080"

[task_limits]
01_checkbalance = 20
02_mintusdtplus = 10
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        // Verify proxies
        assert!(cfg.proxies.is_some());
        let proxies = cfg.proxies.unwrap();
        assert_eq!(proxies.len(), 2);
        assert_eq!(proxies[0].url, "http://10.0.0.1:3128");
        assert_eq!(proxies[1].url, "http://10.0.0.2:8080");
        // Verify task_limits
        let limits = cfg.task_limits.expect("task_limits should be Some");
        assert_eq!(limits.len(), 2);
        assert_eq!(limits.get("01_checkbalance"), Some(&20));
        assert_eq!(limits.get("02_mintusdtplus"), Some(&10));
    }

    #[test]
    fn test_sepolia_config_to_spam_config_with_wallet_dir() {
        let toml = r#"
rpc_url = "https://sepolia-rpc.example.com"
chain_id = 11155111
explorer = "https://sepolia.etherscan.io"
symbol = "ETH"
tps = 15
wallet_dir = "chains/sepolia-overlayer/wallets-json-sepolia-overlayer"
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        let spam = cfg.to_spam_config();
        assert_eq!(spam.rpc_url, "https://sepolia-rpc.example.com");
        assert_eq!(spam.chain_id, 11155111);
        assert_eq!(spam.target_tps, 15);
        // WalletSource should use private_key_file path (empty default when unset)
        match spam.wallet_source {
            core_logic::config::WalletSource::File { path, encrypted } => {
                assert_eq!(
                    path, "",
                    "private_key_file not set, should default to empty string"
                );
                assert!(encrypted);
            }
            _ => panic!("Expected File wallet source"),
        }
    }

    #[test]
    fn test_sepolia_config_with_task_timeout() {
        let toml = r#"
rpc_url = "https://sepolia-rpc.example.com"
chain_id = 11155111
explorer = "https://sepolia.etherscan.io"
symbol = "ETH"
tps = 10
task_timeout_secs = 300
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        assert_eq!(cfg.task_timeout_secs, Some(300));
    }

    #[test]
    fn test_sepolia_config_task_timeout_defaults_to_none() {
        let toml = r#"
rpc_url = "https://sepolia-rpc.example.com"
chain_id = 11155111
explorer = "https://sepolia.etherscan.io"
symbol = "ETH"
tps = 10
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        assert_eq!(cfg.task_timeout_secs, None);
    }
}
