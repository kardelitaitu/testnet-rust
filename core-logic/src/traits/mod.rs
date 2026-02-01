use crate::config::SpamConfig;
use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Default, Clone)]
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

#[derive(Debug, Clone)]
pub struct TaskResult {
    pub success: bool,
    pub message: String,
    pub tx_hash: Option<String>,
}

#[async_trait]
pub trait Task<Ctx>: Send + Sync {
    /// Returns the name of the task
    fn name(&self) -> &str;

    /// Executes the task
    async fn run(&self, ctx: Ctx) -> Result<TaskResult>;
}

#[async_trait]
pub trait WalletLoader: Send + Sync {
    type Wallet;

    /// Load wallets from a source (encrypted file, etc.)
    async fn load_wallets(&self) -> Result<Vec<Self::Wallet>>;
}
