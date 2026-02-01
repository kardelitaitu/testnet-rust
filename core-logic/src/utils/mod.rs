//! # Utilities Module
//!
//! Internal utility modules for the core-logic crate.
//! These modules are marked as `pub(crate)` to enforce API boundaries.

// Internal modules - not part of public API
pub(crate) mod gas;
pub(crate) mod logger;
pub(crate) mod proxy_manager;
pub(crate) mod rate_limiter;
pub(crate) mod retry;
pub(crate) mod rpc_manager;
pub(crate) mod runner;
pub(crate) mod wallet_manager;

// Selective exports - only public utilities
pub use gas::GasConfig;
pub use logger::setup_logger;
pub use proxy_manager::ProxyManager;
pub use rpc_manager::RpcManager;
pub use runner::WorkerRunner;
pub use wallet_manager::WalletManager;
