use anyhow::{Context, Result};
use clap::Parser;
use core_logic::WalletManager;
use core_logic::database::DatabaseManager;
use dialoguer::{Input, Password, theme::ColorfulTheme};
use dotenv::dotenv;
use std::env;

use std::sync::Arc;
use std::time::Duration;
use tempo_spammer::config::TempoSpammerConfig;
use tempo_spammer::tasks::{TaskContext, TempoTask, load_proxies};
use tokio::sync::Semaphore;

// Include compile-time configuration from build.rs
include!(concat!(env!("OUT_DIR"), "/build_config.rs"));

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Smart Tempo Sequence Runner - Auto-detects wallets and skips completed ones"
)]
struct Args {
    /// Path to config.toml
    #[arg(short, long, default_value = "config/config.toml")]
    config: String,

    /// Skip database logging
    #[arg(long, default_value = "false")]
    no_db: bool,

    /// Number of concurrent workers
    #[arg(long, default_value = "5")]
    workers: usize,

    /// Start from specific wallet index (for resuming)
    #[arg(long, default_value = "0")]
    start_from: usize,

    /// Skip wallets that already have completed tasks in database
    #[arg(long, default_value = "true")]
    skip_completed: bool,

    /// Show wallet summary and exit (don't run tasks)
    #[arg(long, default_value = "false")]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let args = Args::parse();

    // 1. Load Config
    let config_path = if std::path::Path::new(&args.config).exists() {
        args.config.clone()
    } else if args.config == "config/config.toml"
        && std::path::Path::new("chains/tempo-spammer/config/config.toml").exists()
    {
        "chains/tempo-spammer/config/config.toml".to_string()
    } else {
        args.config.clone()
    };
    let config = TempoSpammerConfig::from_path(&config_path).context("Failed to load config")?;

    // 2. Load Wallets - SMART: Auto-detect all wallets
    // Priority: env var > compile-time > interactive prompt
    let mut wallet_password = env::var("WALLET_PASSWORD").ok().or_else(|| {
        if !COMPILE_TIME_PASSWORD.is_empty() {
            Some(COMPILE_TIME_PASSWORD.to_string())
        } else {
            None
        }
    });

    let wallet_manager = WalletManager::new()?;
    let total_wallets = wallet_manager.count();
    if total_wallets == 0 {
        return Err(anyhow::anyhow!("No wallets found in wallet-json directory"));
    }

    // Interactive password prompt if env var not set or invalid
    if let Err(_) = wallet_manager
        .get_wallet(0, wallet_password.as_deref())
        .await
    {
        println!("\n⚠️  Wallet decryption failed (password not set or invalid).");
        let input = Password::with_theme(&ColorfulTheme::default())
            .with_prompt("Enter wallet password")
            .interact()?;
        wallet_password = Some(input);

        // Validate again
        if let Err(e) = wallet_manager
            .get_wallet(0, wallet_password.as_deref())
            .await
        {
            return Err(anyhow::anyhow!(
                "Decryption failed with provided password: {}",
                e
            ));
        }
        println!("✅ Password accepted.");
    }

