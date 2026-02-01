//! # Core Logic - Chain Templates
//!
//! This module provides templates and traits for implementing new blockchain
//! integrations. The core-logic utilities are designed to work with any
//! chain implementation that adheres to these traits.

use async_trait::async_trait;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub mod evm_adapter;
pub use evm_adapter::*;

/// Result type for spammer operations
#[derive(Debug, Clone)]
pub struct SpammerResult {
    pub success: bool,
    pub message: String,
    pub tx_hash: Option<String>,
}

/// Configuration for a spammer
#[derive(Debug, Clone)]
pub struct SpammerConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    pub target_tps: u32,
}

/// Trait for implementing chain-specific spammer logic
#[async_trait]
pub trait ChainSpammer: Send + Sync {
    fn name(&self) -> &str;
    fn config(&self) -> &SpammerConfig;
    async fn execute_task(&self) -> Result<SpammerResult, String>;
    async fn start(&self, token: CancellationToken) -> Result<(), String>;
    fn stop(&self);
}

/// Trait for gas estimation - chains implement this based on their gas model
#[async_trait]
pub trait GasEstimator: Send + Sync {
    type Error;
    async fn estimate_gas(&self, data: &[u8], to: Option<&str>) -> Result<u64, Self::Error>;
    async fn get_gas_price(&self) -> Result<u64, Self::Error>;
}

/// Trait for transaction signing - chains implement this based on their signature scheme
#[async_trait]
pub trait TransactionSigner: Send + Sync {
    type Address;
    type TransactionHash;
    type Signature;

    fn address(&self) -> &Self::Address;
    async fn sign_transaction(
        &self,
        to: &str,
        data: &[u8],
        value: u64,
        gas_limit: u64,
    ) -> Result<Vec<u8>, String>;
    async fn send_raw_transaction(&self, signed_tx: &[u8])
        -> Result<Self::TransactionHash, String>;
}

/// Trait for RPC provider - chains implement this based on their RPC API
#[async_trait]
pub trait RpcProvider: Send + Sync {
    type Block;
    type Transaction;

    async fn get_latest_block_number(&self) -> Result<u64, String>;
    async fn get_block_by_number(&self, number: u64) -> Result<Option<Self::Block>, String>;
    async fn get_transaction_receipt(
        &self,
        tx_hash: &str,
    ) -> Result<Option<Self::Transaction>, String>;
    async fn call(&self, to: &str, data: &[u8]) -> Result<Vec<u8>, String>;
}

/// Builder for creating new chain implementations
#[derive(Debug, Default)]
pub struct ChainBuilder {
    rpc_urls: Vec<String>,
    chain_id: Option<u64>,
    spammer_config: Option<SpammerConfig>,
}

impl ChainBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_rpc_urls(mut self, urls: Vec<String>) -> Self {
        self.rpc_urls = urls;
        self
    }

    pub fn with_chain_id(mut self, chain_id: u64) -> Self {
        self.chain_id = Some(chain_id);
        self
    }

    pub fn with_tps(mut self, tps: u32) -> Self {
        if let Some(ref mut config) = self.spammer_config {
            config.target_tps = tps;
        } else {
            self.spammer_config = Some(SpammerConfig {
                rpc_url: String::new(),
                chain_id: self.chain_id.unwrap_or(1),
                target_tps: tps,
            });
        }
        self
    }

    pub fn build_evm(self) -> Result<EvmChainAdapter, String> {
        let config = self.spammer_config.unwrap_or(SpammerConfig {
            rpc_url: self.rpc_urls.first().cloned().unwrap_or_default(),
            chain_id: self.chain_id.unwrap_or(1),
            target_tps: 10,
        });

        Ok(EvmChainAdapter::new(config, self.rpc_urls))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_builder_default() {
        let builder = ChainBuilder::new();
        assert!(builder.rpc_urls.is_empty());
        assert!(builder.chain_id.is_none());
    }

    #[test]
    fn test_chain_builder_with_chain_id() {
        let builder = ChainBuilder::new().with_chain_id(137);
        assert_eq!(builder.chain_id, Some(137));
    }
}
