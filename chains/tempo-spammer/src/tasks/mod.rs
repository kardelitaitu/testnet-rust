//! Tasks Module - Task trait and utilities for blockchain operations
//!
//! This module defines the core task system for the tempo-spammer. It provides:
//!
//! - [`TempoTask`] trait for implementing new tasks
//! - [`TaskContext`] for accessing resources within tasks
//! - [`GasManager`] for fee estimation and management
//! - Utility functions for common operations
//!
//! # Task System Architecture
//!
//! Tasks are the fundamental unit of work in the spammer. Each task implements
//! the [`TempoTask`] trait and can be executed by the spammer workers.
//!
//! ## Task Lifecycle
//!
//! 1. **Registration**: Tasks are registered in the binary (e.g., `tempo-spammer.rs`)
//! 2. **Selection**: Worker randomly selects a task based on weights
//! 3. **Execution**: Task's `run()` method is called with a [`TaskContext`]
//! 4. **Result**: Task returns [`TaskResult`] indicating success/failure
//! 5. **Logging**: Results are logged to console and optionally to database
//!
//! # Creating a New Task
//!
//! ```rust,no_run
//! use async_trait::async_trait;
//! use tempo_spammer::tasks::{TaskContext, TempoTask, TaskResult};
//! use anyhow::Result;
//!
//! #[derive(Debug, Clone, Default)]
//! pub struct MyTask;
//!
//! #[async_trait]
//! impl TempoTask for MyTask {
//!     fn name(&self) -> &'static str {
//!         "99_my_task"
//!     }
//!
//!     async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
//!         // Access client
//!         let client = &ctx.client;
//!         let address = ctx.address();
//!
//!         // Perform operation...
//!
//!         Ok(TaskResult {
//!             success: true,
//!             message: "Task completed".to_string(),
//!             tx_hash: None,
//!         })
//!     }
//! }
//! ```
//!
//! # Task Context
//!
//! The [`TaskContext`] provides tasks with access to:
//!
//! - **client**: [`TempoClient`] for blockchain interactions
//! - **config**: [`TempoSpammerConfig`] for settings
//! - **db**: Optional [`DatabaseManager`] for persistence
//! - **gas_manager**: [`GasManager`] for fee operations
//! - **timeout**: Maximum execution duration
//!
//! # Gas Management
//!
//! The [`GasManager`] provides utilities for:
//!
//! - Estimating gas prices from the network
//! - Bumping fees by a percentage (for retries)
//! - Future: EIP-1559 fee estimation
//!
//! # Utilities
//!
//! Common utility functions provided:
//!
//! - [`get_random_address()`]: Gets random address from file or generates one
//! - [`generate_random_shares()`]: Generates random share distributions
//! - [`load_proxies()`]: Loads proxy configuration from file
//!
//! # See Also
//!
//! - [Task Catalog](../../docs/TASK_CATALOG.md) - Complete task reference
//! - [Task Development Guide](../../docs/TASK_DEVELOPMENT.md) - Creating new tasks

use crate::client::TempoClient;
use crate::config::TempoSpammerConfig;
use alloy_primitives::{Address, U256};
use anyhow::{Context, Result};
use async_trait::async_trait;
use core_logic::database::DatabaseManager;
use rand::Rng;
use rand::prelude::SliceRandom;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

pub use core_logic::traits::TaskResult;

/// Execution context provided to tasks
///
/// Contains all resources and configuration needed for task execution.
/// Passed to [`TempoTask::run`] when a task is executed.
///
/// # Fields
///
/// - `client`: Blockchain provider and signer
/// - `config`: Spammer configuration
/// - `db`: Optional database for persistence
/// - `gas_manager`: Fee estimation utilities
/// - `timeout`: Maximum execution time (default 180s)
///
/// # Example
///
/// ```rust,no_run
/// use tempo_spammer::tasks::TaskContext;
///
/// # async fn example(ctx: &TaskContext) -> anyhow::Result<()> {
/// // Access wallet address
/// let address = ctx.address();
///
/// // Access client
/// let client = &ctx.client;
///
/// // Access config
/// let chain_id = ctx.config.chain_id;
///
/// // Use database if available
/// if let Some(db) = &ctx.db {
///     // Log to database...
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct TaskContext {
    /// Blockchain client for transactions and queries
    pub client: TempoClient,
    /// Spammer configuration
    pub config: TempoSpammerConfig,
    /// Optional database manager for persistence
    pub db: Option<Arc<DatabaseManager>>,
    /// Gas fee estimation and management
    pub gas_manager: Arc<GasManager>,
    /// Maximum task execution duration
    pub timeout: Duration,
}

