use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpamConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    pub target_tps: u32,
    pub duration_seconds: Option<u64>,
    pub wallet_source: WalletSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalletSource {
    File { path: String, encrypted: bool },
    Env { key: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub name: String,
    pub rpc_endpoint: String,
    pub chain_id: u64,
}
