use crate::config::EvmConfig;
use crate::utils::gas::GasManager;
use ethers::prelude::*;
use std::sync::Arc;

pub mod t01_check_balance;
pub mod t02_simple_eth_transfer;

pub use core_logic::traits::{Task, TaskResult};

#[derive(Clone, Debug)]
pub struct TaskContext {
    pub provider: Arc<Provider<Http>>,
    pub wallet: LocalWallet,
    pub config: EvmConfig,
    pub proxy: Option<String>,
    pub db: Option<Arc<core_logic::database::DatabaseManager>>,
    pub gas_manager: Arc<GasManager>,
}

pub type RobinhoodTask = dyn Task<TaskContext> + Send + Sync;
