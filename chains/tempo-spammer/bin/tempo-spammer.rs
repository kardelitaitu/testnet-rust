use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use core_logic::WalletManager;
use core_logic::database::{AsyncDbConfig, DatabaseManager, FallbackStrategy, QueuedTaskResult};
use core_logic::setup_logger;
use dialoguer::{Input, Password, theme::ColorfulTheme};
use dotenv::dotenv;
use futures::future::join_all;

use rand::distributions::{Distribution, WeightedIndex};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tempo_spammer::ProxyBanlist;
use tempo_spammer::TempoClient;
use tempo_spammer::bot::notification::spawn_notification_service;
use tempo_spammer::config::TempoSpammerConfig as Config;
use tempo_spammer::tasks::{TaskContext, TempoTask, load_proxies};
use tracing::{error, info, warn};
use zeroize::Zeroizing;

// Include compile-time configuration from build.rs
include!(concat!(env!("OUT_DIR"), "/build_config.rs"));

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
    Spammer {
        #[arg(short, long)]
        workers: Option<u64>,
        #[arg(short, long)]
        quiet: bool,
        #[arg(long, default_value = "false")]
        no_proxy: bool,
    },
    Run {
        #[arg(short, long)]
        task: String,
    },
    List,
}
#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let args = Args::parse();

    // Determine quiet mode and no_proxy
    let (is_quiet, no_proxy) = match &args.command {
        Some(Commands::Spammer {
            quiet, no_proxy, ..
        }) => (*quiet, *no_proxy),
        _ => (false, false),
    };

    if !is_quiet {
        let _log_guard = setup_logger();
        // Keep guard alive for file logging - will be dropped at end of main()
        std::mem::forget(_log_guard);
    } else {
        // Minimal logger for quiet mode (Errors only, or muted stdout)
        // For now, we just skip setup_logger which typically enables the flashy output
        // We might want `tracing_subscriber::fmt().with_max_level(Level::ERROR).init();`
        // But the user asked for "quiet", so let's stick to minimal.
        // Assuming core_logic::utils::logger configures global default.
        // We'll initialize a basic one if quiet.
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::ERROR)
            .init();
    }

    // Auto-detect config path if default is not found
    let config_path = if std::path::Path::new(&args.config).exists() {
        args.config.clone()
    } else if args.config == "config/config.toml"
        && std::path::Path::new("chains/tempo-spammer/config/config.toml").exists()
    {
        "chains/tempo-spammer/config/config.toml".to_string()
    } else {
        args.config.clone()
    };

    let config = Config::from_path(&config_path).context("Failed to load config")?;

    if !is_quiet {
        println!(
            r#"
        ╔════════════════════════════════════════════════════════════╗
        ║                 TEMPO SPAMMER - LIVE LOG                   ║
        ╚════════════════════════════════════════════════════════════╝
        "#
        );
    }
    info!(target: "task_result", "Target RPC: {}", config.rpc_url);
    info!(target: "task_result", "Target Chain ID: {}", config.chain_id);
    info!(
        target: "task_result",
        "Workers: {}",
        args.command
            .as_ref()
            .and_then(|c| match c {
                Commands::Spammer { workers, .. } => *workers,
                _ => None,
            })
            .unwrap_or(config.worker_count)
    );
    info!(
        target: "task_result",
        "Interval: {}ms - {}ms",
        config.task_interval_min, config.task_interval_max
    );

    // Prompt for wallet password at runtime (never stored in binary)
    let wallet_manager = WalletManager::new()?;
    let total_wallets = wallet_manager.count();

    if total_wallets == 0 {
        error!("No wallets found");
        return Ok(());
    }

    // Prompt for password immediately at startup
    if !is_quiet {
        println!("\n🔐 Wallet Configuration:");
        println!("   Found {} wallets", total_wallets);
    }

    let password_input = Password::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter wallet password")
        .report(true) // Show asterisks (*****) when typing
        .interact()?;

    // Wrap password in Zeroizing to ensure it's cleared from memory when dropped
    let wallet_password = Zeroizing::new(password_input);

    // Validate password with first wallet
    if let Err(e) = wallet_manager.get_wallet(0, Some(&wallet_password)).await {
        error!("Decryption failed with provided password: {}", e);
        return Ok(());
    }

    if !is_quiet {
        println!("✅ Password accepted.");
    }

    info!("Found {} wallets", total_wallets);

    // Prompt for number of workers BEFORE proxy health check
    let runtime_workers = if !is_quiet {
        println!("\n👷 Worker Configuration:");
        println!("   Available wallets: {}", total_wallets);
        println!("   Config default: {}", config.worker_count);

        let workers_input: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Number of workers [default: {}]",
                config.worker_count
            ))
            .default(config.worker_count.to_string())
            .interact()
            .unwrap_or_else(|_| config.worker_count.to_string());

        let workers: u64 = workers_input.parse().unwrap_or(config.worker_count);
        println!("✅ Using {} workers", workers);
        workers
    } else {
        config.worker_count
    };

    // Get the directory containing the config file to find proxies.txt
    let config_path_obj = std::path::Path::new(&config_path);
    let config_dir = config_path_obj
        .parent()
        .unwrap_or(std::path::Path::new("."));

    // Check config dir first, then root
    let config_proxies = config_dir.join("proxies.txt");
    let root_proxies = std::path::Path::new("proxies.txt");

    let proxy_path_str = if config_proxies.exists() {
        config_proxies
            .to_str()
            .unwrap_or("config/proxies.txt")
            .to_string()
    } else if root_proxies.exists() {
        "proxies.txt".to_string()
    } else {
        "config/proxies.txt".to_string()
    };

    // Convert to slice for load_proxies
    let proxy_path_str = proxy_path_str.as_str();

    // Load proxies
    let proxies = if no_proxy {
        Vec::new()
    } else {
        load_proxies(proxy_path_str).unwrap_or_else(|_| Vec::new())
    };

    // Proxy Health Check (if proxies enabled)
    let proxy_banlist = if !proxies.is_empty() {
        if !is_quiet {
            info!(target: "task_result", "🔍 Starting proxy health check for {} proxies...", proxies.len());
        }

        // Start banner animation concurrently
        let banner_handle = if !is_quiet {
            Some(tokio::spawn(display_animated_banner()))
        } else {
            None
        };

        let banlist = ProxyBanlist::new(10); // 10-minute ban
        let (healthy, banned) = tempo_spammer::proxy_health::scan_proxies(
            &proxies,
            &config.rpc_url,
            &banlist,
            10, // 10 concurrent checks to prevent rate limits
        )
        .await;

        // Ensure banner finishes before summary
        if let Some(handle) = banner_handle {
            let _ = handle.await;
        }

        info!(target: "task_result", "✅ {}/{} proxies healthy, ⏱️ {} temporarily banned (10min)", 
            healthy, proxies.len(), banned);

        // Spawn background re-check task
        let proxies_arc = Arc::new(proxies.clone());
        let banlist_clone = banlist.clone();
        let rpc_url_clone = config.rpc_url.clone();
        tokio::spawn(async move {
            tempo_spammer::proxy_health::start_recheck_task(
                proxies_arc,
                rpc_url_clone,
                banlist_clone,
                10, // Re-check every 10 minutes
            )
            .await;
        });

        Some(banlist)
    } else {
        // No proxies, but still show banner if not quiet
        if !is_quiet {
            display_animated_banner().await;
        }
        None
    };

    // Configure async database logging
    let async_db_config = AsyncDbConfig {
        channel_capacity: 1000,
        batch_size: 200,
        flush_interval_ms: 200,
    };

    // Create shared database manager with async logging
    let db_manager = Arc::new(
        DatabaseManager::new_with_async(
            "tempo-spammer.db",
            async_db_config,
            FallbackStrategy::Hybrid, // Drop + warning when full
        )
        .await?,
    );

    // Create ClientPool with cloned password and configurable connection semaphore
    // The original Zeroizing password will be cleared after this scope
    let client_pool = Arc::new(
        tempo_spammer::ClientPool::new(
            config.clone(),
            db_manager.clone(),                // Use same instance
            Some(wallet_password.to_string()), // Clone for ClientPool
            config.connection_semaphore,       // Use configurable semaphore size from config
        )
        .context("Failed to create client pool")?
        .with_proxies(proxies)
        .with_proxy_banlist(proxy_banlist.unwrap_or_else(|| ProxyBanlist::new(10))),
    );

    // wallet_password (Zeroizing<String>) is dropped here and automatically zeroized from memory

    let total_wallets = client_pool.count();
    info!("Found {} wallets", total_wallets);

    // Initialize Telegram bot notification service (every 3 hours)
    if let Some(bot_handle) = spawn_notification_service().await {
        info!(
            "Telegram bot notification service started (chat_id: 1754837820, notifications every 3 hours)"
        );
        // The bot runs independently in the background
        tokio::spawn(async move {
            if let Err(e) = bot_handle.await {
                error!("Telegram bot task failed: {}", e);
            }
        });
    }

    let tasks: Vec<Box<dyn TempoTask>> = vec![
        Box::new(tempo_spammer::tasks::t01_deploy_contract::DeployContractTask::new()),
        Box::new(tempo_spammer::tasks::t02_claim_faucet::ClaimFaucetTask::new()),
        Box::new(tempo_spammer::tasks::t03_send_token::SendTokenTask::new()),
        Box::new(tempo_spammer::tasks::t04_create_stable::CreateStableTask::new()),
        Box::new(tempo_spammer::tasks::t05_swap_stable::SwapStableTask::new()),
        Box::new(tempo_spammer::tasks::t06_add_liquidity::AddLiquidityTask::new()),
        Box::new(tempo_spammer::tasks::t07_mint_stable::MintStableTask::new()),
        Box::new(tempo_spammer::tasks::t08_burn_stable::BurnStableTask::new()),
        Box::new(tempo_spammer::tasks::t09_transfer_token::TransferTokenTask::new()),
        Box::new(tempo_spammer::tasks::t10_transfer_memo::TransferMemoTask::new()),
        Box::new(tempo_spammer::tasks::t11_limit_order::LimitOrderTask::new()),
        Box::new(tempo_spammer::tasks::t12_remove_liquidity::RemoveLiquidityTask::new()),
        Box::new(tempo_spammer::tasks::t13_grant_role::GrantRoleTask::new()),
        Box::new(tempo_spammer::tasks::t14_nft_create_mint::NftCreateMintTask::new()),
        Box::new(tempo_spammer::tasks::t15_mint_domain::MintDomainTask::new()),
        Box::new(tempo_spammer::tasks::t16_mint_random_nft::MintRandomNftTask::new()),
        Box::new(tempo_spammer::tasks::t17_batch_eip7702::BatchEip7702Task::new()),
        Box::new(tempo_spammer::tasks::t18_tip403_policies::Tip403PoliciesTask::new()),
        Box::new(tempo_spammer::tasks::t19_wallet_analytics::WalletAnalyticsTask::new()),
        Box::new(tempo_spammer::tasks::t20_wallet_activity::WalletActivityTask::new()),
        Box::new(tempo_spammer::tasks::t21_create_meme::CreateMemeTask::new()),
        Box::new(tempo_spammer::tasks::t22_mint_meme::MintMemeTask::new()),
        Box::new(tempo_spammer::tasks::t23_transfer_meme::TransferMemeTask::new()),
        Box::new(tempo_spammer::tasks::t24_batch_swap::BatchSwapTask::new()),
        Box::new(tempo_spammer::tasks::t25_batch_system_token::BatchSystemTokenTask::new()),
        Box::new(tempo_spammer::tasks::t26_batch_stable_token::BatchStableTokenTask::new()),
        Box::new(tempo_spammer::tasks::t27_batch_meme_token::BatchMemeTokenTask::new()),
        Box::new(tempo_spammer::tasks::t28_multi_send_disperse::MultiSendDisperseTask::new()),
        Box::new(tempo_spammer::tasks::t29_multi_send_disperse_stable::MultiSendDisperseStableTask::new()),
        Box::new(tempo_spammer::tasks::t30_multi_send_disperse_meme::MultiSendDisperseMemeTask::new()),
        Box::new(tempo_spammer::tasks::t31_multi_send_concurrent::MultiSendConcurrentTask::new()),
        Box::new(tempo_spammer::tasks::t32_multi_send_concurrent_stable::MultiSendConcurrentStableTask::new()),
        Box::new(tempo_spammer::tasks::t33_multi_send_concurrent_meme::MultiSendConcurrentMemeTask::new()),
        Box::new(tempo_spammer::tasks::t34_batch_send_transaction::BatchSendTransactionTask::new()),
        Box::new(tempo_spammer::tasks::t35_batch_send_transaction_stable::BatchSendTransactionStableTask::new()),
        Box::new(tempo_spammer::tasks::t36_batch_send_transaction_meme::BatchSendTransactionMemeTask::new()),
        Box::new(tempo_spammer::tasks::t37_transfer_later::TransferLaterTask::new()),
        Box::new(tempo_spammer::tasks::t38_transfer_later_stable::TransferLaterStableTask::new()),
        Box::new(tempo_spammer::tasks::t39_transfer_later_meme::TransferLaterMemeTask::new()),
        Box::new(tempo_spammer::tasks::t40_distribute_shares::DistributeSharesTask::new()),
        Box::new(tempo_spammer::tasks::t41_distribute_shares_stable::DistributeSharesStableTask::new()),
        Box::new(tempo_spammer::tasks::t42_distribute_shares_meme::DistributeSharesMemeTask::new()),
        Box::new(tempo_spammer::tasks::t43_batch_mint_stable::BatchMintStableTask::new()),
        Box::new(tempo_spammer::tasks::t44_batch_mint_meme::BatchMintMemeTask::new()),
        Box::new(tempo_spammer::tasks::t45_deploy_viral_faucet::DeployViralFaucetTask::new()),
        Box::new(tempo_spammer::tasks::t46_claim_viral_faucet::ClaimViralFaucetTask::new()),
        Box::new(tempo_spammer::tasks::t47_deploy_viral_nft::DeployViralNftTask::new()),
        Box::new(tempo_spammer::tasks::t48_mint_viral_nft::MintViralNftTask::new()),
        Box::new(tempo_spammer::tasks::t49_time_bomb::TimeBombTask::new()),
        Box::new(tempo_spammer::tasks::t50_deploy_storm::DeployStormTask::new()),
    ];

    match args.command {
        Some(Commands::Spammer { workers, .. }) => {
            // Use CLI workers if provided, otherwise use runtime_workers (already prompted)
            let worker_count = workers.unwrap_or(runtime_workers);
            run_spammer(client_pool, tasks, &config, db_manager, worker_count).await;
        }
        Some(Commands::Run { task }) => {
            // run_single_task logic would need updating too, but skipping for now to focus on spammer
            let client = client_pool
                .get_client(0)
                .await
                .expect("Failed to get client 0");
            run_single_task(&client, &tasks, &task, &config, db_manager.clone()).await;
        }
        Some(Commands::List) => {
            println!("Available tasks:");
            for (i, task) in tasks.iter().enumerate() {
                println!("  {}: {}", i + 1, task.name());
            }
        }
        None => {
            // Use runtime_workers (already prompted before proxy health check)
            run_spammer(client_pool, tasks, &config, db_manager, runtime_workers).await;
        }
    }

    Ok(())
}

