//! Tasks module for tempo-alloy client
//!
//! Contains the task trait and implementations for various blockchain operations.

use crate::provider::TempoClient;
use alloy::primitives::U256;
use anyhow::Result;
use async_trait::async_trait;
use core_logic::database::DatabaseManager;
use std::sync::Arc;
use std::time::Duration;

pub use core_logic::traits::TaskResult;

/// Task context passed to all tasks
#[derive(Debug, Clone)]
pub struct TaskContext {
    /// The Tempo client
    pub client: TempoClient,
    /// Optional database manager
    pub db: Option<Arc<DatabaseManager>>,
    /// Gas manager for gas estimation
    pub gas_manager: Arc<GasManager>,
    /// Task timeout
    pub timeout: Duration,
}

impl TaskContext {
    /// Create a new task context
    pub fn new(client: TempoClient, db: Option<Arc<DatabaseManager>>) -> Self {
        Self {
            client,
            db,
            gas_manager: Arc::new(GasManager),
            timeout: Duration::from_secs(60),
        }
    }

    /// Get the wallet address
    #[inline]
    pub fn address(&self) -> alloy::primitives::Address {
        self.client.address()
    }

    /// Get the chain ID
    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.client.chain_id()
    }
}

/// Task trait for tempo operations
#[async_trait]
pub trait TempoTask: Send + Sync {
    /// Returns the task name
    fn name(&self) -> &'static str;
    /// Run the task with the given context
    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult>;
}

/// Gas management utilities
#[derive(Debug, Default, Clone)]
pub struct GasManager;

impl GasManager {
    /// Estimate current gas price
    pub async fn estimate_gas(&self, client: &TempoClient) -> Result<U256> {
        let gas_price = client.provider.get_gas_price().await?;
        Ok(gas_price)
    }

    /// Bump gas fees by percentage (default 20%)
    pub fn bump_fees(&self, gas_price: U256, percent: u64) -> U256 {
        let multiplier = U256::from(100 + percent);
        let divisor = U256::from(100);
        gas_price * multiplier / divisor
    }

    /// Get priority fee (use current gas price as priority)
    pub fn priority_fee(&self, gas_price: U256) -> U256 {
        // For Tempo, we use the full gas price as priority
        gas_price
    }
}

/// Task utilities
pub mod prelude {
    pub use super::{TaskContext, TempoTask};
    pub use crate::utils::{get_random_address, load_config, Config};
}

/// Task modules
pub mod t01_deploy_contract;
pub mod t02_claim_faucet;
pub mod t03_send_token;
pub mod t04_create_stable;
pub mod t05_swap_stable;