impl TaskContext {
    /// Creates a new task context
    ///
    /// # Arguments
    ///
    /// * `client` - The blockchain client
    /// * `config` - Spammer configuration
    /// * `db` - Optional database manager
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tempo_spammer::tasks::TaskContext;
    /// use tempo_spammer::{TempoClient, config::TempoSpammerConfig};
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let client = TempoClient::new(
    ///     "https://rpc.moderato.tempo.xyz",
    ///     "0x...",
    ///     None,
    ///     None,
    /// ).await?;
    ///
    /// let config = TempoSpammerConfig::from_path("config/config.toml")?;
    ///
    /// let ctx = TaskContext::new(client, config, None);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(
        client: TempoClient,
        config: TempoSpammerConfig,
        db: Option<Arc<DatabaseManager>>,
    ) -> Self {
        Self {
            client,
            config,
            db,
            gas_manager: Arc::new(GasManager),
            timeout: Duration::from_secs(180),
        }
    }

    /// Returns the wallet address
    ///
    /// Convenience method that delegates to the client.
    #[inline]
    pub fn address(&self) -> Address {
        self.client.address()
    }

    /// Returns the chain ID
    ///
    /// Convenience method that delegates to the client.
    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.client.chain_id()
    }
}

/// Trait for implementing tempo tasks
///
/// All tasks in the tempo-spammer implement this trait. It defines the interface
/// that the spammer uses to execute tasks.
///
/// # Implementation
///
/// Implement this trait for your custom tasks:
///
/// ```rust,no_run
/// use async_trait::async_trait;
/// use tempo_spammer::tasks::{TaskContext, TempoTask, TaskResult};
/// use anyhow::Result;
///
/// #[derive(Debug, Clone, Default)]
/// pub struct MyTask;
///
/// impl MyTask {
///     pub fn new() -> Self {
///         Self
///     }
/// }
///
/// #[async_trait]
/// impl TempoTask for MyTask {
///     fn name(&self) -> &'static str {
///         "99_my_task"
///     }
///
///     async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
///         // Task implementation...
///         Ok(TaskResult {
///             success: true,
///             message: "Completed".to_string(),
///             tx_hash: None,
///         })
///     }
/// }
/// ```
///
/// # Registration
///
/// After implementing, register your task in the binary:
///
/// ```rust,ignore
/// let tasks: Vec<Box<dyn TempoTask>> = vec![
///     Box::new(MyTask::new()),
///     // ... other tasks
/// ];
/// ```
#[async_trait]
pub trait TempoTask: Send + Sync {
    /// Returns the task name
    ///
    /// This is used for logging and identification. Use a consistent naming
    /// convention like "XX_task_name" where XX is the task number.
    fn name(&self) -> &'static str;

    /// Executes the task
    ///
    /// This is the main task logic. It receives a [`TaskContext`] with all
    /// necessary resources and should return a [`TaskResult`] indicating
    /// success or failure.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The task execution context
    ///
    /// # Returns
    ///
    /// Returns `Result<TaskResult>` where:
    /// - `Ok(TaskResult)` - Task completed (check `success` field for status)
    /// - `Err(e)` - Task failed with error
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tempo_spammer::tasks::{TaskContext, TempoTask, TaskResult};
    /// use async_trait::async_trait;
    /// use anyhow::Result;
    ///
    /// # struct MyTask;
    /// # #[async_trait]
    /// # impl TempoTask for MyTask {
    /// #     fn name(&self) -> &'static str { "example" }
    /// async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
    ///     let client = &ctx.client;
    ///
    ///     // Perform blockchain operation...
    ///
    ///     Ok(TaskResult {
    ///         success: true,
    ///         message: "Operation completed".to_string(),
    ///         tx_hash: Some("0x...".to_string()),
    ///     })
    /// }
    /// # }
    /// ```
    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult>;
}

/// Gas price estimation and fee management
///
/// Provides utilities for estimating gas prices and calculating fees.
/// Currently supports legacy gas price estimation. Future versions may
/// add EIP-1559 support.
///
/// # Example
///
/// ```rust,no_run
/// use tempo_spammer::tasks::GasManager;
/// use tempo_spammer::TempoClient;
///
/// # async fn example() -> anyhow::Result<()> {
/// let gas_manager = GasManager;
/// let client = TempoClient::new(
///     "https://rpc.moderato.tempo.xyz",
///     "0x...",
///     None,
///     None,
/// ).await?;
///
/// // Estimate current gas price
/// let gas_price = gas_manager.estimate_gas(&client).await?;
///
/// // Bump fees by 20% for faster confirmation
/// let bumped = gas_manager.bump_fees(gas_price, 20);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Default, Clone)]
pub struct GasManager;

