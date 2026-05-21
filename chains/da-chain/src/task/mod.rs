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

pub use t01_check_balance::DaChainCheckBalanceTask;
pub use t02_simple_native_transfer::SimpleNativeTransferTask;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_names_are_correct() {
        let tasks: Vec<Box<dyn DaChainTask>> = vec![
            Box::new(DaChainCheckBalanceTask),
            Box::new(SimpleNativeTransferTask),
        ];
        let expected = ["01_checkBalance", "02_simpleNativeTransfer"];
        assert_eq!(tasks.len(), expected.len());
        for (i, task) in tasks.iter().enumerate() {
            assert_eq!(task.name(), expected[i], "Task {} name mismatch", i);
        }
    }

    #[test]
    fn test_task_names_unique() {
        let tasks: Vec<Box<dyn DaChainTask>> = vec![
            Box::new(DaChainCheckBalanceTask),
            Box::new(SimpleNativeTransferTask),
        ];
        let mut seen = std::collections::HashSet::new();
        for task in &tasks {
            assert!(seen.insert(task.name()), "Duplicate task name: {}", task.name());
        }
    }

    #[test]
    fn test_task_name_prefix_format() {
        let tasks: Vec<Box<dyn DaChainTask>> = vec![
            Box::new(DaChainCheckBalanceTask),
            Box::new(SimpleNativeTransferTask),
        ];
        for task in &tasks {
            let name = task.name();
            assert!(name.len() >= 3, "Task '{}' too short", name);
            // First two chars should be a 2-digit number
            let _: u32 = name[..2].parse().expect("Task name should start with 2-digit number");
            // Third char should be underscore
            assert_eq!(name.as_bytes()[2], b'_', "Task '{}' missing separator", name);
        }
    }
}
