//! Tempo Spammer - High-performance transaction spammer for Tempo blockchain
//!
//! A multi-worker transaction spammer built with Alloy 1.4.3 for the Tempo blockchain,
//! featuring 50+ task implementations, proxy rotation, nonce management, and comprehensive
//! testnet coverage.
//!
//! # Features
//!
//! - **50+ Task Implementations**: Comprehensive coverage including token operations,
//!   DEX interactions, batch transactions, NFTs, and viral mechanics
//! - **Multi-Wallet Support**: Automatic wallet rotation with thread-safe leasing
//! - **Proxy Management**: Health checking, automatic failover, and rotation
//! - **Nonce Management**: Intelligent caching with automatic synchronization
//! - **Database Persistence**: SQLite logging for metrics, contracts, and assets
//! - **Alloy 1.4.3**: Modern Ethereum library with 10x faster ABI encoding
//!
//! # Architecture
//!
//! The spammer consists of several key components:
//!
//! - **[`TempoClient`]**: Alloy-based provider wrapper with proxy support
//! - **[`ClientPool`]**: Manages wallet leasing with RAII pattern and cooldown
//! - **[`NonceManager`]**: Thread-safe nonce caching per wallet address
//! - **[`ProxyBanlist`]**: Health tracking with temporary banning and recovery
//! - **Task System**: 50+ pluggable task implementations via [`TempoTask`] trait
//!
//! # Quick Start
//!
//! ```bash
//! # Run the spammer with default configuration
//! cargo run -p tempo-spammer --bin tempo-spammer
//!
//! # Run a specific task for testing
//! cargo run -p tempo-spammer --bin tempo-debug -- --task 01_deploy_contract
//!
//! # List all available tasks
//! cargo run -p tempo-spammer --bin tempo-spammer -- list
//! ```
//!
//! # Configuration
//!
//! Configuration is loaded from `config/config.toml`. See the `config` module for
//! available options.
//!
//! # Task System
//!
//! Tasks implement the [`TempoTask`] trait and are executed within a [`TaskContext`]
//! that provides access to the blockchain provider, wallet, and database.
//!
//! See the [`tasks`] module for task implementations and the task catalog at
//! `docs/TASK_CATALOG.md` for complete documentation.
//!
//! # Examples
//!
//! ## Creating a Client
//!
//! ```rust,no_run
//! use tempo_spammer::TempoClient;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let client = TempoClient::new(
//!     "https://rpc.moderato.tempo.xyz",
//!     "0x...", // private key
//!     None,    // no proxy
//!     None,    // no proxy index
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Using the Client Pool
//!
//! ```rust,no_run
//! use tempo_spammer::ClientPool;
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create pool with wallets and proxies
//! let pool = Arc::new(ClientPool::new(
//!     vec!["0x...".to_string()], // private keys
//!     vec![],                     // proxies
//!     "https://rpc.moderato.tempo.xyz",
//!     None, // no banlist
//! ).await?);
//!
//! // Acquire a client (returns automatically on drop)
//! if let Some(lease) = pool.try_acquire_client().await {
//!     let client = &lease.client;
//!     // Use client...
//! } // Client released here after 4s cooldown
//! # Ok(())
//! # }
//! ```
//!
//! # System Tokens
//!
//! The Tempo blockchain provides several system tokens (TIP-20 standard):
//!
//! - **PathUSD**: `0x20C0000000000000000000000000000000000000`
//! - **AlphaUSD**: `0x20c0000000000000000000000000000000000001`
//! - **BetaUSD**: `0x20c0000000000000000000000000000000000002`
//! - **ThetaUSD**: `0x20c0000000000000000000000000000000000003`
//!
//! # Important Contracts
//!
//! - **TIP-20 Factory**: `0x20fc000000000000000000000000000000000000`
//! - **Fee AMM DEX**: `0xdec0000000000000000000000000000000000000`
//! - **Faucet**: `0x4200000000000000000000000000000000000019`
//!
//! # Safety
//!
//! - Private keys are never logged or exposed
//! - Wallet data uses secure memory handling
//! - Proxy credentials are handled securely
//! - Database connections are pooled and limited
//!
//! # See Also
//!
//! - [Task Catalog](../docs/TASK_CATALOG.md) - Complete task reference
//! - [Architecture](../docs/ARCHITECTURE.md) - System design documentation
//! - [Configuration](../docs/CONFIG_REFERENCE.md) - Configuration options

#![allow(unused)]

pub mod bot;
pub mod client;
pub mod client_pool;
pub mod config;
pub mod nonce_manager;
pub mod proxy_health;
pub mod robust_nonce_manager;
pub mod tasks;

pub use client::TempoClient;
pub use client_pool::ClientPool;
pub use config::TempoSpammerConfig;
pub use nonce_manager::NonceManager;
pub use proxy_health::ProxyBanlist;
pub use robust_nonce_manager::{
    NonceManagerConfig, NonceReservation, NonceStats, RobustNonceManager,
};
pub use tasks::{ProxyConfig, TaskContext, TempoTask};