    // 3. Initialize DB for smart filtering
    let db_arc = match DatabaseManager::new("tempo-spammer.db").await {
        Ok(db) => std::sync::Arc::new(db),
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to initialize database: {}", e));
        }
    };

    // 4. Define The Sequence
    let sequence_ids = vec![2, 4, 7, 21, 22];
    let task_names = vec![
        "Claim Faucet",
        "Create Stable",
        "Mint Stable",
        "Create Meme",
        "Mint Meme",
    ];

    // 5. SMART: Analyze wallet completion status
    println!("🔍 Analyzing {} wallets...", total_wallets);
    let mut wallets_to_process = Vec::new();
    let mut skipped_count = 0;

    for wallet_idx in args.start_from..total_wallets {
        // Get wallet address
        let wallet = match wallet_manager
            .get_wallet(wallet_idx, wallet_password.as_deref())
            .await
        {
            Ok(w) => w,
            Err(e) => {
                println!("⚠️  Wallet {}: Failed to load - {}", wallet_idx, e);
                continue;
            }
        };
        let wallet_address = wallet.evm_address.clone();

        // Check completion status if skip_completed is enabled
        let should_skip = if args.skip_completed && !args.no_db {
            let completed_tasks =
                check_wallet_completion(&db_arc, &wallet_address, &sequence_ids).await;
            completed_tasks.len() == sequence_ids.len()
        } else {
            false
        };

        if should_skip {
            skipped_count += 1;
            continue;
        }

        wallets_to_process.push(wallet_idx);
    }

    let wallets_to_run = wallets_to_process.len();

    if wallets_to_run == 0 {
        println!("✅ All wallets have completed the sequence!");
        return Ok(());
    }

    println!("🚀 Starting Smart Tempo Sequence Runner");
    println!(
        "📋 Sequence: {:?}",
        sequence_ids
            .iter()
            .zip(task_names.iter())
            .map(|(id, name)| format!("{}:{}", id, name))
            .collect::<Vec<_>>()
    );
    println!(
        "💼 Total Wallets: {} | Processing: {} | Skipped (completed): {}",
        total_wallets, wallets_to_run, skipped_count
    );
    println!(
        "👷 Workers: {} | Starting from wallet: {}",
        args.workers, args.start_from
    );

    if args.dry_run {
        println!("\n📊 Dry Run - Wallets to process:");
        for (_i, &wallet_idx) in wallets_to_process.iter().take(10).enumerate() {
            let wallet = wallet_manager
                .get_wallet(wallet_idx, wallet_password.as_deref())
                .await?;
            let completed =
                check_wallet_completion(&db_arc, &wallet.evm_address, &sequence_ids).await;
            let status = if completed.is_empty() {
                "New"
            } else {
                &format!("Partial ({}/{})", completed.len(), sequence_ids.len())
            };
            println!(
                "  Wallet {}: {}... [{}]",
                wallet_idx,
                &wallet.evm_address[..20],
                status
            );
        }
        if wallets_to_process.len() > 10 {
            println!("  ... and {} more", wallets_to_process.len() - 10);
        }
        println!("\n✅ Dry run complete. Use without --dry-run to execute.");
        return Ok(());
    }

    // Determine effective worker count: compile-time > CLI arg > interactive
    let compile_workers = if COMPILE_TIME_WORKERS > 0 {
        COMPILE_TIME_WORKERS as usize
    } else {
        args.workers
    };

    // Interactive worker count prompt
    let effective_workers = if !args.dry_run {
        println!("\n👷 Worker Configuration:");
        println!("   Build default: {}", compile_workers);
        println!("   Available wallets: {}", wallets_to_run);
        let input: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Number of workers [default: {}]", compile_workers))
            .default(compile_workers.to_string())
            .interact()
            .unwrap_or_else(|_| compile_workers.to_string());
        input.parse().unwrap_or(compile_workers)
    } else {
        compile_workers
    };

    println!("---------------------------------------------------");

    // 6. Initialize ClientPool
    let config_dir = std::path::Path::new(&config_path)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let proxies_path = config_dir.join("proxies.txt");
    let proxies = load_proxies(proxies_path.to_str().unwrap_or("config/proxies.txt"))?;

    let client_pool = Arc::new(
        tempo_spammer::ClientPool::new(config.clone(), db_arc.clone(), wallet_password.clone())
            .context("Failed to create client pool")?
            .with_proxies(proxies.clone()),
    );

    let db = if !args.no_db { Some(db_arc) } else { None };

    let semaphore = Arc::new(Semaphore::new(effective_workers));
    let mut handles = Vec::new();

    // SMART: Process only wallets that need processing
    for &wallet_idx in &wallets_to_process {
        let semaphore = semaphore.clone();
        let client_pool = client_pool.clone();
        let config = config.clone();
        let db = db.clone();
        let sequence_ids = sequence_ids.clone();

        let handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();

            println!("Starting Wallet {:02}", wallet_idx);

            for task_id in &sequence_ids {
                // 1. Get Client with ROTATED proxy
                let client = match client_pool.get_client_with_rotated_proxy(wallet_idx).await {
                    Ok(c) => c,
                    Err(e) => {
                        println!("❌ [Wallet {:02}] Client Error: {}", wallet_idx, e);
                        continue;
                    }
                };

                let proxy_idx_str = client
                    .proxy_index
                    .map(|i| format!("{:03}", i))
                    .unwrap_or_else(|| "DIR".to_string());

                // 2. Instantiate Task
                let task: Box<dyn TempoTask> = match task_id {
                    2 => Box::new(tempo_spammer::tasks::t02_claim_faucet::ClaimFaucetTask::new()),
                    4 => Box::new(tempo_spammer::tasks::t04_create_stable::CreateStableTask::new()),
                    7 => Box::new(tempo_spammer::tasks::t07_mint_stable::MintStableTask::new()),
                    21 => Box::new(tempo_spammer::tasks::t21_create_meme::CreateMemeTask::new()),
                    22 => Box::new(tempo_spammer::tasks::t22_mint_meme::MintMemeTask::new()),
                    _ => {
                        println!(
                            "⚠️ [Wallet {:02}] Task {} not implemented in sequence runner",
                            wallet_idx, task_id
                        );
                        continue;
                    }
                };

                println!(
                    "▶️  [Wallet {:02}] Task {:02} | Proxy {} | Running...",
                    wallet_idx, task_id, proxy_idx_str
                );

                let context = TaskContext::new(client, config.clone(), db.clone());
                let start = std::time::Instant::now();

                // Run Task
                let result = tokio::time::timeout(context.timeout, task.run(&context)).await;
                let duration = start.elapsed();

                match result {
                    Ok(Ok(res)) => {
                        if res.success {
                            println!(
                                "✅ [Wallet {:02}] Task {:02} | {:.2}s | {}",
                                wallet_idx,
                                task_id,
                                duration.as_secs_f64(),
                                res.message
                            );
                        } else {
                            println!(
                                "❌ [Wallet {:02}] Task {:02} | {:.2}s | Failed: {}",
                                wallet_idx,
                                task_id,
                                duration.as_secs_f64(),
                                res.message
                            );
                        }
                    }
                    Ok(Err(e)) => {
                        println!(
                            "❌ [Wallet {:02}] Task {:02} | {:.2}s | Error: {:?}",
                            wallet_idx,
                            task_id,
                            duration.as_secs_f64(),
                            e
                        );
                    }
                    Err(_) => {
                        println!(
                            "❌ [Wallet {:02}] Task {:02} | Timeout",
                            wallet_idx, task_id
                        );
                    }
                }

                // Small delay between tasks for same wallet
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            println!("🏁 Wallet {:02} Set Complete", wallet_idx);
        });

        handles.push(handle);
    }

    // Wait for all
    for handle in handles {
        let _ = handle.await;
    }

    println!("All wallets completed.");
    Ok(())
}

/// Check which tasks from the sequence have been completed by this wallet
async fn check_wallet_completion(
    db: &Arc<DatabaseManager>,
    wallet_address: &str,
    sequence_ids: &[u32],
) -> Vec<u32> {
    let mut completed = Vec::new();

    for &task_id in sequence_ids {
        // Query database for successful task completion
        let task_name = match task_id {
            2 => "ClaimFaucet",
            4 => "CreateStable",
            7 => "MintStable",
            21 => "CreateMeme",
            22 => "MintMeme",
            _ => continue,
        };

        match db.has_task_succeeded(wallet_address, task_name).await {
            Ok(true) => completed.push(task_id),
            Ok(false) => {}
            Err(e) => {
                eprintln!("⚠️  DB query error for {}: {}", task_name, e);
            }
        }
    }

    completed
}
