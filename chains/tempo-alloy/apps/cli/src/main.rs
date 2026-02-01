use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use core_logic::database::DatabaseManager;
use dotenv::dotenv;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tempo_client::prelude::*;
use tempo_client::{TaskContext, TempoClient, TempoTask};
use tracing::{Instrument, error, info};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "config/config.toml")]
    config: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the spammer with all tasks
    Spammer {
        #[arg(short, long)]
        workers: Option<u64>,
    },
    /// Run a specific task once
    Run {
        #[arg(short, long)]
        task: String,
    },
    /// List available tasks
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    // Load config
    let config = load_config(&args.config).context("Failed to load config")?;
    info!(
        "Loaded config: {} (chain {})",
        config.rpc_url, config.chain_id
    );

    // Load wallet
    let wallet_password = env::var("WALLET_PASSWORD").ok();
    let wallet_manager = core_logic::utils::WalletManager::new()?;
    let total_wallets = wallet_manager.count();

    if total_wallets == 0 {
        error!("No wallets found");
        return Ok(());
    }

    info!("Found {} wallets", total_wallets);

    // Load proxies
    let proxies = load_proxies("config/proxies.txt")?;
    info!("Loaded {} proxies", proxies.len());

    // Initialize database
    let db_manager = Arc::new(DatabaseManager::new("tempo.db").await?);

    // Get private key from first wallet
    let decrypted = wallet_manager
        .get_wallet(0, wallet_password.as_deref())
        .await?;
    let private_key = decrypted.evm_private_key;

    // Create Tempo client
    let client = TempoClient::new(&config.rpc_url, &private_key).await?;
    let client_address = client.address();
    info!("Using wallet: {:?}", client_address);

    // Define available tasks
    let tasks: Vec<Box<dyn TempoTask>> = vec![
        Box::new(tempo_client::tasks::t01_deploy_contract::DeployContractTask::new()),
        Box::new(tempo_client::tasks::t02_claim_faucet::ClaimFaucetTask::new()),
        Box::new(tempo_client::tasks::t03_send_token::SendTokenTask::new()),
        Box::new(tempo_client::tasks::t04_create_stable::CreateStableTask::new()),
        Box::new(tempo_client::tasks::t05_swap_stable::SwapStableTask::new()),
    ];

    // Execute command
    match args.command {
        Some(Commands::Spammer { workers }) => {
            let worker_count = workers.unwrap_or(config.worker_count.unwrap_or(1));
            run_spammer(&client, &tasks, worker_count, db_manager).await;
        }
        Some(Commands::Run { task }) => {
            run_single_task(&client, &tasks, &task, db_manager.clone()).await;
        }
        Some(Commands::List) => {
            println!("Available tasks:");
            for (i, task) in tasks.iter().enumerate() {
                println!("  {}: {}", i, task.name());
            }
        }
        None => {
            // Default: run spammer
            run_spammer(
                &client,
                &tasks,
                config.worker_count.unwrap_or(1),
                db_manager,
            )
            .await;
        }
    }

    Ok(())
}

async fn run_spammer(
    client: &TempoClient,
    tasks: &[Box<dyn TempoTask>],
    worker_count: u64,
    db_manager: Arc<DatabaseManager>,
) {
    info!("Starting spammer with {} workers...", worker_count);

    let tasks = Arc::new(tasks.to_vec());

    let mut handles = Vec::new();

    for worker_id in 0..worker_count {
        let client = client.clone();
        let tasks = tasks.clone();
        let db = db_manager.clone();

        let handle = tokio::spawn(async move {
            let initial_sleep = rand::thread_rng().gen_range(0..2000);
            tokio::time::sleep(Duration::from_millis(initial_sleep)).await;

            loop {
                let task_idx = rand::thread_rng().gen_range(0..tasks.len());
                let task = &tasks[task_idx];

                let ctx = TaskContext::new(client.clone(), Some(db.clone()));

                let span = tracing::info_span!("task", worker_id = worker_id, task = task.name());
                let start = std::time::Instant::now();

                match tokio::time::timeout(Duration::from_secs(60), task.run(&ctx)).await {
                    Ok(Ok(result)) => {
                        let _enter = span.enter();
                        let duration = start.elapsed();
                        if result.success {
                            info!(target: "task_result", "Success [{}] {} in {:.1?}", task.name(), result.message, duration);
                        } else {
                            info!(target: "task_result", "Failed  [{}] {} in {:.1?}", task.name(), result.message, duration);
                        }
                    }
                    Ok(Err(e)) => {
                        error!("Task error: {:?}", e);
                    }
                    Err(_) => {
                        error!("Task timed out");
                    }
                }

                // Wait between tasks
                let sleep_sec = rand::thread_rng().gen_range(5..15);
                tokio::time::sleep(Duration::from_secs(sleep_sec)).await;
            }
        });

        handles.push(handle);
    }

    futures::future::join_all(handles).await;
}

async fn run_single_task(
    client: &TempoClient,
    tasks: &[Box<dyn TempoTask>],
    task_name: &str,
    db_manager: Arc<DatabaseManager>,
) {
    let task = tasks
        .iter()
        .find(|t| t.name() == task_name)
        .or_else(|| tasks.iter().find(|t| t.name().contains(task_name)))
        .expect("Task not found");

    let ctx = TaskContext::new(client.clone(), Some(db_manager));

    match task.run(&ctx).await {
        Ok(result) => {
            if result.success {
                println!("✅ Success: {}", result.message);
            } else {
                println!("❌ Failed: {}", result.message);
            }
            if let Some(hash) = result.tx_hash {
                println!("📎 Transaction: {}", hash);
            }
        }
        Err(e) => {
            println!("❌ Error: {:?}", e);
        }
    }
}