impl GasManager {
    /// Estimates the current gas price from the network
    ///
    /// Queries the RPC for the current gas price.
    ///
    /// # Arguments
    ///
    /// * `client` - The blockchain client
    ///
    /// # Returns
    ///
    /// Returns `Result<U256>` containing the gas price in wei.
    pub async fn estimate_gas(&self, client: &TempoClient) -> Result<U256> {
        let gas_price = client.provider.get_gas_price().await?;
        Ok(U256::from(gas_price))
    }

    /// Increases gas price by a percentage
    ///
    /// Useful for retrying transactions with higher fees for faster confirmation.
    ///
    /// # Arguments
    ///
    /// * `gas_price` - The current gas price
    /// * `percent` - Percentage increase (e.g., 20 for 20%)
    ///
    /// # Returns
    ///
    /// The increased gas price.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tempo_spammer::tasks::GasManager;
    /// use alloy_primitives::U256;
    ///
    /// let gas_manager = GasManager;
    /// let current = U256::from(1000000000u64); // 1 Gwei
    ///
    /// // Bump by 20%
    /// let bumped = gas_manager.bump_fees(current, 20);
    /// assert_eq!(bumped, U256::from(1200000000u64)); // 1.2 Gwei
    /// ```
    pub fn bump_fees(&self, gas_price: U256, percent: u64) -> U256 {
        let multiplier = U256::from(100 + percent);
        let divisor = U256::from(100);
        gas_price * multiplier / divisor
    }
}

fn generate_random_address() -> Address {
    let mut rng = rand::rngs::OsRng;
    let bytes: [u8; 20] = rng.r#gen();
    Address::from_slice(&bytes)
}

pub fn get_random_address() -> Result<Address> {
    // Check root directory first
    let root_path = Path::new("address.txt");
    let config_path = Path::new("config/address.txt");

    let path = if root_path.exists() {
        root_path
    } else if config_path.exists() {
        config_path
    } else {
        return Ok(generate_random_address());
    };

    let content = fs::read_to_string(path).context("Failed to read address.txt")?;
    let addresses: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    if addresses.is_empty() {
        return Ok(generate_random_address());
    }

    let random_address = addresses
        .choose(&mut rand::rngs::OsRng)
        .unwrap_or(&addresses[0]);

    Address::from_str(random_address.trim()).context("Invalid address in address.txt")
}

pub fn get_n_random_addresses(n: usize) -> Result<Vec<Address>> {
    let root_path = Path::new("address.txt");
    let config_path = Path::new("config/address.txt");

    let path = if root_path.exists() {
        root_path
    } else if config_path.exists() {
        config_path
    } else {
        return Ok(Vec::new());
    };

    let content = fs::read_to_string(path).context("Failed to read address.txt")?;

    let addresses: Vec<Address> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| Address::from_str(line.trim()).ok())
        .collect();

    if addresses.is_empty() {
        return Ok(Vec::new());
    }

    let mut unique_vec: Vec<Address> = addresses
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let mut rng = rand::rngs::OsRng;
    unique_vec.shuffle(&mut rng);

    Ok(unique_vec.into_iter().take(n).collect())
}

pub fn generate_random_shares(count: usize, total: u64) -> Vec<u64> {
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![total];
    }
    let mut rng = rand::thread_rng();
    let weights: Vec<f64> = (0..count).map(|_| rng.gen_range(0.0f64..1.0f64)).collect();
    let sum_weights: f64 = weights.iter().sum();
    let mut int_shares: Vec<u64> = weights
        .iter()
        .map(|&w| (w / sum_weights * total as f64) as u64)
        .collect();
    let current_sum: u64 = int_shares.iter().sum();
    let diff = total as i64 - current_sum as i64;
    if diff > 0 {
        for i in 0..diff as usize {
            int_shares[i % count] += 1;
        }
    }
    for s in &mut int_shares {
        if *s == 0 {
            *s = 1;
        }
    }
    let current_sum: u64 = int_shares.iter().sum();
    if current_sum > total {
        let mut diff = current_sum - total;
        for i in 0..count {
            if int_shares[i] > 1 {
                int_shares[i] -= 1;
                diff -= 1;
                if diff == 0 {
                    break;
                }
            }
        }
    }
    int_shares
}

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

