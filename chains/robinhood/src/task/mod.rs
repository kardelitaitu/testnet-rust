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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_names_are_correct() {
        let tasks: Vec<Box<RobinhoodTask>> = vec![
            Box::new(t01_check_balance::CheckBalanceTask),
            Box::new(t02_simple_eth_transfer::SimpleEthTransferTask),
        ];
        let expected = ["01_checkBalance", "02_simpleEthTransfer"];
        assert_eq!(tasks.len(), expected.len());
        for (i, task) in tasks.iter().enumerate() {
            assert_eq!(task.name(), expected[i], "Task {} name mismatch", i);
        }
    }

    #[test]
    fn test_task_names_unique() {
        let tasks: Vec<Box<RobinhoodTask>> = vec![
            Box::new(t01_check_balance::CheckBalanceTask),
            Box::new(t02_simple_eth_transfer::SimpleEthTransferTask),
        ];
        let mut seen = std::collections::HashSet::new();
        for task in &tasks {
            assert!(seen.insert(task.name()), "Duplicate task name: {}", task.name());
        }
    }

    #[test]
    fn test_task_name_prefix_format() {
        let tasks: Vec<Box<RobinhoodTask>> = vec![
            Box::new(t01_check_balance::CheckBalanceTask),
            Box::new(t02_simple_eth_transfer::SimpleEthTransferTask),
        ];
        for task in &tasks {
            let name = task.name();
            assert!(name.len() >= 3, "Task '{}' too short", name);
            let _: u32 = name[..2].parse().expect("Task should start with 2-digit number");
            assert_eq!(name.as_bytes()[2], b'_', "Task '{}' missing separator", name);
        }
    }
}
