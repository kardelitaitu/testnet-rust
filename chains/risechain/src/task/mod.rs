use crate::config::RiseConfig;
use ethers::prelude::*;

pub mod t01_check_balance;
pub mod t02_simple_eth_transfer;
pub mod t03_deploy_contract;
pub mod t04_interact_contract;
pub mod t05_self_transfer;
pub mod t06_send_meme;
pub mod t07_create_meme;
pub mod t09_weth_wrap;
pub mod t10_weth_unwrap;
pub mod t11_batch_transfer;
pub mod t12_nft_mint;
pub mod t13_nft_transfer;
pub mod t14_approve_token;
pub mod t16_multicall;
pub mod t17_read_oracle;
pub mod t18_contract_call_raw;
pub mod t19_high_gas_limit;
pub mod t20_gas_price_test;
pub mod t21_erc1155_mint;
pub mod t22_erc1155_transfer;
pub mod t23_timed_interaction;
pub mod t24_create2_deploy;
pub mod t25_message_sign;
pub mod t26_verify_signature;
pub mod t27_permit_token;
pub mod t28_delegatecall;
pub mod t29_cross_contract_call;
pub mod t30_revert_test;
pub mod t31_event_emission;
pub mod t32_eth_with_data;
pub mod t33_batch_approve;
pub mod t34_role_based_access;
pub mod t35_pausable_contract;
pub mod t36_create2_factory;
pub mod t37_uups_proxy;
pub mod t38_transparent_proxy;
pub mod t39_uniswap_v2_swap;
pub mod t40_erc4626_vault;
pub mod t41_flash_loan;
pub mod t42_erc721_mint;
pub mod t43_erc1155_batch;
pub mod t44_storage_pattern;
pub mod t45_custom_error;
pub mod t46_revert_reason;
pub mod t47_assert_fail;
pub mod t48_anonymous_event;
pub mod t49_indexed_topics;
pub mod t50_large_event;
pub mod t51_memory_expansion;
pub mod t52_calldata_size;
pub mod t53_gas_stipend;
pub mod t54_gas_price_zero;
pub mod t55_block_hash;
pub mod t57_eip7702_explore;
pub mod t58_verify_create2;
pub mod t59_deploy_factory;
pub mod t60_rise_to_weth;

pub use self::t18_contract_call_raw::ContractCallRawTask;
pub use self::t19_high_gas_limit::HighGasLimitTask;
pub use self::t20_gas_price_test::GasPriceTestTask;
pub use self::t21_erc1155_mint::Erc1155MintTask;
pub use self::t22_erc1155_transfer::Erc1155TransferTask;
pub use self::t23_timed_interaction::TimedInteractionTask;
pub use self::t24_create2_deploy::Create2DeployTask;
pub use self::t25_message_sign::MessageSignTask;
pub use self::t26_verify_signature::VerifySignatureTask;
pub use self::t27_permit_token::PermitTokenTask;
pub use self::t28_delegatecall::DelegatecallTask;
pub use self::t29_cross_contract_call::CrossContractCallTask;
pub use self::t30_revert_test::RevertTestTask;
pub use self::t31_event_emission::EventEmissionTask;
pub use self::t32_eth_with_data::EthWithDataTask;
pub use self::t33_batch_approve::BatchApproveTask;
pub use self::t34_role_based_access::RoleBasedAccessTask;
pub use self::t35_pausable_contract::PausableContractTask;
pub use self::t36_create2_factory::Create2FactoryTask;
pub use self::t37_uups_proxy::UUPSProxyTask;
pub use self::t38_transparent_proxy::TransparentProxyTask;
pub use self::t39_uniswap_v2_swap::UniswapV2SwapTask;
pub use self::t40_erc4626_vault::ERC4626VaultTask;
pub use self::t41_flash_loan::FlashLoanTestTask;
pub use self::t42_erc721_mint::ERC721MintTask;
pub use self::t43_erc1155_batch::ERC1155BatchTask;
pub use self::t44_storage_pattern::StoragePatternTask;
pub use self::t45_custom_error::CustomErrorTestTask;
pub use self::t46_revert_reason::RevertWithReasonTask;
pub use self::t47_assert_fail::AssertFailTask;
pub use self::t48_anonymous_event::AnonymousEventTask;
pub use self::t49_indexed_topics::IndexedTopicsTask;
pub use self::t50_large_event::LargeEventDataTask;
pub use self::t51_memory_expansion::MemoryExpansionTask;
pub use self::t52_calldata_size::CalldataSizeTask;
pub use self::t53_gas_stipend::GasStipendTask;
pub use self::t54_gas_price_zero::GasPriceZeroTask;
pub use self::t55_block_hash::BlockHashUsageTask;
pub use self::t57_eip7702_explore::Eip7702ExploreTask;
pub use self::t58_verify_create2::VerifyCreate2Task;
pub use self::t59_deploy_factory::DeployFactoryTask;
pub use self::t60_rise_to_weth::RiseToWethTask;

pub use core_logic::traits::{Task, TaskResult};

#[derive(Clone, Debug)]
pub struct TaskContext {
    pub provider: Provider<Http>,
    pub wallet: LocalWallet,
    pub config: RiseConfig,
    pub proxy: Option<String>,
    pub db: Option<std::sync::Arc<core_logic::database::DatabaseManager>>,
    pub gas_manager: std::sync::Arc<crate::utils::gas::GasManager>,
}

// Trait alias
pub type RiseTask = dyn Task<TaskContext> + Send + Sync;