// Add at the top of the file: use url::Url;

pub fn load_proxies(path: &str) -> Result<Vec<ProxyConfig>> {
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path).context("Failed to read proxies.txt")?;

    let proxies: Vec<ProxyConfig> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let line = line.trim();
            // Try parsing as URL first if it looks like one
            if line.starts_with("http") && line.contains('@') {
                if let Ok(u) = url::Url::parse(line) {
                    let host = u.host_str().unwrap_or("").to_string();
                    let port = u
                        .port()
                        .unwrap_or(if u.scheme() == "https" { 443 } else { 80 });
                    let username = if !u.username().is_empty() {
                        Some(u.username().to_string())
                    } else {
                        None
                    };
                    let password = if let Some(p) = u.password() {
                        Some(p.to_string())
                    } else {
                        None
                    };

                    let base_url = format!("{}://{}:{}", u.scheme(), host, port);

                    return Some(ProxyConfig {
                        url: base_url,
                        username,
                        password,
                    });
                }
            }

            let parts: Vec<&str> = line.split(':').map(|s| s.trim()).collect();
            match parts.len() {
                1 => Some(ProxyConfig {
                    url: if parts[0].starts_with("http") {
                        parts[0].to_string()
                    } else {
                        format!("http://{}", parts[0])
                    },
                    username: None,
                    password: None,
                }),
                2 => Some(ProxyConfig {
                    url: format!("http://{}:{}", parts[0], parts[1]),
                    username: None,
                    password: None,
                }),
                3 => Some(ProxyConfig {
                    url: format!("http://{}", parts[0]), // host:user:pass ? Unusual.
                    username: Some(parts[1].to_string()),
                    password: Some(parts[2].to_string()),
                }),
                4 => Some(ProxyConfig {
                    url: format!("http://{}:{}", parts[0], parts[1]),
                    username: Some(parts[2].to_string()),
                    password: Some(parts[3].to_string()),
                }),
                _ => None,
            }
        })
        .collect();

    Ok(proxies)
}

pub mod prelude {
    pub use super::{
        GasManager, TaskContext, TaskResult, TempoTask, generate_random_shares,
        get_n_random_addresses, get_random_address, load_proxies as load_proxy_config,
    };
}

pub mod check_native_balance;
pub mod t01_deploy_contract;
pub mod t02_claim_faucet;
pub mod t03_send_token;
pub mod t04_create_stable;
pub mod t05_swap_stable;
pub mod t06_add_liquidity;
pub mod t07_mint_stable;
pub mod t08_burn_stable;
pub mod t09_transfer_token;
pub mod t10_transfer_memo;
pub mod t11_limit_order;
pub mod t12_remove_liquidity;
pub mod t13_grant_role;
pub mod t14_nft_create_mint;
pub mod t15_mint_domain;
pub mod t16_mint_random_nft;
pub mod t17_batch_eip7702;
pub mod t18_tip403_policies;
pub mod t19_wallet_analytics;
pub mod t20_wallet_activity;
pub mod t21_create_meme;
pub mod t22_mint_meme;
pub mod t23_transfer_meme;
pub mod t24_batch_swap;
pub mod t25_batch_system_token;
pub mod t26_batch_stable_token;
pub mod t27_batch_meme_token;
pub mod t28_multi_send_disperse;
pub mod t29_multi_send_disperse_stable;
pub mod t30_multi_send_disperse_meme;
pub mod t31_multi_send_concurrent;
pub mod t32_multi_send_concurrent_stable;
pub mod t33_multi_send_concurrent_meme;
pub mod t34_batch_send_transaction;
pub mod t35_batch_send_transaction_stable;
pub mod t36_batch_send_transaction_meme;
pub mod t37_transfer_later;
pub mod t38_transfer_later_stable;
pub mod t39_transfer_later_meme;
pub mod t40_distribute_shares;
pub mod t41_distribute_shares_stable;
pub mod t42_distribute_shares_meme;
pub mod t43_batch_mint_stable;
pub mod t44_batch_mint_meme;
pub mod t45_deploy_viral_faucet;
pub mod t46_claim_viral_faucet;
pub mod t47_deploy_viral_nft;
pub mod t48_mint_viral_nft;
pub mod t49_time_bomb;
pub mod t50_deploy_storm;
pub mod tempo_tokens;
