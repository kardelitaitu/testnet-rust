use crate::config::TempoConfig;
use crate::utils::gas_manager::GasManager;
use crate::TempoProvider;
use anyhow::Result;
use async_trait::async_trait;
use core_logic::database::DatabaseManager;
use core_logic::traits::TaskResult;
use ethers::prelude::*;
use std::sync::Arc;

#[derive(Clone)]
pub struct TaskContext {
    pub provider: Arc<TempoProvider>,
    pub wallet: LocalWallet,
    pub config: TempoConfig,
    pub gas_manager: Arc<GasManager>,
    pub db: Option<Arc<DatabaseManager>>,
}

#[async_trait]
pub trait TempoTask: Send + Sync {
    fn name(&self) -> &str;
    async fn run(&self, ctx: TaskContext) -> Result<TaskResult>;
}

pub mod t01_deploy_contract;
pub mod t02_claim_faucet;
pub mod t03_send_token;
pub mod t04_create_stable;
pub mod t05_swap_stable;
pub mod t06_add_liquidity;
pub mod t07_mint_stable;
pub mod t08_burn_stable;
pub mod t09_transfer_token;
pub mod t10_transfer_memo;
pub mod t11_limit_order;
pub mod t12_remove_liquidity;
pub mod t13_grant_role;
pub mod t14_nft_create_mint;
pub mod t15_mint_domain;
pub mod t16_retrieve_nft;
pub mod t17_batch_eip7702;
pub mod t18_tip403_policies;
pub mod t19_wallet_analytics;
pub mod t20_wallet_activity;
pub mod t21_create_meme;
pub mod t22_mint_meme;
pub mod t23_transfer_meme;
pub mod t24_batch_swap;
pub mod t25_batch_system_token;
pub mod t26_batch_stable_token;
pub mod t27_batch_meme_token;
pub mod t28_multi_send_disperse;
pub mod t29_multi_send_disperse_stable;
pub mod t30_multi_send_disperse_meme;
pub mod t31_multi_send_concurrent;
pub mod t32_multi_send_concurrent_stable;
pub mod t33_multi_send_concurrent_meme;
pub mod t34_batch_send_transaction;
pub mod t35_batch_send_transaction_stable;
pub mod t36_batch_send_transaction_meme;
pub mod t37_transfer_later;
pub mod t38_transfer_later_stable;
pub mod t39_transfer_later_meme;
pub mod t40_distribute_shares;
pub mod t41_distribute_shares_stable;
pub mod t42_distribute_shares_meme;
pub mod t43_batch_mint_stable;
pub mod t44_batch_mint_meme;
pub mod t45_deploy_viral_faucet;
pub mod t46_claim_viral_faucet;
pub mod t47_deploy_viral_nft;
pub mod t48_mint_viral_nft;
pub mod t49_time_bomb;
pub mod t50_deploy_storm;
