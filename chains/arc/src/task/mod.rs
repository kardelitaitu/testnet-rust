use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct TaskContext {
    pub provider: Provider<Http>,
    pub wallet: LocalWallet,
    pub config: super::config::ArcConfig,
    pub proxy: Option<String>,
    pub db: Option<Arc<core_logic::database::DatabaseManager>>,
    pub gas_manager: Arc<crate::utils::gas::GasManager>,
}

pub struct TaskResult {
    pub success: bool,
    pub message: String,
}

#[async_trait]
pub trait ArcTask: Send + Sync {
    fn name(&self) -> &str;
    fn weight(&self) -> u32 {
        1
    }
    async fn run(&self, ctx: TaskContext) -> anyhow::Result<TaskResult>;
}

pub mod t01_check_balance;
pub mod t02_send_usdc;
pub mod t03_send_eurc;
pub mod t04_send_cirbtc;

pub use t01_check_balance::ArcCheckBalanceTask;
pub use t02_send_usdc::SendUsdcTask;
pub use t03_send_eurc::SendEurcTask;
pub use t04_send_cirbtc::SendCirbtcTask;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_names_are_correct() {
        let tasks: Vec<Box<dyn ArcTask>> = vec![
            Box::new(ArcCheckBalanceTask),
            Box::new(SendUsdcTask),
            Box::new(SendEurcTask),
            Box::new(SendCirbtcTask),
        ];
        let expected = [
            "01_checkBalance",
            "02_sendUsdc",
            "03_sendEurc",
            "04_sendCirbtc",
        ];
        assert_eq!(tasks.len(), expected.len());
        for (i, task) in tasks.iter().enumerate() {
            assert_eq!(task.name(), expected[i], "Task {} name mismatch", i);
        }
    }

    #[test]
    fn test_task_names_unique() {
        let tasks: Vec<Box<dyn ArcTask>> = vec![
            Box::new(ArcCheckBalanceTask),
            Box::new(SendUsdcTask),
            Box::new(SendEurcTask),
            Box::new(SendCirbtcTask),
        ];
        let mut seen = std::collections::HashSet::new();
        for task in &tasks {
            assert!(
                seen.insert(task.name()),
                "Duplicate task name: {}",
                task.name()
            );
        }
    }

    #[test]
    fn test_task_name_prefix_format() {
        let tasks: Vec<Box<dyn ArcTask>> = vec![
            Box::new(ArcCheckBalanceTask),
            Box::new(SendUsdcTask),
            Box::new(SendEurcTask),
            Box::new(SendCirbtcTask),
        ];
        for task in &tasks {
            let name = task.name();
            assert!(name.len() >= 3, "Task '{}' too short", name);
            let _: u32 = name[..2]
                .parse()
                .expect("Task name should start with 2-digit number");
            assert_eq!(
                name.as_bytes()[2],
                b'_',
                "Task '{}' missing separator",
                name
            );
        }
    }
}
