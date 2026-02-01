//! # EVM Chain Adapter
//!
//! This module provides a reference implementation of the chain traits
//! for EVM-compatible blockchains. Use this as a template when adding
//! new EVM-based chains to the framework.

use super::*;
use crate::utils::{GasConfig, RpcManager};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// EVM-compatible chain adapter implementation
#[derive(Debug)]
pub struct EvmChainAdapter {
    config: SpammerConfig,
    rpc_manager: RpcManager,
    gas_config: GasConfig,
}

impl EvmChainAdapter {
    /// Create a new EVM chain adapter
    pub fn new(config: SpammerConfig, rpc_urls: Vec<String>) -> Self {
        Self {
            config: config.clone(),
            rpc_manager: RpcManager::new(config.chain_id, &rpc_urls),
            gas_config: GasConfig::new(),
        }
    }

    /// Create with custom gas configuration
    pub fn with_gas_config(mut self, gas_config: GasConfig) -> Self {
        self.gas_config = gas_config;
        self
    }

    /// Get the RPC manager
    pub fn rpc_manager(&self) -> &RpcManager {
        &self.rpc_manager
    }

    /// Get the gas configuration
    pub fn gas_config(&self) -> &GasConfig {
        &self.gas_config
    }
}

#[async_trait]
impl ChainSpammer for EvmChainAdapter {
    fn name(&self) -> &str {
        "EvmChainAdapter"
    }

    fn config(&self) -> &SpammerConfig {
        &self.config
    }

    async fn execute_task(&self) -> Result<SpammerResult, String> {
        // Placeholder implementation
        // In a real implementation, this would execute spam transactions
        Ok(SpammerResult {
            success: true,
            message: "Task executed".to_string(),
            tx_hash: None,
        })
    }

    async fn start(&self, token: CancellationToken) -> Result<(), String> {
        info!(
            "Starting EVM chain spammer for chain {}",
            self.config.chain_id
        );

        loop {
            if token.is_cancelled() {
                break;
            }

            if let Err(e) = self.execute_task().await {
                tracing::error!("Task failed: {}", e);
            }

            let delay_ms = 1000 / self.config.target_tps.max(1) as u64;
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        Ok(())
    }

    fn stop(&self) {
        info!("Stopping EVM chain spammer");
    }
}

/// Helper function to create an EVM adapter with default settings
pub fn create_evm_adapter(rpc_url: &str, chain_id: u64, tps: u32) -> EvmChainAdapter {
    let config = SpammerConfig {
        rpc_url: rpc_url.to_string(),
        chain_id,
        target_tps: tps,
    };

    EvmChainAdapter::new(config, vec![rpc_url.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evm_adapter_creation() {
        let adapter = create_evm_adapter("https://rpc.example.com", 1, 10);
        assert_eq!(adapter.config.chain_id, 1);
        assert_eq!(adapter.config.target_tps, 10);
    }
}
