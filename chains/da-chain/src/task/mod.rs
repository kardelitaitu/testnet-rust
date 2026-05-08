use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct TaskContext {
    pub provider: Provider<Http>,
    pub wallet: LocalWallet,
    pub config: super::config::DaChainConfig,
    pub proxy: Option<String>,
    pub db: Option<Arc<core_logic::database::DatabaseManager>>,
    pub gas_manager: Arc<crate::utils::gas::GasManager>,
}

pub struct TaskResult {
    pub success: bool,
    pub message: String,
}

#[async_trait]
pub trait DaChainTask: Send + Sync {
    fn name(&self) -> &str;
    async fn run(&self, ctx: TaskContext) -> anyhow::Result<TaskResult>;
}

pub mod t01_check_balance;
pub mod t02_simple_native_transfer;
pub mod t03_deploy_contract;
pub mod t04_interact_contract;
pub mod t05_self_transfer;

pub use t01_check_balance::DaChainCheckBalanceTask;
pub use t02_simple_native_transfer::SimpleNativeTransferTask;
pub use t03_deploy_contract::DaChainDeployContractTask;
pub use t04_interact_contract::DaChainInteractContractTask;
pub use t05_self_transfer::SelfTransferTask;
