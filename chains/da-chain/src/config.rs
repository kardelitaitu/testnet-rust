use anyhow::Result;
use config::{Config, File};
use core_logic::config::{ProxyConfig, SpamConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct DaChainConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    pub explorer: String,
    pub symbol: String,
    #[serde(default)] // Optional - WalletManager auto-detects wallet-json/
    pub private_key_file: Option<String>,
    pub tps: u32,
    pub worker_amount: Option<usize>,
    pub min_delay_ms: Option<u64>,
    pub max_delay_ms: Option<u64>,
    pub create2_factory: Option<String>,
    #[serde(default)]
    pub wallet_dir: Option<String>, // NEW: Custom wallet directory
    #[allow(dead_code)]
    pub proxies: Option<Vec<ProxyConfig>>,
}

impl DaChainConfig {
    pub fn load(path: &str) -> Result<Self> {
        let settings = Config::builder().add_source(File::with_name(path)).build()?;

        settings.try_deserialize().map_err(|e| anyhow::anyhow!(e))
    }

    pub fn to_spam_config(&self) -> SpamConfig {
        SpamConfig {
            rpc_url: self.rpc_url.clone(),
            chain_id: self.chain_id,
            target_tps: self.tps,
            duration_seconds: None, // Infinite by default
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
rpc_url = "https://rpctest.dachain.tech"
chain_id = 21894
explorer = "https://dachain.tech"
symbol = "DACC"
tps = 10
worker_amount = 5
wallet_dir = "chains/da-chain/wallets-json-da-chain"
min_delay_ms = 60000
max_delay_ms = 120000
"#
    }

    #[test]
    fn test_dachain_config_deserialize() {
        let settings = Config::builder()
            .add_source(config::File::from_str(test_config_toml(), config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: DaChainConfig = settings.try_deserialize().unwrap();
        assert_eq!(cfg.rpc_url, "https://rpctest.dachain.tech");
        assert_eq!(cfg.chain_id, 21894);
        assert_eq!(cfg.symbol, "DACC");
        assert_eq!(cfg.tps, 10);
        assert_eq!(cfg.worker_amount, Some(5));
    }

    #[test]
    fn test_dachain_config_to_spam_config() {
        let settings = Config::builder()
            .add_source(config::File::from_str(test_config_toml(), config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: DaChainConfig = settings.try_deserialize().unwrap();
        let spam = cfg.to_spam_config();
        assert_eq!(spam.rpc_url, cfg.rpc_url);
        assert_eq!(spam.chain_id, cfg.chain_id);
    }

    #[test]
    fn test_dachain_config_minimal() {
        let toml = r#"
rpc_url = "https://rpc.com"
chain_id = 1
explorer = "https://exp.com"
symbol = "TKN"
tps = 5
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap();
        let cfg: DaChainConfig = settings.try_deserialize().unwrap();
        assert_eq!(cfg.tps, 5);
        assert!(cfg.wallet_dir.is_none());
    }

    #[test]
    fn test_dachain_config_missing_required() {
        let settings = Config::builder()
            .add_source(config::File::from_str(r#"rpc_url = "x""#, config::FileFormat::Toml))
            .build()
            .unwrap();
        let result: Result<DaChainConfig, _> = settings.try_deserialize();
        let err = result.as_ref().unwrap_err().to_string();
        assert!(
            err.contains("chain_id") || err.contains("symbol") || err.contains("explorer") || err.contains("tps"),
            "error should mention missing field, got: {}",
            err
        );
    }
}
