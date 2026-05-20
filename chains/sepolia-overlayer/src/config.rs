use anyhow::Result;
use config::{Config, File};
use core_logic::config::{ProxyConfig, SpamConfig};
use serde::Deserialize;

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
            .add_source(config::File::from_str(test_config_toml(), config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: SepoliaConfig = settings.try_deserialize().unwrap();
        assert_eq!(cfg.rpc_url, "https://ethereum-sepolia-rpc.publicnode.com");
        assert_eq!(cfg.chain_id, 11155111);
        assert_eq!(cfg.explorer, "https://sepolia.etherscan.io");
        assert_eq!(cfg.symbol, "ETH");
        assert_eq!(cfg.tps, 10);
        assert_eq!(cfg.worker_amount, Some(5));
        assert_eq!(cfg.wallet_dir.as_deref(), Some("chains/sepolia-overlayer/wallets-json-sepolia-overlayer"));
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
            .add_source(config::File::from_str(test_config_toml(), config::FileFormat::Toml))
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
}
