use rise_project::config;
use rise_project::spammer;

use anyhow::Result;
use clap::Parser;
use config::RiseConfig;
use core_logic::metrics::MetricsCollector;
use core_logic::{setup_logger, WorkerRunner};
use dialoguer::{theme::ColorfulTheme, Password};
use dotenv::dotenv;
use ethers::prelude::*;
use rand::seq::SliceRandom;
use spammer::EvmSpammer;
use std::env;
use tokio::time::{interval, Duration};
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "chains/risechain/config.toml")]
    config: String,
    #[arg(short, long)]
    export_metrics: Option<String>,
    #[arg(long, default_value = "30")]
    metrics_interval: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _log_guard = setup_logger();
    // Keep guard alive for file logging
    std::mem::forget(_log_guard);
    dotenv().ok();

    let args = Args::parse();
    info!("Loading config from: {}", args.config);

    let config = match RiseConfig::load(&args.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load config: {}", e);
            return Ok(());
        }
    };

    info!("Configuration loaded for chain ID: {}", config.chain_id);

    // Load Wallet Manager with password handling
    let manager = core_logic::WalletManager::new()?;
    let total_wallets = manager.count();

    info!("Found {} wallet files.", total_wallets);

    // Get password (env var first, then interactive fallback)
    let wallet_password = if total_wallets > 0 {
        let mut password = env::var("WALLET_PASSWORD").ok();

        // Validate password or prompt
        if password.is_none() || manager.get_wallet(0, password.as_deref()).await.is_err() {
            if password.is_none() {
                error!("WALLET_PASSWORD environment variable is not set.");
            } else {
                error!("Wallet decryption failed with provided password.");
            }

            // Try interactive prompt
            match Password::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter wallet password")
                .interact()
            {
                Ok(input) => {
                    password = Some(input);
                    // Validate interactive password
                    if let Err(e) = manager.get_wallet(0, password.as_deref()).await {
                        error!("Interactive password also failed: {}", e);
                        return Ok(());
                    }
                    info!("Interactive password validated successfully.");
                }
                Err(_) => {
                    // Non-interactive mode - show helpful error
                    error!("Cannot prompt for password (not a terminal).");
                    error!("Please set WALLET_PASSWORD environment variable:");
                    error!("  PowerShell: $env:WALLET_PASSWORD='your_password'");
                    error!("  CMD: set WALLET_PASSWORD=your_password");
                    return Ok(());
                }
            }
        } else {
            info!("Wallet password validated successfully.");
        }

        password
    } else {
        None
    };

    // Load proxies
    let proxies = core_logic::ProxyManager::load_proxies()?;
    if !proxies.is_empty() {
        info!("Loaded {} proxies for rotation.", proxies.len());
    }

    // Initialize Database
    let db_manager = core_logic::database::DatabaseManager::new("rise.db").await?;
    let db_arc = std::sync::Arc::new(db_manager);

    // Create spammers
    let mut spammers = Vec::new();

    // Limit workers if configured
    let max_workers = if total_wallets == 0 {
        0
    } else {
        config
            .worker_amount
            .unwrap_or(total_wallets)
            .min(total_wallets)
    };

    info!(
        "Starting {} workers (Available: {}, Configured limit: {:?})",
        max_workers, total_wallets, config.worker_amount
    );

    let mut rng = rand::thread_rng();
    let mut wallet_indices: Vec<usize> = (0..total_wallets).collect();
    wallet_indices.shuffle(&mut rng);

    for i in 0..max_workers {
        let wallet_idx = wallet_indices[i];
        // Lazy decrypt
        let decrypted = match manager
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

        // Assign proxy randomly if available
        let (proxy_config, proxy_id_str) = if !proxies.is_empty() {
            use rand::Rng;
            let idx = rng.gen_range(0..proxies.len());
            (Some(proxies[idx].clone()), format!("{:03}", idx + 1))
        } else {
            (None, "000".to_string())
        };

        if let Some(ref p) = proxy_config {
            info!("Assigned proxy {} to wallet {:?}", p.url, wallet.address());
        }

        // Use wallet_idx for the ID string to track which actual wallet is being used
        let wallet_id_str = format!("{:03}", wallet_idx + 1);

        let spammer = EvmSpammer::new_with_signer(
            config.to_spam_config(),
            config.clone(),
            wallet,
            proxy_config,
            wallet_id_str,
            proxy_id_str,
            Some(db_arc.clone()),
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

    // Cancel metrics task
    if let Some(task) = metrics_task {
        task.abort();
    }

    // Export final metrics if requested
    if let Some(metrics_path) = args.export_metrics {
        let metrics = MetricsCollector::global();
        match metrics.export_to_file(&metrics_path).await {
            Ok(_) => info!("Final metrics exported to {}", metrics_path),
            Err(e) => error!("Failed to export final metrics: {}", e),
        }
    }

    Ok(())
}
