use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct TaskContext {
    pub provider: Provider<Http>,
    pub wallet: LocalWallet,
    pub config: super::config::SepoliaConfig,
    pub proxy: Option<String>,
    pub db: Option<Arc<core_logic::database::DatabaseManager>>,
    pub gas_manager: Arc<crate::utils::gas::GasManager>,
}

pub struct TaskResult {
    pub success: bool,
    pub message: String,
}

#[async_trait]
pub trait SepoliaTask: Send + Sync {
    fn name(&self) -> &str;
    async fn run(&self, ctx: TaskContext) -> anyhow::Result<TaskResult>;
}

pub mod t01_check_balance;
pub mod t02_mint_usdt_plus;
pub mod t03_mint_usdc_plus;
pub mod t04_redeem_usdt_plus;
pub mod t05_redeem_usdc_plus;
pub mod t06_stake_usdt_plus;
pub mod t07_stake_usdc_plus;
pub mod t08_unstake_tplus;
pub mod t09_unstake_cplus;
pub mod t10_aave_usdt_faucet;
pub mod t11_aave_usdc_faucet;
pub mod t12_bridge_tplus;
pub mod t13_bridge_cplus;

pub use t01_check_balance::SepoliaCheckBalanceTask;
pub use t02_mint_usdt_plus::MintUsdtPlusTask;
pub use t03_mint_usdc_plus::MintUsdcPlusTask;
pub use t04_redeem_usdt_plus::RedeemUsdtPlusTask;
pub use t05_redeem_usdc_plus::RedeemUsdcPlusTask;
pub use t06_stake_usdt_plus::StakeUsdtPlusTask;
pub use t07_stake_usdc_plus::StakeUsdcPlusTask;
pub use t08_unstake_tplus::UnstakeTplusTask;
pub use t09_unstake_cplus::UnstakeCplusTask;
pub use t10_aave_usdt_faucet::AaveUsdtFaucetTask;
pub use t11_aave_usdc_faucet::AaveUsdcFaucetTask;
pub use t12_bridge_tplus::BridgeTplusTask;
pub use t13_bridge_cplus::BridgeCplusTask;
