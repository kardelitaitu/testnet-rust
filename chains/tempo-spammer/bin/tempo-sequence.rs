use anyhow::{Context, Result};
use clap::Parser;
use core_logic::WalletManager;
use core_logic::database::DatabaseManager;
use dialoguer::{Input, Password, theme::ColorfulTheme};
use dotenv::dotenv;
use std::env;

use rand::Rng;
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
            .report(true) // Show asterisks (*****) when typing
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

    // 5. Build wallet list (simple - no skipping)
    let wallets_to_process: Vec<usize> = (args.start_from..total_wallets).collect();
    let wallets_to_run = wallets_to_process.len();

    if wallets_to_run == 0 {
        println!("❌ No wallets to process!");
        return Ok(());
    }

    println!("🚀 Starting Tempo Sequence Runner");
    println!(
        "📋 Sequence: {:?}",
        sequence_ids
            .iter()
            .zip(task_names.iter())
            .map(|(id, name)| format!("{}:{}", id, name))
            .collect::<Vec<_>>()
    );
    println!(
        "💼 Total Wallets: {} | Processing: {} wallets",
        total_wallets, wallets_to_run
    );
    println!(
        "👷 Workers: {} | Starting from wallet: {}",
        args.workers, args.start_from
    );

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
        tempo_spammer::ClientPool::new(
            config.clone(),
            db_arc.clone(),
            wallet_password.clone(),
            config.connection_semaphore,
        )
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
                let mut attempt = 0;

                // Infinite retry loop until task succeeds
                loop {
                    attempt += 1;

                    // 1. Get Client with Random Proxy (new proxy every attempt)
                    let rotate_offset = rand::thread_rng().gen_range(0..100);
                    let client = match client_pool
                        .get_client_with_rotated_proxy(wallet_idx, rotate_offset)
                        .await
                    {
                        Ok(c) => c,
                        Err(e) => {
                            if attempt % 5 == 0 || attempt == 1 {
                                println!(
                                    "⚠️  [Wallet {:02}] Client/Proxy Error (Attempt {}): {}. Rotating proxy...",
                                    wallet_idx, attempt, e
                                );
                            }
                            tokio::time::sleep(Duration::from_millis(1000)).await;
                            continue;
                        }
                    };

                    let proxy_idx_str = client
                        .proxy_index
                        .map(|i| format!("{:03}", i))
                        .unwrap_or_else(|| "DIR".to_string());

                    // 2. Instantiate Task
                    let task: Box<dyn TempoTask> = match task_id {
                        2 => {
                            Box::new(tempo_spammer::tasks::t02_claim_faucet::ClaimFaucetTask::new())
                        }
                        4 => Box::new(
                            tempo_spammer::tasks::t04_create_stable::CreateStableTask::new(),
                        ),
                        7 => Box::new(tempo_spammer::tasks::t07_mint_stable::MintStableTask::new()),
                        21 => {
                            Box::new(tempo_spammer::tasks::t21_create_meme::CreateMemeTask::new())
                        }
                        22 => Box::new(tempo_spammer::tasks::t22_mint_meme::MintMemeTask::new()),
                        _ => {
                            println!(
                                "⚠️ [Wallet {:02}] Task {} not implemented in sequence runner",
                                wallet_idx, task_id
                            );
                            break; // Skip non-implemented tasks
                        }
                    };

                    println!(
                        "▶️  [Wallet {:02}] Task {:02} | Proxy {} | Attempt {} | Running...",
                        wallet_idx, task_id, proxy_idx_str, attempt
                    );

                    let context = TaskContext::new(client, config.clone(), db.clone());
                    let start = std::time::Instant::now();

                    // 3. Run Task
                    let result = tokio::time::timeout(context.timeout, task.run(&context)).await;
                    let duration = start.elapsed();

                    // 4. Handle Result
                    let success = match result {
                        Ok(Ok(res)) => {
                            if res.success {
                                println!(
                                    "✅ [Wallet {:02}] Task {:02} | {:.2}s | {}",
                                    wallet_idx,
                                    task_id,
                                    duration.as_secs_f64(),
                                    res.message
                                );
                                true
                            } else {
                                println!(
                                    "❌ [Wallet {:02}] Task {:02} | {:.2}s | Failed: {}",
                                    wallet_idx,
                                    task_id,
                                    duration.as_secs_f64(),
                                    res.message
                                );
                                false
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
                            false
                        }
                        Err(_) => {
                            println!(
                                "❌ [Wallet {:02}] Task {:02} | {:.2}s | Timeout",
                                wallet_idx,
                                task_id,
                                duration.as_secs_f64()
                            );
                            false
                        }
                    };

                    if success {
                        break; // Move to next task
                    }

                    // Wait before retry
                    let sleep_time = if attempt < 5 { 2000 } else { 5000 };
                    tokio::time::sleep(Duration::from_millis(sleep_time)).await;
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
