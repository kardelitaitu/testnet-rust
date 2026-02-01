//! Tempo EVM
//!
//! EVM configuration and execution for the Tempo blockchain.

use alloy_evm::{Database, EvmEnv};
use alloy_primitives::Address;
use reth_chainspec::EthChainSpec;
use reth_evm::{ConfigureEvm, EvmEnvFor};
use std::sync::Arc;
use tempo_protocol::{TempoHeader, TempoTxEnvelope};

/// Tempo EVM configuration
#[derive(Debug, Clone, Default)]
pub struct TempoEvmConfig;

impl TempoEvmConfig {
    /// Create new config
    pub fn new() -> Self {
        Self
    }
}

/// Block environment for Tempo
#[derive(Debug, Clone)]
pub struct TempoBlockEnv {
    /// Inner block environment
    pub inner: alloy_evm::BlockEnv,
    /// Milliseconds part of timestamp (Tempo-specific)
    pub timestamp_millis_part: u64,
}

impl Default for TempoBlockEnv {
    fn default() -> Self {
        Self {
            inner: alloy_evm::BlockEnv::default(),
            timestamp_millis_part: 0,
        }
    }
}

/// State access for Tempo
#[derive(Debug, Clone)]
pub struct TempoStateAccess;

impl ConfigureEvm for TempoEvmConfig {
    type Primitives = ();
    type Error = alloy_evm::Error;
    type NextBlockEnvCtx = ();
    type BlockExecutorFactory = Self;
    type BlockAssembler = ();

    fn evm_env(&self, _header: &TempoHeader) -> Result<EvmEnvFor<Self>, Self::Error> {
        // Simplified for client use - full implementation in tempo/crates/evm
        Ok(EvmEnv {
            cfg_env: alloy_evm::CfgEnv::default(),
            block_env: alloy_evm::BlockEnv::default(),
        })
    }
}
