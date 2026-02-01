use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct TempoConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    pub worker_count: u64,
}
