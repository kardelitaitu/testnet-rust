use anyhow::{Context, Result};
use clap::Parser;
use core_logic::database::{AsyncDbConfig, DatabaseManager, FallbackStrategy, QueuedTaskResult};
use core_logic::exit_with_error;
use core_logic::setup_logger;
use core_logic::WalletManager;
use dialoguer::{theme::ColorfulTheme, Password};
use dotenv::dotenv;
use ethers::prelude::*;
use ethers::signers::Signer;
use futures::future::join_all;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use robinhood_spammer::client_pool::ClientPool;
use robinhood_spammer::config::EvmConfig;
use robinhood_spammer::task::{RobinhoodTask, TaskContext};
use robinhood_spammer::utils::gas::GasManager;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};
use zeroize::Zeroizing;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    #[arg(short, long, default_value = "10")]
    workers: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logger();
    dotenv().ok();

    let args = Args::parse();
    let config = EvmConfig::load(&args.config).context("Failed to load config")?;

    info!("🚀 Starting Robinhood Spammer...");
    info!("Target Chain ID: {}", config.chain_id);

    let wallet_manager = Arc::new(WalletManager::new()?);
    let total_wallets = wallet_manager.count();

    if total_wallets == 0 {
        error!("No wallets found");
        exit_with_error("No wallets found. Cannot proceed.");
    }

    let password_input = Password::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter wallet password")
        .interact()?;
    let wallet_password = Zeroizing::new(password_input);

    let async_db_config = AsyncDbConfig {
        channel_capacity: 5000,
        batch_size: 1000,
        flush_interval_ms: 5000,
    };

    let db_manager = Arc::new(
        DatabaseManager::new_with_async("robinhood-spammer.db", async_db_config, FallbackStrategy::Drop).await?,
    );

    let client_pool = Arc::new(
        ClientPool::new(
            config.clone(),
            wallet_manager.clone(),
            Some(wallet_password.to_string()),
        )
        .context("Failed to create client pool")?,
    );

    let tasks: Vec<Box<RobinhoodTask>> = vec![
        Box::new(robinhood_spammer::task::t01_check_balance::CheckBalanceTask),
        Box::new(robinhood_spammer::task::t02_simple_eth_transfer::SimpleEthTransferTask),
    ];
    let tasks = Arc::new(tasks);

    let mut handles = Vec::new();
    for worker_id in 0..args.workers {
        let client_pool = client_pool.clone();
        let tasks = tasks.clone();
        let db = db_manager.clone();
        let config = config.clone();
        let worker_id_str = format!("{:03}", worker_id);

        let handle = tokio::spawn(async move {
            let mut rng = StdRng::from_entropy();
            tokio::time::sleep(Duration::from_millis(rng.gen_range(0..2000))).await;

            loop {
                let lease = match client_pool.try_acquire_client().await {
                    Some(l) => l,
                    None => {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    },
                };

                let task_idx = rng.gen_range(0..tasks.len());
                let task = &tasks[task_idx];
                let wallet_address = lease.client.wallet.address();

                let ctx = TaskContext {
                    provider: lease.client.provider.clone(),
                    wallet: lease.client.wallet.clone(),
                    config: config.clone(),
                    proxy: lease.client.proxy_url.clone(),
                    db: Some(db.clone()),
                    gas_manager: Arc::new(GasManager::new(lease.client.provider.clone())),
                };

                let start = std::time::Instant::now();
                let task_name = task.name().to_string();

                match task.run(ctx).await {
                    Ok(result) => {
                        let duration = start.elapsed();
                        let _ = db.queue_task_result(QueuedTaskResult {
                            worker_id: worker_id_str.clone(),
                            wallet_address: wallet_address.to_string(),
                            task_name: task_name.clone(),
                            success: result.success,
                            message: result.message.clone(),
                            duration_ms: duration.as_millis() as u64,
                            timestamp: chrono::Utc::now().timestamp(),
                        });

                        info!(
                            "[WK:{}][WL:{:03}] {} [{}] {} t:{:.1}s",
                            worker_id_str,
                            lease.index,
                            if result.success { "SUCCESS" } else { "FAILED " },
                            task_name,
                            result.message,
                            duration.as_secs_f32()
                        );
                    },
                    Err(e) => {
                        error!("[WK:{}][WL:{:03}] Error: {:?}", worker_id_str, lease.index, e);
                    },
                }

                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
        });
        handles.push(handle);
    }

    join_all(handles).await;
    Ok(())
}
