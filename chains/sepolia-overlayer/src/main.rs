use sepolia_overlayer::config;
use sepolia_overlayer::spammer;

use anyhow::Result;
use clap::Parser;
use config::SepoliaConfig;
use core_logic::metrics::MetricsCollector;
use core_logic::{exit_with_error, setup_logger, WorkerRunner};
use dialoguer::{theme::ColorfulTheme, Input, Password};
use ethers::prelude::*;
use rand::seq::SliceRandom;
use spammer::EvmSpammer;
use std::env;
use std::path::Path;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "chains/sepolia-overlayer/config.toml")]
    config: String,
    #[arg(long)]
    base_config: Option<String>,
    #[arg(short, long)]
    export_metrics: Option<String>,
    #[arg(long, default_value = "30")]
    metrics_interval: u64,
    #[arg(long)]
    no_proxy: bool,
    #[arg(long, default_value = "10")]
    max_tps: u32,
    #[arg(long, default_value_t = 1.0)]
    min_gwei: f64,
    #[arg(long, default_value_t = 2.0)]
    max_gwei: f64,
    #[arg(long)]
    workers: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _log_guard = setup_logger();
    std::mem::forget(_log_guard);

    let args = Args::parse();
    info!("Loading config from: {}", args.config);

    // Load .env from same directory as config
    if let Some(parent) = Path::new(&args.config).parent() {
        let env_path = parent.join(".env");
        if env_path.exists() {
            let _ = dotenv::from_path(&env_path);
        }
    }

    let config = match SepoliaConfig::load(&args.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load config: {}", e);
            exit_with_error(format!("Failed to load config: {}", e));
        }
    };

    info!(
        "Configuration loaded for chain ID: {}, symbol: {}, explorer: {}",
        config.chain_id, config.symbol, config.explorer
    );

    // Load Wallet Manager with password handling
    let manager = if let Some(ref dir) = config.wallet_dir {
        Arc::new(core_logic::WalletManager::with_wallet_dir(dir)?)
    } else {
        Arc::new(core_logic::WalletManager::new()?)
    };
    let total_wallets = manager.count();

    info!("Found {} wallet files.", total_wallets);

    // Get password (env var first, then interactive fallback)
    let wallet_password = if total_wallets > 0 {
        let mut password = env::var("WALLET_PASSWORD").ok();

        if password.is_none()
            || manager
                .as_ref()
                .get_wallet(0, password.as_deref())
                .await
                .is_err()
        {
            if password.is_none() {
                error!("WALLET_PASSWORD environment variable is not set.");
            } else {
                error!("Wallet decryption failed with provided password.");
            }

            match Password::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter wallet password")
                .interact()
            {
                Ok(input) => {
                    password = Some(input);
                    if let Err(e) = manager.as_ref().get_wallet(0, password.as_deref()).await {
                        error!("Interactive password also failed: {}", e);
                        exit_with_error(format!("Interactive password also failed: {}", e));
                    }
                    info!("Interactive password validated successfully.");
                }
                Err(_) => {
                    error!("Cannot prompt for password (not a terminal).");
                    error!("Please set WALLET_PASSWORD environment variable:");
                    error!("  PowerShell: $env:WALLET_PASSWORD='your_password'");
                    error!("  CMD: set WALLET_PASSWORD=your_password");
                    exit_with_error("Cannot prompt for password. Set WALLET_PASSWORD env var.");
                }
            }
        } else {
            info!("Wallet password validated successfully.");
        }

        password
    } else {
        None
    };

    // Load proxies unless explicitly disabled
    let proxy_pool: Arc<tokio::sync::RwLock<Vec<core_logic::config::ProxyConfig>>> =
        if args.no_proxy {
            info!("Proxy loading disabled by --no-proxy");
            Arc::new(tokio::sync::RwLock::new(Vec::new()))
        } else {
            let proxies = core_logic::ProxyManager::load_proxies()?;
            if !proxies.is_empty() {
                info!("Loaded {} proxies for rotation.", proxies.len());
            }
            Arc::new(tokio::sync::RwLock::new(proxies))
        };

    let proxy_health = Arc::new(core_logic::ProxyHealthManager::new(3, 5));

    // Initialize Database
    let db_manager = core_logic::database::DatabaseManager::new("sepolia-overlayer.db").await?;
    let db_arc = std::sync::Arc::new(db_manager);

    // Get worker count: CLI arg or interactive prompt
    let max_workers: usize = if let Some(w) = args.workers {
        info!("Using {} workers from CLI argument", w);
        w
    } else {
        match Input::with_theme(&ColorfulTheme::default())
            .with_prompt("How many workers?")
            .default(5)
            .interact_text()
        {
            Ok(w) => w.min(total_wallets),
            Err(_) => {
                info!("Non-interactive mode: using default 5 workers");
                5
            }
        }
    };

    let max_tps = args.max_tps;
    info!("Using TPS per proxy: {}", max_tps);

    info!(
        "Starting {} workers, {} TPS per proxy (Available: {})",
        max_workers, max_tps, total_wallets
    );

    let proxy_rate_limiter = Arc::new(core_logic::ProxyRateLimiter::new(max_tps));

    let mut rng = rand::thread_rng();
    let mut wallet_indices: Vec<usize> = (0..total_wallets).collect();
    wallet_indices.shuffle(&mut rng);

    let busy_wallets: Arc<tokio::sync::Mutex<std::collections::HashSet<usize>>> =
        Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new()));

    let mut spammers = Vec::new();
    for &wallet_idx in wallet_indices.iter().take(max_workers) {
        let decrypted = match manager
            .as_ref()
            .get_wallet(wallet_idx, wallet_password.as_deref())
            .await
        {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to decrypt wallet {}: {}", wallet_idx, e);
                continue;
            }
        };

        let key = decrypted.evm_private_key.clone();
        let wallet = key.parse::<ethers::signers::LocalWallet>()?;

        let wallet_id_str = format!("{:03}", wallet_idx + 1);

        // Load base config for dual-chain support
        let (base_rpc_url, base_config) = if let Some(ref base_config_path) = args.base_config {
            // Load .env from base config directory
            if let Some(parent) = Path::new(base_config_path).parent() {
                let env_path = parent.join(".env");
                if env_path.exists() {
                    let _ = dotenv::from_path(&env_path);
                }
            }
            match SepoliaConfig::load(base_config_path) {
                Ok(cfg) => (Some(cfg.rpc_url.clone()), Some(cfg)),
                Err(e) => {
                    error!("Failed to load base config: {}", e);
                    exit_with_error(format!("Failed to load base config: {}", e));
                }
            }
        } else {
            (None, None)
        };

        let spammer = EvmSpammer::new_with_signer(
            config.to_spam_config(),
            config.clone(),
            wallet,
            proxy_pool.clone(),
            proxy_health.clone(),
            proxy_rate_limiter.clone(),
            wallet_id_str,
            Some(db_arc.clone()),
            manager.clone(),
            wallet_password.clone(),
            total_wallets,
            busy_wallets.clone(),
            args.min_gwei,
            args.max_gwei,
            base_rpc_url,
            base_config,
        )?;
        spammers.push(Box::new(spammer) as Box<dyn core_logic::traits::Spammer>);
    }

    // Run
    let metrics_task = if let Some(ref metrics_path) = args.export_metrics {
        let path = metrics_path.clone();
        let interval_secs = args.metrics_interval;
        Some(tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                let metrics = MetricsCollector::global();
                if let Err(e) = metrics.export_to_file(&path).await {
                    tracing::error!("Metrics export failed: {}", e);
                } else {
                    tracing::info!("Metrics exported to {}", path);
                }
            }
        }))
    } else {
        None
    };

    WorkerRunner::run_spammers(spammers).await?;

    if let Some(task) = metrics_task {
        task.abort();
    }

    if let Some(metrics_path) = args.export_metrics {
        let metrics = MetricsCollector::global();
        match metrics.export_to_file(&metrics_path).await {
            Ok(_) => info!("Final metrics exported to {}", metrics_path),
            Err(e) => error!("Failed to export final metrics: {}", e),
        }
    }

    Ok(())
}
