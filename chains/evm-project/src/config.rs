use anyhow::Result;
use config::{Config, File};
use core_logic::config::{ProxyConfig, SpamConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
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