async fn display_animated_banner() {
    let lines = [
        "\n",
        "    ╔══════════════════════════════════════════════════════════════════╗",
        "    ║                                                                  ║",
        "    ║  ██████  █████   ███    ██ ████████ ███████ ███    ██  ██████    ║",
        "    ║  ██      ██   ██ ████   ██    ██    ██      ████   ██ ██         ║",
        "    ║  ██   ██ ███████ ██ ██  ██    ██    █████   ██ ██  ██ ██   ███   ║",
        "    ║  ██   ██ ██   ██ ██  ██ ██    ██    ██      ██  ██ ██ ██    ██   ║",
        "    ║   ██████ ██   ██ ██   ████    ██    ███████ ██   ████  ██████    ║",
        "    ║                                                                  ║",
        "    ║  ███    ███  █████  ██   ██ ███████ ██ ███    ███  █████  ██     ║",
        "    ║  ████  ████ ██   ██ ██  ██  ██      ██ ████  ████ ██   ██ ██     ║",
        "    ║  ██ ████ ██ ███████ █████   ███████ ██ ██ ████ ██ ███████ ██     ║",
        "    ║  ██  ██  ██ ██   ██ ██  ██       ██ ██ ██  ██  ██ ██   ██ ██     ║",
        "    ║  ██      ██ ██   ██ ██   ██ ███████ ██ ██      ██ ██   ██ █████  ║",
        "    ║                                                                  ║",
        "    ╚══════════════════════════════════════════════════════════════════╝",
        "\n",
    ];

    for line in lines {
        println!("{}", line);
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

async fn run_spammer(
    client_pool: Arc<tempo_spammer::ClientPool>,
    tasks: Vec<Box<dyn TempoTask>>,
    config: &Config,
    db_manager: Arc<DatabaseManager>,
    worker_count: u64,
) {
    info!(target: "task_result", "Starting spammer with {} workers...", worker_count);

    let task_weights: Vec<u32> = tasks
        .iter()
        .map(|t| match t.name() {
            n if n.contains("SendToken") => 10,
            n if n.contains("Transfer") => 10,
            n if n.contains("Swap") => 5,
            _ => 1,
        })
        .collect();
    let dist = WeightedIndex::new(&task_weights).expect("Failed to create weighted distribution");
    let tasks = Arc::new(tasks);

    let config = config.clone();
    let _client_count = client_pool.count();

    let mut handles = Vec::new();

    for worker_id in 0..worker_count {
        let client_pool = client_pool.clone();
        let tasks = tasks.clone();
        let db = db_manager.clone();
        let config = config.clone();
        let dist = dist.clone();

        let handle = tokio::spawn(async move {
            let mut rng = StdRng::from_entropy();
            let initial_sleep = rng.gen_range(0..2000);
            tokio::time::sleep(Duration::from_millis(initial_sleep)).await;

            let mut backoff_ms = 10u64; // Start with 10ms backoff

            loop {
                // Check for cancellation
                if false {
                    break;
                } // Placeholder

                // let wallet_idx = rng.gen_range(0..client_count); // Handled by pool

                // Acquire lease on a wallet with exponential backoff
                let lease = match client_pool.try_acquire_client().await {
                    Some(l) => {
                        backoff_ms = 10; // Reset backoff on success
                        l
                    }
                    None => {
                        // All wallets busy, use exponential backoff (10ms -> 20ms -> 40ms... max 100ms)
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                        backoff_ms = (backoff_ms * 2).min(100); // Double but cap at 100ms
                        continue;
                    }
                };

                let wallet_idx = lease.index;
                let client = lease.client.clone(); // Clone ARC, lease stays alive until end of scope

                let task_idx = dist.sample(&mut rng);
                let task = &tasks[task_idx];

                let ctx = TaskContext::new(client.clone(), config.clone(), Some(db.clone()));

                let proxy_url_for_span = client
                    .proxy_config
                    .as_ref()
                    .map(|p| p.url.as_str())
                    .unwrap_or("direct");

                let span = tracing::info_span!(
                    "task",
                    worker_id = worker_id,
                    wallet = ?client.address(),
                    task = task.name(),
                    proxy = proxy_url_for_span
                );
                let start = std::time::Instant::now();

                match tokio::time::timeout(Duration::from_secs(config.task_timeout), task.run(&ctx))
                    .await
                {
                    Ok(Ok(result)) => {
                        let _enter = span.enter();
                        let duration = start.elapsed();

                        // Async logging: queue result without blocking
                        if let Some(database) = &ctx.db {
                            let queued_result = QueuedTaskResult {
                                worker_id: format!("{:03}", worker_id),
                                wallet_address: client.address().to_string(),
                                task_name: task.name().to_string(),
                                success: result.success,
                                message: result.message.clone(),
                                duration_ms: duration.as_millis() as u64,
                                timestamp: chrono::Utc::now().timestamp(),
                            };

                            // Non-blocking send (returns immediately)
                            if let Err(e) = database.queue_task_result(queued_result) {
                                // Log at warn level for visibility - this shouldn't happen often
                                warn!("Failed to queue task result for DB logging: {}", e);
                            }
                        }

                        let status_msg = if result.success {
                            if let Some(tx_hash) = &result.tx_hash {
                                format!("TxHash: {}", tx_hash)
                            } else if !result.message.is_empty() {
                                result.message.clone()
                            } else {
                                "Success".to_string()
                            }
                        } else {
                            result.message.clone()
                        };

                        info!(
                            target: "task_result",
                            "[WK:{:03}][WL:{:03}][P:{}] {} [{}] {} t:{:.1}s",
                            worker_id,
                            wallet_idx,
                            client.proxy_index.map(|i| format!("{:03}", i)).unwrap_or_else(|| "DIR".to_string()),
                            if result.success { "SUCCESS" } else { "FAILED " },
                            task.name(),
                            status_msg,
                            duration.as_secs_f32()
                        );
                    }
                    Ok(Err(e)) => {
                        let _enter = span.enter();
                        let duration = start.elapsed();
                        let error_msg = format!("{:#}", e);

                        // === PROXY BANNING LOGIC ===
                        // Detect connection/tunnel errors that indicate a bad proxy
                        if error_msg.contains("tunnel error")
                            || error_msg.contains("Connect")
                            || error_msg.contains("connection closed")
                            || error_msg.contains("error sending request")
                        {
                            if let Some(proxy_idx) = client.proxy_index {
                                if let Some(banlist) = &client_pool.proxy_banlist {
                                    tracing::warn!(
                                        "[WK:{:03}][P:{:03}] 🚫 Banning unhealthy proxy due to error: {:.100}...",
                                        worker_id,
                                        proxy_idx,
                                        error_msg
                                    );
                                    banlist.ban(proxy_idx).await;
                                }
                            }
                        }

                        let mut recovered = false;

                        // Auto-refresh nonce cache on "nonce too low" errors
                        if error_msg.contains("nonce too low") {
                            tracing::debug!(
                                "[WK:{:03}] Detected stale nonce, refreshing from blockchain...",
                                worker_id
                            );

                            // Force refresh nonce from blockchain
                            if let Some(robust_manager) = &ctx.client.robust_nonce_manager {
                                let mut handled = false;
                                // Parse error: "nonce too low: next nonce <next>, tx nonce <tx>"
                                if let (Some(next_pos), Some(tx_pos)) =
                                    (error_msg.find("next nonce "), error_msg.find(", tx nonce "))
                                {
                                    let next_str = &error_msg[next_pos + 11..tx_pos];
                                    let tx_str_check = &error_msg[tx_pos + 11..];
                                    // tx_str might have trailing chars, take until non-digit
                                    let tx_str = tx_str_check
                                        .chars()
                                        .take_while(|c| c.is_ascii_digit())
                                        .collect::<String>();

                                    if let (Ok(next_nonce), Ok(tx_nonce)) = (
                                        next_str.trim().parse::<u64>(),
                                        tx_str.trim().parse::<u64>(),
                                    ) {
                                        robust_manager
                                            .handle_nonce_error(ctx.address(), tx_nonce, next_nonce)
                                            .await;
                                        tracing::info!(
                                            "[WK:{:03}] Robust recovery: failed {} -> actual {}",
                                            worker_id,
                                            tx_nonce,
                                            next_nonce
                                        );
                                        handled = true;
                                        recovered = true;
                                    }
                                }

                                if !handled {
                                    // Fallback: use get_pending_nonce which handles "pending" tag correctly manual
                                    match ctx.client.get_pending_nonce(&ctx.config.rpc_url).await {
                                        Ok(fresh_nonce) => {
                                            robust_manager
                                                .initialize(ctx.address(), fresh_nonce)
                                                .await;
                                            tracing::debug!(
                                                "[WK:{:03}] RobustNonceManager re-initialized to {}",
                                                worker_id,
                                                fresh_nonce
                                            );
                                            recovered = true;
                                        }
                                        Err(e) => tracing::warn!(
                                            "[WK:{:03}] Failed to refresh robust nonce: {:?}",
                                            worker_id,
                                            e
                                        ),
                                    }
                                }
                            }
                            // Legacy Manager
                            else if let Some(manager) = &ctx.client.nonce_manager {
                                match ctx.client.get_pending_nonce(&ctx.config.rpc_url).await {
                                    Ok(fresh_nonce) => {
                                        manager.set(ctx.address(), fresh_nonce).await;
                                        tracing::debug!(
                                            "[WK:{:03}] Nonce cache refreshed to {}",
                                            worker_id,
                                            fresh_nonce
                                        );
                                        recovered = true;
                                    }
                                    Err(refresh_err) => {
                                        tracing::warn!(
                                            "[WK:{:03}] Failed to refresh nonce: {:?}",
                                            worker_id,
                                            refresh_err
                                        );
                                    }
                                }
                            }
                        }

                        // Async logging for error
                        if let Some(database) = &ctx.db {
                            let queued_result = QueuedTaskResult {
                                worker_id: format!("{:03}", worker_id),
                                wallet_address: client.address().to_string(),
                                task_name: task.name().to_string(),
                                success: false,
                                message: error_msg.clone(),
                                duration_ms: duration.as_millis() as u64,
                                timestamp: chrono::Utc::now().timestamp(),
                            };

                            if let Err(e) = database.queue_task_result(queued_result) {
                                warn!("Failed to queue error result for DB logging: {}", e);
                            }
                        }

                        if recovered {
                            // Log as INFO/WARN - it's a recovered error, normal operation
                            info!(target: "task_result", "[WK:{:03}][WL:{:03}][P:{}] \x1b[33mRETRY\x1b[0m [{}] Nonce mismatch (recovered) t:{:.1}s",
                                worker_id,
                                wallet_idx,
                                client.proxy_index.map(|i| format!("{:03}", i)).unwrap_or_else(|| "DIR".to_string()),
                                task.name(),
                                duration.as_secs_f32()
                            );
                        } else {
                            error!(target: "task_result", "[WK:{:03}][WL:{:03}][P:{}] \x1b[31mERROR\x1b[0m [{}] Task error: {} t:{:.1}s",
                                worker_id,
                                wallet_idx,
                                client.proxy_index.map(|i| format!("{:03}", i)).unwrap_or_else(|| "DIR".to_string()),
                                task.name(),
                                error_msg,
                                duration.as_secs_f32()
                            );
                        }
                    }
                    Err(_) => {
                        let _enter = span.enter();
                        let duration = start.elapsed();
                        let error_msg = "Task timed out".to_string();

                        // Async logging for timeout
                        if let Some(database) = &ctx.db {
                            let queued_result = QueuedTaskResult {
                                worker_id: format!("{:03}", worker_id),
                                wallet_address: client.address().to_string(),
                                task_name: task.name().to_string(),
                                success: false,
                                message: error_msg.clone(),
                                duration_ms: duration.as_millis() as u64,
                                timestamp: chrono::Utc::now().timestamp(),
                            };

                            if let Err(e) = database.queue_task_result(queued_result) {
                                warn!("Failed to queue timeout result for DB logging: {}", e);
                            }
                        }
                        error!(target: "task_result", "[WK:{:03}][WL:{:03}][P:{}] \x1b[31mERROR\x1b[0m [{}] {} t:{:.1}s",
                            worker_id,
                            wallet_idx,
                            client.proxy_index.map(|i| format!("{:03}", i)).unwrap_or_else(|| "DIR".to_string()),
                            task.name(),
                            error_msg,
                            duration.as_secs_f32()
                        );
                    }
                }

                // Explicitly release the lease with cooldown
                lease.release().await;

                let sleep_ms = config.random_interval();
                tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
            }
        });

        handles.push(handle);
    }

    // Spawn database monitoring task
    let db_monitor = db_manager.clone();
    let monitor_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let metrics = db_monitor.get_metrics();
            let (queued, dropped) = db_monitor.get_async_metrics();
            info!(
                "DB Metrics: {} queries, {} errors ({:.1}%), {} queued, {} dropped",
                metrics.total_queries,
                metrics.total_errors,
                metrics.error_rate(),
                queued,
                dropped
            );
        }
    });

    join_all(handles).await;

    // Cancel monitor task
    monitor_handle.abort();
}

async fn run_single_task(
    client: &TempoClient,
    tasks: &[Box<dyn TempoTask>],
    task_name: &str,
    config: &Config,
    db_manager: Arc<DatabaseManager>,
) {
    let task = tasks
        .iter()
        .find(|t| t.name() == task_name)
        .or_else(|| {
            tasks
                .iter()
                .find(|t| t.name().to_lowercase().contains(&task_name.to_lowercase()))
        })
        .expect("Task not found");

    let ctx = TaskContext::new(client.clone(), config.clone(), Some(db_manager.clone()));

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

    // Graceful shutdown: flush any pending database writes
    if let Some(db) = Arc::try_unwrap(db_manager).ok() {
        info!("Shutting down database...");
        if let Err(e) = db.shutdown().await {
            error!("Error during database shutdown: {}", e);
        }
    }

    info!("tempo-spammer shutdown complete");
}
