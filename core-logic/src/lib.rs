//! # Core Logic - Shared Utilities for Testnet Framework
//!
//! This crate provides shared utilities used across all chain implementations.
//! It includes database management, wallet handling, configuration, and more.
//!
//! ## Modules
//!
//! - [`config`] - Configuration structures for spammer setup
//! - [`database`] - Async SQLite database with connection pooling
//! - [`error`] - Typed error handling with thiserror
//! - [`metrics`] - Performance metrics collection
//! - [`security`] - Encryption and security utilities
//! - [`templates`] - Chain adapter templates
//! - [`traits`] - Core trait definitions
//! - [`utils`] - Utility modules (wallet, proxy, gas management)

// Module declarations - internal modules marked pub(crate)
pub mod config;
pub mod database;
pub mod error;
pub mod metrics;
pub mod security;
pub mod templates;
pub mod traits;
pub(crate) mod utils;

// Selective exports - only public API types
pub use config::{ChainConfig, ProxyConfig, SpamConfig, WalletSource};
pub use database::{
    AsyncDbConfig, DatabaseManager, DbMetrics, DbMetricsSnapshot, DexOrder, FallbackStrategy,
    QueuedTaskResult, TaskMetricBatchItem,
};
pub use error::{ConfigError, CoreError, DatabaseError, NetworkError, SecurityError, WalletError};
pub use metrics::{MetricsCollector, MetricsSnapshot};
pub use security::SecurityUtils;
pub use templates::{
    ChainBuilder, ChainSpammer, EvmChainAdapter, GasEstimator, RpcProvider, SpammerConfig,
    SpammerResult, TransactionSigner,
};
pub use traits::{Spammer as SpammerTrait, SpammerStats, Task, TaskResult, WalletLoader};

// Utils are pub(crate) - only export specific public utilities
pub use utils::{
    generate_random_address, setup_logger, ExplorerGasSnapshot, ExplorerGasTracker,
    ExplorerGasTrackerPayload, GasConfig, ProxyHealthManager, ProxyManager, ProxyRateLimiter,
    RpcManager, WalletManager, ChainType, WorkerRunner,
};

// Export memory optimization utilities
pub use utils::memory_monitor::{
    get_memory_report, init_memory_monitoring, sample_memory_usage, MemoryMonitor,
    MemoryMonitorConfig, MemoryStats, MemoryTrend,
};
pub use utils::memory_optimized_logger::{
    setup_memory_optimized_logger, MemoryOptimizedFileAppender, MemoryOptimizedLayer,
    MemoryOptimizedLoggerConfig,
};
pub use utils::memory_optimizer::{
    get_memory_status, init_memory_optimization, perform_memory_cleanup,
    register_memory_cleanup_hook, MemoryOptimizer, MemoryOptimizerConfig,
};

// Export retry and rate-limiter utilities for testing
pub use utils::{
    is_transient_error, with_retry, CircuitBreaker, CircuitBreakerConfig, RetryConfig,
    TokenBucket,
};

/// Log an error message and immediately exit the process with code 1.
///
/// Use this in binary entry points where a failure should terminate the process
/// with a non-zero exit code. Library code should return `Result` instead.
///
/// # Example
///
/// ```no_run
/// exit_with_error("Failed to load config: {}", "file not found");
/// ```
pub fn exit_with_error(msg: impl std::fmt::Display) -> ! {
    tracing::error!("{}", msg);
    std::process::exit(1);
}
