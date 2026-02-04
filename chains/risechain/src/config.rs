use anyhow::Result;
use config::{Config, File};
use core_logic::config::{ProxyConfig, SpamConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct RiseConfig {
    pub rpc_url: String,
    pub chain_id: u64,
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

impl RiseConfig {
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
