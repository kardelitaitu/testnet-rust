use anyhow::Result;
use config::{Config, File};
use core_logic::config::{ProxyConfig, SpamConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct XeneaConfig {
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
    #[allow(dead_code)]
    pub proxies: Option<Vec<ProxyConfig>>,
}

impl XeneaConfig {
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

    #[test]
    fn test_xenea_config_deserialize() {
        let toml = r#"
rpc_url = "https://rpc.xenea.com"
chain_id = 777
explorer = "https://exp.xenea.com"
symbol = "XEN"
tps = 10
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build().unwrap();
        let cfg: XeneaConfig = settings.try_deserialize().unwrap();
        assert_eq!(cfg.rpc_url, "https://rpc.xenea.com");
        assert_eq!(cfg.chain_id, 777);
    }

    #[test]
    fn test_xenea_config_to_spam() {
        let toml = r#"
rpc_url = "https://rpc.xenea.com"
chain_id = 777
explorer = "https://exp.xenea.com"
symbol = "XEN"
tps = 10
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build().unwrap();
        let cfg: XeneaConfig = settings.try_deserialize().unwrap();
        let spam = cfg.to_spam_config();
        assert_eq!(spam.chain_id, 777);
    }
}

