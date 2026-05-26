use crate::config::SpamConfig;
use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SpammerStats {
    pub success: u64,
    pub failed: u64,
}

#[async_trait]
pub trait Spammer: Send + Sync {
    /// Initialize the spammer with configuration
    async fn new(config: SpamConfig) -> Result<Self>
    where
        Self: Sized;

    /// Start the spamming process
    async fn start(
        &self,
        cancellation_token: tokio_util::sync::CancellationToken,
    ) -> Result<SpammerStats>;

    /// Stop the spamming process
    async fn stop(&self) -> Result<()>;
}

/// Result of a single task execution.
///
/// Returned by all task implementations to report outcome and optional
/// blockchain transaction hash.
///
/// ```
/// use core_logic::traits::TaskResult;
///
/// let success = TaskResult {
///     success: true,
///     message: "Successfully transferred 100 USDC".into(),
///     tx_hash: Some("0xabc123".into()),
/// };
/// assert!(success.success);
/// assert!(success.tx_hash.is_some());
///
/// let failure = TaskResult {
///     success: false,
///     message: "Insufficient balance".into(),
///     tx_hash: None,
/// };
/// assert!(!failure.success);
/// assert!(failure.tx_hash.is_none());
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TaskResult {
    pub success: bool,
    pub message: String,
    pub tx_hash: Option<String>,
}

#[async_trait]
pub trait Task<Ctx>: Send + Sync {
    /// Returns the name of the task
    fn name(&self) -> &str;

    /// Returns the weight of the task for weighted random selection.
    /// Higher weight = higher probability of being selected.
    /// Default is 1.
    fn weight(&self) -> u32 {
        1
    }

    /// Executes the task
    async fn run(&self, ctx: Ctx) -> Result<TaskResult>;
}

#[async_trait]
pub trait WalletLoader: Send + Sync {
    type Wallet;

    /// Load wallets from a source (encrypted file, etc.)
    async fn load_wallets(&self) -> Result<Vec<Self::Wallet>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spammer_stats_default() {
        let stats = SpammerStats::default();
        assert_eq!(stats.success, 0);
        assert_eq!(stats.failed, 0);
    }

    #[test]
    fn test_spammer_stats_with_values() {
        let stats = SpammerStats {
            success: 42,
            failed: 7,
        };
        assert_eq!(stats.success, 42);
        assert_eq!(stats.failed, 7);
    }

    #[test]
    fn test_spammer_stats_clone() {
        let a = SpammerStats {
            success: 5,
            failed: 3,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn test_task_result_success() {
        let r = TaskResult {
            success: true,
            message: "done".into(),
            tx_hash: Some("0xabc".into()),
        };
        assert!(r.success);
        assert_eq!(r.message, "done");
        assert_eq!(r.tx_hash, Some("0xabc".into()));
    }

    #[test]
    fn test_task_result_failure_no_tx() {
        let r = TaskResult {
            success: false,
            message: "error".into(),
            tx_hash: None,
        };
        assert!(!r.success);
        assert!(r.tx_hash.is_none());
    }

    #[test]
    fn test_task_result_clone() {
        let a = TaskResult {
            success: true,
            message: "ok".into(),
            tx_hash: None,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }
}
