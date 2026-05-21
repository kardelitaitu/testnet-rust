//! # Sepolia Overlayer — Daily Runner
//!
//! Runs each wallet's 17 tasks exactly once per day in randomized order.
//! Successful tasks are tracked in the daily database and excluded for
//! the rest of the day. Failed tasks stay in the pool for retry.
//!
//! ## Usage
//!
//! ```bash
//! $env:WALLET_PASSWORD="your_password"
//! cargo run -p sepolia-overlayer --bin sepolia-daily -- \
//!     --config chains/sepolia-overlayer/config.toml \
//!     --workers 5
//! ```

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use clap::Parser;
use sepolia_overlayer::config::SepoliaConfig;
use sepolia_overlayer::daily_runner::{all_tasks, DailyRunStats, DailyRunner};
use sepolia_overlayer::utils::gas::GasManager;

use std::env;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::info;

#[derive(Parser, Debug)]
#[command(
    name = "sepolia-daily",
    about = "Daily runner for Sepolia Overlayer — one pass per wallet per day"
)]
struct Args {
    /// Path to the sepolia-overlayer config.toml
    #[arg(short, long, default_value = "chains/sepolia-overlayer/config.toml")]
    config: String,

    /// Path to the base chain config.toml (for bridge-back tasks)
    #[arg(long)]
    base_config: Option<String>,

    /// Number of concurrent workers (default: same as wallets)
    #[arg(short, long)]
    workers: Option<usize>,

    /// Minimum gas fee in gwei
    #[arg(long, default_value_t = 0.01)]
    min_gwei: f64,

    /// Disable proxy loading
    #[arg(long)]
    no_proxy: bool,

    /// Path to the daily database file
    #[arg(long, default_value = "sepolia-overlayer-daily.db")]
    db_path: String,

    /// Timeout for one daily task attempt, in seconds
    #[arg(long)]
    task_timeout_secs: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _log_guard = core_logic::setup_logger();
    std::mem::forget(_log_guard);

    let args = Args::parse();
    println!("=== Sepolia Overlayer — Daily Runner ===");

    // Load .env from config directory
    if let Some(parent) = Path::new(&args.config).parent() {
        let env_path = parent.join(".env");
        if env_path.exists() {
            let _ = dotenv::from_path(&env_path);
        }
    }

    // Load config
    let mut config = SepoliaConfig::load(&args.config).context("Failed to load config")?;
    println!(
        "Config loaded: chain_id={}, symbol={}",
        config.chain_id, config.symbol
    );

    // Load base config if provided
    let (base_rpc_url, base_config) = if let Some(ref base_path) = args.base_config {
        if let Some(parent) = Path::new(base_path).parent() {
            let env_path = parent.join(".env");
            if env_path.exists() {
                let _ = dotenv::from_path(&env_path);
            }
        }
        let cfg = SepoliaConfig::load(base_path).context("Failed to load base config")?;
        (Some(cfg.rpc_url.clone()), Some(cfg))
    } else {
        (None, None)
    };

    let task_timeout_secs = args
        .task_timeout_secs
        .or(config.task_timeout_secs)
        .unwrap_or(120)
        .max(1);
    config.task_timeout_secs = Some(task_timeout_secs);

    // Wallet manager
    let manager = if let Some(ref dir) = config.wallet_dir {
        Arc::new(core_logic::WalletManager::with_wallet_dir(dir)?)
    } else {
        Arc::new(core_logic::WalletManager::new()?)
    };
    let total_wallets = manager.count();
    println!("Found {} wallet files", total_wallets);

    if total_wallets == 0 {
        anyhow::bail!("No wallet files found. Create wallets first.");
    }

    // Password
    let wallet_password = resolve_password().await?;
    println!(
        "Password resolved ({} chars)",
        wallet_password.as_deref().unwrap_or("").len()
    );

    // Proxies
    let proxy_pool: Arc<RwLock<Vec<core_logic::config::ProxyConfig>>> = if args.no_proxy {
        println!("Proxies disabled by --no-proxy");
        Arc::new(RwLock::new(Vec::new()))
    } else {
        let proxies = core_logic::ProxyManager::load_proxies()?;
        if !proxies.is_empty() {
            println!("Loaded {} proxies", proxies.len());
        }
        Arc::new(RwLock::new(proxies))
    };

    let proxy_health = Arc::new(core_logic::ProxyHealthManager::new(3, 5));
    let proxy_rate_limiter = Arc::new(core_logic::ProxyRateLimiter::new(config.tps as u32));

    // Daily database
    let db = sepolia_overlayer::daily_runner::database::DailyDb::new(&args.db_path).await?;
    println!("Daily database ready: {}", args.db_path);

    // Worker count
    let worker_count = args
        .workers
        .unwrap_or(config.worker_amount.unwrap_or(5))
        .min(total_wallets)
        .max(1);
    println!("Workers: {}", worker_count);
    println!("Task timeout: {}s", task_timeout_secs);

