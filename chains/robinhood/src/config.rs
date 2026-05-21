use anyhow::Result;
use config::{Config, File};
use core_logic::config::{ProxyConfig, SpamConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct EvmConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    pub private_key_file: String, // Path to encrypted wallet file
    pub tps: u32,
    #[allow(dead_code)]
    pub proxies: Option<Vec<ProxyConfig>>,
}

impl EvmConfig {
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
            duration_seconds: None, // Infinite by default
            wallet_source: core_logic::config::WalletSource::File {
                path: self.private_key_file.clone(),
                encrypted: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Config;

    #[test]
    fn test_evm_config_deserialize() {
        let toml = r#"
rpc_url = "https://rpc.evm.com"
chain_id = 42
tps = 10
private_key_file = "wallet.json"
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: EvmConfig = settings.try_deserialize().unwrap();
        assert_eq!(cfg.rpc_url, "https://rpc.evm.com");
        assert_eq!(cfg.chain_id, 42);
        assert_eq!(cfg.private_key_file, "wallet.json");
    }

    #[test]
    fn test_evm_config_to_spam() {
        let toml = r#"
rpc_url = "https://rpc.evm.com"
chain_id = 42
tps = 10
private_key_file = "wallet.json"
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: EvmConfig = settings.try_deserialize().unwrap();
        let spam = cfg.to_spam_config();
        assert_eq!(spam.chain_id, 42);
        assert_eq!(spam.target_tps, 10);
    }

    #[test]
    fn test_evm_config_missing_required() {
        let settings = Config::builder()
            .add_source(config::File::from_str(
                r#"rpc_url = "x""#,
                config::FileFormat::Toml,
            ))
            .build()
            .unwrap();
        let result: Result<EvmConfig, _> = settings.try_deserialize();
        let err = result.as_ref().unwrap_err().to_string();
        assert!(
            err.contains("chain_id") || err.contains("private_key_file") || err.contains("tps"),
            "error should mention missing field, got: {}",
            err
        );
    }

    #[test]
    fn test_evm_config_with_proxies() {
        let toml = r#"
rpc_url = "https://rpc.evm.com"
chain_id = 1
tps = 5
private_key_file = "wallets.json"
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
        let cfg: EvmConfig = settings.try_deserialize().unwrap();
        assert_eq!(cfg.rpc_url, "https://rpc.evm.com");
        assert_eq!(cfg.chain_id, 1);
        assert!(cfg.proxies.is_some());
        let proxies = cfg.proxies.unwrap();
        assert_eq!(proxies.len(), 2);
        assert_eq!(proxies[0].url, "http://10.0.0.1:3128");
        assert_eq!(proxies[0].username.as_deref(), Some("user1"));
        assert_eq!(proxies[1].url, "http://10.0.0.2:8080");
        assert!(proxies[1].username.is_none());
    }

    #[test]
    fn test_evm_config_edge_cases() {
        // Zero chain_id, empty rpc_url
        let toml = r#"
rpc_url = ""
chain_id = 0
tps = 0
private_key_file = ""
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: EvmConfig = settings.try_deserialize().unwrap();
        assert_eq!(cfg.rpc_url, "");
        assert_eq!(cfg.chain_id, 0);
        assert_eq!(cfg.tps, 0);
        assert_eq!(cfg.private_key_file, "");
    }
}
