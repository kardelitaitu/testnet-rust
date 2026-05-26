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
    fn weight(&self) -> u32 {
        1
    }
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
pub mod t14_send_random_usdt_plus;
pub mod t15_send_random_usdc_plus;
pub mod t16_bridge_back_tplus;
pub mod t17_bridge_back_cplus;
pub mod t18_receive_tplus;
pub mod t19_receive_cplus;
pub mod t20_aave_wbtc_faucet;
pub mod t21_redeem_to_ausdt;
pub mod t22_redeem_to_ausdc;

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
pub use t14_send_random_usdt_plus::SendRandomUsdtPlusTask;
pub use t15_send_random_usdc_plus::SendRandomUsdcPlusTask;
pub use t16_bridge_back_tplus::BridgeBackTplusTask;
pub use t17_bridge_back_cplus::BridgeBackCplusTask;
pub use t18_receive_tplus::ReceiveTplusTask;
pub use t19_receive_cplus::ReceiveCplusTask;
pub use t20_aave_wbtc_faucet::AaveWbtcFaucetTask;
pub use t21_redeem_to_ausdt::RedeemToAusdtTask;
pub use t22_redeem_to_ausdc::RedeemToAusdcTask;
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_names_are_correct_and_unique() {
        let tasks: Vec<Box<dyn SepoliaTask>> = vec![
            Box::new(SepoliaCheckBalanceTask),
            Box::new(MintUsdtPlusTask),
            Box::new(MintUsdcPlusTask),
            Box::new(RedeemUsdtPlusTask),
            Box::new(RedeemUsdcPlusTask),
            Box::new(StakeUsdtPlusTask),
            Box::new(StakeUsdcPlusTask),
            Box::new(UnstakeTplusTask),
            Box::new(UnstakeCplusTask),
            Box::new(AaveUsdtFaucetTask),
            Box::new(AaveUsdcFaucetTask),
            Box::new(BridgeTplusTask),
            Box::new(BridgeCplusTask),
            Box::new(SendRandomUsdtPlusTask),
            Box::new(SendRandomUsdcPlusTask),
            Box::new(BridgeBackTplusTask),
            Box::new(BridgeBackCplusTask),
            Box::new(ReceiveTplusTask),
            Box::new(ReceiveCplusTask),
            Box::new(AaveWbtcFaucetTask),
            Box::new(RedeemToAusdtTask),
            Box::new(RedeemToAusdcTask),
        ];

        let expected = [
            "01_checkBalance",
            "02_mintUsdtPlus",
            "03_mintUsdcPlus",
            "04_redeemUsdtPlus",
            "05_redeemUsdcPlus",
            "06_stakeUsdtPlus",
            "07_stakeUsdcPlus",
            "08_unstakeTplus",
            "09_unstakeCplus",
            "10_aaveUsdtFaucet",
            "11_aaveUsdcFaucet",
            "12_bridgeTplus",
            "13_bridgeCplus",
            "14_sendRandomUsdtPlus",
            "15_sendRandomUsdcPlus",
            "16_bridgeBackTplus",
            "17_bridgeBackCplus",
            "18_receiveTplus",
            "19_receiveCplus",
            "20_aaveWbtcFaucet",
            "21_redeemToAusdt",
            "22_redeemToAusdc",
        ];
        assert_eq!(tasks.len(), expected.len(), "tasks len != expected len");

        let mut seen = std::collections::HashSet::new();
        for (i, task) in tasks.iter().enumerate() {
            let name = task.name();
            assert_eq!(name, expected[i], "Task {} name mismatch", i);
            assert!(seen.insert(name), "Duplicate task name: {}", name);
        }
    }

    #[test]
    fn test_task_name_prefix_format() {
        let tasks: Vec<Box<dyn SepoliaTask>> = vec![
            Box::new(SepoliaCheckBalanceTask),
            Box::new(MintUsdtPlusTask),
            Box::new(MintUsdcPlusTask),
            Box::new(RedeemUsdtPlusTask),
            Box::new(RedeemUsdcPlusTask),
            Box::new(StakeUsdtPlusTask),
            Box::new(StakeUsdcPlusTask),
            Box::new(UnstakeTplusTask),
            Box::new(UnstakeCplusTask),
            Box::new(AaveUsdtFaucetTask),
            Box::new(AaveUsdcFaucetTask),
            Box::new(BridgeTplusTask),
            Box::new(BridgeCplusTask),
            Box::new(SendRandomUsdtPlusTask),
            Box::new(SendRandomUsdcPlusTask),
            Box::new(BridgeBackTplusTask),
            Box::new(BridgeBackCplusTask),
        ];

        for task in &tasks {
            let name = task.name();
            // Names should start with a 2-digit number followed by underscore
            assert!(name.len() >= 3, "Task name '{}' too short", name);
            let prefix = &name[..2];
            let num: u32 = prefix
                .parse()
                .unwrap_or_else(|_| panic!("Task '{}' doesn't start with 2-digit number", name));
            assert!(
                num >= 1 && num <= 99,
                "Task '{}' prefix {} out of range",
                name,
                num
            );
            // Third char should be underscore
            assert_eq!(
                name.as_bytes()[2],
                b'_',
                "Task '{}' missing underscore separator",
                name
            );
        }
    }

    #[test]
    fn test_task_name_camel_case_after_prefix() {
        let tasks: Vec<Box<dyn SepoliaTask>> = vec![
            Box::new(SepoliaCheckBalanceTask),
            Box::new(MintUsdtPlusTask),
            Box::new(MintUsdcPlusTask),
            Box::new(RedeemUsdtPlusTask),
            Box::new(RedeemUsdcPlusTask),
            Box::new(StakeUsdtPlusTask),
            Box::new(StakeUsdcPlusTask),
            Box::new(UnstakeTplusTask),
            Box::new(UnstakeCplusTask),
            Box::new(AaveUsdtFaucetTask),
            Box::new(AaveUsdcFaucetTask),
            Box::new(BridgeTplusTask),
            Box::new(BridgeCplusTask),
            Box::new(SendRandomUsdtPlusTask),
            Box::new(SendRandomUsdcPlusTask),
            Box::new(BridgeBackTplusTask),
            Box::new(BridgeBackCplusTask),
        ];

        for task in &tasks {
            let name = task.name();
            // After "XX_" prefix, the name should start with a lowercase letter (camelCase)
            let body = &name[3..];
            let first_body = body.chars().next().unwrap();
            assert!(
                first_body.is_ascii_lowercase(),
                "Task '{}' body '{}' should start with lowercase (camelCase)",
                name,
                body
            );
        }
    }
}