    // Resolve wallet addresses by decrypting each wallet once
    eprintln!("[Daily] Decrypting {} wallets for address resolution (this may take a while in debug mode)...", total_wallets);
    let mut wallet_addresses: Vec<String> = Vec::with_capacity(total_wallets);
    for idx in 0..total_wallets {
        if idx % 25 == 0 {
            eprintln!("[Daily] Wallet {}/{}...", idx, total_wallets);
        }
        match manager.get_wallet(idx, wallet_password.as_deref()).await {
            Ok(w) => wallet_addresses.push(w.address().to_string()),
            Err(_) => anyhow::bail!("Failed to decrypt wallet {} to get address", idx),
        }
    }

    // Gas managers
    let client = reqwest::Client::new();
    let provider = ethers::providers::Provider::new(ethers::providers::Http::new_with_client(
        reqwest::Url::parse(&config.rpc_url)?,
        client,
    ));
    let gas_manager = Arc::new(GasManager::new(Arc::new(provider), args.min_gwei));

    let base_gas_manager = base_config.as_ref().map(|_| {
        let base_provider = ethers::providers::Provider::new(ethers::providers::Http::new(
            reqwest::Url::parse(base_rpc_url.as_deref().unwrap()).expect("Invalid base RPC URL"),
        ));
        Arc::new(GasManager::new(Arc::new(base_provider), args.min_gwei))
    });

    // Build per-task limits (default = 1 for tasks not in config)
    let task_limits: HashMap<String, u32> = config.task_limits.clone().unwrap_or_default();
    if !task_limits.is_empty() {
        println!("Task limits configured: {:?}", task_limits);
    }

    // Build runner
    let runner = DailyRunner {
        db,
        config,
        wallet_manager: manager,
        wallet_password,
        total_wallets,
        wallet_addresses,
        worker_count,
        tasks: all_tasks(),
        task_limits,
        proxy_pool,
        proxy_health,
        proxy_rate_limiter,
        gas_manager,
        min_gwei: args.min_gwei,
        busy_wallets: Arc::new(Mutex::new(HashSet::new())),
        base_rpc_url,
        base_gas_manager,
        base_config,
    };

    // Run until Ctrl+C
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancel.clone();

    // Handle Ctrl+C
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Received Ctrl+C. Shutting down daily runner...");
        cancel_clone.cancel();
    });

    eprintln!("[Daily] Calling runner.run()...");
    let stats = runner.run(cancel).await?;

    println!("=== Daily Runner Finished ===");
    println!("{}", format_daily_stats_summary(&stats));

    Ok(())
}

/// Format the error message shown when WALLET_PASSWORD env var is not set.
fn format_password_error_msg() -> String {
    "WALLET_PASSWORD environment variable not set.\n\
     Set it before running:\n  $env:WALLET_PASSWORD=\"your_password\"\n  cargo run ..."
        .to_string()
}

/// Resolve the wallet password from env var.
async fn resolve_password() -> Result<Option<String>> {
    match env::var("WALLET_PASSWORD") {
        Ok(pw) => Ok(Some(pw)),
        Err(_) => anyhow::bail!("{}", format_password_error_msg()),
    }
}

/// Format the daily run summary for display.
fn format_daily_stats_summary(stats: &DailyRunStats) -> String {
    format!(
        "Attempts: {}, Success: {}, Failed: {}",
        stats.total_attempts, stats.successful, stats.failed
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sepolia_overlayer::daily_runner::DailyRunStats;

    #[test]
    fn test_format_password_error_msg_contains_keyword() {
        let msg = format_password_error_msg();
        assert!(
            msg.contains("WALLET_PASSWORD"),
            "Error should mention WALLET_PASSWORD"
        );
    }

    #[test]
    fn test_format_daily_stats_summary_all_zeros() {
        let stats = DailyRunStats::default();
        let result = format_daily_stats_summary(&stats);
        assert_eq!(result, "Attempts: 0, Success: 0, Failed: 0");
    }

    #[test]
    fn test_format_daily_stats_summary_mixed_values() {
        let stats = DailyRunStats {
            total_attempts: 10,
            successful: 7,
            failed: 3,
        };
        let result = format_daily_stats_summary(&stats);
        assert_eq!(result, "Attempts: 10, Success: 7, Failed: 3");
    }

    #[test]
    fn test_format_daily_stats_summary_large_numbers() {
        let stats = DailyRunStats {
            total_attempts: 999999,
            successful: 888888,
            failed: 111111,
        };
        let result = format_daily_stats_summary(&stats);
        assert_eq!(result, "Attempts: 999999, Success: 888888, Failed: 111111");
    }

    #[tokio::test]
    async fn test_resolve_password_without_manager_arg() {
        let _ = unsafe { std::env::remove_var("WALLET_PASSWORD") };
        let result = resolve_password().await;
        assert!(
            result.is_err(),
            "Should error when WALLET_PASSWORD is not set"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("WALLET_PASSWORD"),
            "Error should mention WALLET_PASSWORD: {}",
            err
        );
    }
}
