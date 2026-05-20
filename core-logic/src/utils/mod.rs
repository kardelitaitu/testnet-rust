//! # Utilities Module
//!
//! Internal utility modules for the core-logic crate.
//! These modules are marked as `pub(crate)` to enforce API boundaries.

// Internal modules - not part of public API
pub(crate) mod address_gen;
pub(crate) mod gas;
pub(crate) mod explorer_gas_tracker;
pub(crate) mod logger;
pub mod memory_monitor;
pub mod memory_optimized_logger;
pub mod memory_optimizer;
pub(crate) mod proxy_manager;
pub mod proxy_health;
pub mod proxy_rate_limiter;
pub(crate) mod rate_limiter;
pub(crate) mod retry;
pub(crate) mod rpc_manager;
pub(crate) mod runner;
pub(crate) mod wallet_manager;

// Selective exports - only public utilities
pub use address_gen::generate_random_address;
pub use gas::GasConfig;
pub use explorer_gas_tracker::{ExplorerGasSnapshot, ExplorerGasTracker, ExplorerGasTrackerPayload};
pub use logger::setup_logger;
pub use proxy_health::ProxyHealthManager;
pub use proxy_manager::ProxyManager;
pub use proxy_rate_limiter::ProxyRateLimiter;
pub use rpc_manager::RpcManager;
pub use runner::WorkerRunner;
pub use wallet_manager::WalletManager;
