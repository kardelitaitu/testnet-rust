use anyhow::{Context, Result};
use clap::Parser;
use core_logic::WalletManager;
use core_logic::database::DatabaseManager;
use dotenv::dotenv;
use std::env;
use std::io::Write;

use std::time::Duration;
use tempo_spammer::config::TempoSpammerConfig;
use tempo_spammer::tasks::{TaskContext, TempoTask, load_proxies};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to config.toml
    #[arg(short, long, default_value = "config/config.toml")]
    config: String,

    /// Skip database logging
    #[arg(long, default_value = "false")]
    no_db: bool,

    /// Disable proxies (force direct connection)
    #[arg(long, default_value = "false")]
    no_proxy: bool,
}

struct TaskRunResult {
    id: usize,
    name: &'static str,
    success: bool,
    duration: Duration,
    wallet_idx: usize,
    proxy_url: String,
    tx_hash: Option<String>,
    error: Option<String>,
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

    // 2. Load Wallets
    let wallet_password = env::var("WALLET_PASSWORD").ok();
    let wallet_manager = WalletManager::new()?;
    let total_wallets = wallet_manager.count();
    if total_wallets == 0 {
        return Err(anyhow::anyhow!("No wallets found"));
    }

    // 3. Initialize ClientPool
    let config_dir = std::path::Path::new(&config_path)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let proxies_path = config_dir.join("proxies.txt");
    let proxies = load_proxies(proxies_path.to_str().unwrap_or("config/proxies.txt"))?;
    let total_proxies = proxies.len();

    // 4. Initialize DB and ClientPool
    let db_arc = match DatabaseManager::new("tempo-spammer.db").await {
        Ok(db) => std::sync::Arc::new(db),
        Err(e) => {
            if args.no_db {
                // If no_db is requested, we still need a DB instance for ClientPool currently?
                // Let's assume we can just create one that points to a temp file or similar if strictly needed.
                // Or better, let's just propagate the error if we can't create it.
                // But wait, if args.no_db is true, we shouldn't fail?
                // Let's just create it anyway for ClientPool but not use it for logging?
                // Actually, let's just return error if we can't create it for now to be safe.
                return Err(anyhow::anyhow!("Failed to initialize database: {}", e));
            } else {
                return Err(anyhow::anyhow!("Failed to initialize database: {}", e));
            }
        }
    };

    let client_pool = tempo_spammer::ClientPool::new(
        config.clone(),
        db_arc.clone(),
        wallet_password.clone(),
        config.connection_semaphore,
    )
    .context("Failed to create client pool")?
    .with_proxies(proxies.clone());

    let db = if !args.no_db { Some(db_arc) } else { None };

    // 5. Define Tasks (Must match tempo-debug.rs list)
    let tasks: Vec<(usize, &str, &str, Box<dyn TempoTask>)> = vec![
        (1, "01_deploy_contract", "Deploy Counter Contract", Box::new(tempo_spammer::tasks::t01_deploy_contract::DeployContractTask::new())),
        (2, "02_claim_faucet", "Claim Faucet", Box::new(tempo_spammer::tasks::t02_claim_faucet::ClaimFaucetTask::new())),
        (3, "03_send_token", "Send Token", Box::new(tempo_spammer::tasks::t03_send_token::SendTokenTask::new())),
        (4, "04_create_stable", "Create Stablecoin", Box::new(tempo_spammer::tasks::t04_create_stable::CreateStableTask::new())),
        (5, "05_swap_stable", "Swap Stablecoin", Box::new(tempo_spammer::tasks::t05_swap_stable::SwapStableTask::new())),
        (6, "06_add_liquidity", "Add Liquidity", Box::new(tempo_spammer::tasks::t06_add_liquidity::AddLiquidityTask::new())),
        (7, "07_mint_stable", "Mint Stablecoin", Box::new(tempo_spammer::tasks::t07_mint_stable::MintStableTask::new())),
        (8, "08_burn_stable", "Burn Stablecoin", Box::new(tempo_spammer::tasks::t08_burn_stable::BurnStableTask::new())),
        (9, "09_transfer_token", "Transfer Token", Box::new(tempo_spammer::tasks::t09_transfer_token::TransferTokenTask::new())),
        (10, "10_transfer_memo", "Transfer with Memo", Box::new(tempo_spammer::tasks::t10_transfer_memo::TransferMemoTask::new())),
        (11, "11_limit_order", "Limit Order", Box::new(tempo_spammer::tasks::t11_limit_order::LimitOrderTask::new())),
        (12, "12_remove_liquidity", "Remove Liquidity", Box::new(tempo_spammer::tasks::t12_remove_liquidity::RemoveLiquidityTask::new())),
        (13, "13_grant_role", "Grant Role", Box::new(tempo_spammer::tasks::t13_grant_role::GrantRoleTask::new())),
        (14, "14_nft_create_mint", "NFT Create & Mint", Box::new(tempo_spammer::tasks::t14_nft_create_mint::NftCreateMintTask::new())),
        (15, "15_mint_domain", "Mint Domain", Box::new(tempo_spammer::tasks::t15_mint_domain::MintDomainTask::new())),
        (16, "16_mint_random_nft", "Mint Random NFT", Box::new(tempo_spammer::tasks::t16_mint_random_nft::MintRandomNftTask::new())),
        (17, "17_batch_eip7702", "Batch EIP-7702", Box::new(tempo_spammer::tasks::t17_batch_eip7702::BatchEip7702Task::new())),
        (18, "18_tip403_policies", "TIP-403 Policies", Box::new(tempo_spammer::tasks::t18_tip403_policies::Tip403PoliciesTask::new())),
        (19, "19_wallet_analytics", "Wallet Analytics", Box::new(tempo_spammer::tasks::t19_wallet_analytics::WalletAnalyticsTask::new())),
        (20, "20_wallet_activity", "Wallet Activity", Box::new(tempo_spammer::tasks::t20_wallet_activity::WalletActivityTask::new())),
        (21, "21_create_meme", "Create Meme", Box::new(tempo_spammer::tasks::t21_create_meme::CreateMemeTask::new())),
        (22, "22_mint_meme", "Mint Meme", Box::new(tempo_spammer::tasks::t22_mint_meme::MintMemeTask::new())),
        (23, "23_transfer_meme", "Transfer Meme", Box::new(tempo_spammer::tasks::t23_transfer_meme::TransferMemeTask::new())),
        (24, "24_batch_swap", "Batch Swap", Box::new(tempo_spammer::tasks::t24_batch_swap::BatchSwapTask::new())),
        (25, "25_batch_system_token", "Batch System Token", Box::new(tempo_spammer::tasks::t25_batch_system_token::BatchSystemTokenTask::new())),
        (26, "26_batch_stable_token", "Batch Stable Token", Box::new(tempo_spammer::tasks::t26_batch_stable_token::BatchStableTokenTask::new())),
        (27, "27_batch_meme_token", "Batch Meme Token", Box::new(tempo_spammer::tasks::t27_batch_meme_token::BatchMemeTokenTask::new())),
        (28, "28_multi_send_disperse", "Multi-Send Disperse", Box::new(tempo_spammer::tasks::t28_multi_send_disperse::MultiSendDisperseTask::new())),
        (29, "29_multi_send_disperse_stable", "Multi-Send Stable", Box::new(tempo_spammer::tasks::t29_multi_send_disperse_stable::MultiSendDisperseStableTask::new())),
        (30, "30_multi_send_disperse_meme", "Multi-Send Meme", Box::new(tempo_spammer::tasks::t30_multi_send_disperse_meme::MultiSendDisperseMemeTask::new())),
        (31, "31_multi_send_concurrent", "Multi-Send Concurrent", Box::new(tempo_spammer::tasks::t31_multi_send_concurrent::MultiSendConcurrentTask::new())),
        (32, "32_multi_send_concurrent_stable", "Concurrent Stable", Box::new(tempo_spammer::tasks::t32_multi_send_concurrent_stable::MultiSendConcurrentStableTask::new())),
        (33, "33_multi_send_concurrent_meme", "Concurrent Meme", Box::new(tempo_spammer::tasks::t33_multi_send_concurrent_meme::MultiSendConcurrentMemeTask::new())),
        (34, "34_batch_send_transaction", "Batch Send Tx", Box::new(tempo_spammer::tasks::t34_batch_send_transaction::BatchSendTransactionTask::new())),
        (35, "35_batch_send_transaction_stable", "Batch Send Stable", Box::new(tempo_spammer::tasks::t35_batch_send_transaction_stable::BatchSendTransactionStableTask::new())),
        (36, "36_batch_send_transaction_meme", "Batch Send Meme", Box::new(tempo_spammer::tasks::t36_batch_send_transaction_meme::BatchSendTransactionMemeTask::new())),
        (37, "37_transfer_later", "Transfer Later", Box::new(tempo_spammer::tasks::t37_transfer_later::TransferLaterTask::new())),
        (38, "38_transfer_later_stable", "Transfer Later Stable", Box::new(tempo_spammer::tasks::t38_transfer_later_stable::TransferLaterStableTask::new())),
        (39, "39_transfer_later_meme", "Transfer Later Meme", Box::new(tempo_spammer::tasks::t39_transfer_later_meme::TransferLaterMemeTask::new())),
        (40, "40_distribute_shares", "Distribute Shares", Box::new(tempo_spammer::tasks::t40_distribute_shares::DistributeSharesTask::new())),
        (41, "41_distribute_shares_stable", "Distribute Shares Stable", Box::new(tempo_spammer::tasks::t41_distribute_shares_stable::DistributeSharesStableTask::new())),
        (42, "42_distribute_shares_meme", "Distribute Shares Meme", Box::new(tempo_spammer::tasks::t42_distribute_shares_meme::DistributeSharesMemeTask::new())),
        (43, "43_batch_mint_stable", "Batch Mint Stable", Box::new(tempo_spammer::tasks::t43_batch_mint_stable::BatchMintStableTask::new())),
        (44, "44_batch_mint_meme", "Batch Mint Meme", Box::new(tempo_spammer::tasks::t44_batch_mint_meme::BatchMintMemeTask::new())),
        (45, "45_deploy_viral_faucet", "Deploy Viral Faucet", Box::new(tempo_spammer::tasks::t45_deploy_viral_faucet::DeployViralFaucetTask::new())),
        (46, "46_claim_viral_faucet", "Claim Viral Faucet", Box::new(tempo_spammer::tasks::t46_claim_viral_faucet::ClaimViralFaucetTask::new())),
        (47, "47_deploy_viral_nft", "Deploy Viral NFT", Box::new(tempo_spammer::tasks::t47_deploy_viral_nft::DeployViralNftTask::new())),
        (48, "48_mint_viral_nft", "Mint Viral NFT", Box::new(tempo_spammer::tasks::t48_mint_viral_nft::MintViralNftTask::new())),
        (49, "49_time_bomb", "Time Bomb", Box::new(tempo_spammer::tasks::t49_time_bomb::TimeBombTask::new())),
        (50, "50_deploy_storm", "Deploy Storm", Box::new(tempo_spammer::tasks::t50_deploy_storm::DeployStormTask::new())),
    ];

    let mut results = Vec::new();

    println!("🚀 Starting Tempo Runner (10 Concurrent Workers)...");
    println!("Total Tasks: {}", tasks.len());
    println!("Wallets: {}", total_wallets);
    println!("Proxies: {}", total_proxies);
    println!("Workers: 10");
    println!("---------------------------------------------------");

    use futures::StreamExt;
    use rand::Rng;

    let task_stream = futures::stream::iter(tasks).map(|(id, _task_key, task_name, task)| {
        let client_pool = &client_pool;
        let config = config.clone();
        let db = db.clone();
        let wallet_idx = rand::thread_rng().gen_range(0..total_wallets);

        async move {
            let mut last_run = None;
            let mut current_wallet_idx = wallet_idx;

            for attempt in 1..=3 {
                let start = std::time::Instant::now();

                // On retry, force proxy rotation
                let client = match if attempt == 1 {
                    client_pool.get_client(current_wallet_idx).await
                } else {
                    client_pool
                        .get_client_with_rotated_proxy(current_wallet_idx)
                        .await
                } {
                    Ok(c) => c,
                    Err(e) => {
                        let res = TaskRunResult {
                            id,
                            name: task_name,
                            success: false,
                            duration: start.elapsed(),
                            wallet_idx: current_wallet_idx,
                            proxy_url: "Error".to_string(),
                            tx_hash: None,
                            error: Some(format!("Client creation failed: {}", e)),
                        };
                        last_run = Some(res);
                        // Rotate wallet index for the next attempt
                        current_wallet_idx = rand::thread_rng().gen_range(0..total_wallets);
                        continue;
                    }
                };

                let proxy_idx_str = client
                    .proxy_index
                    .map(|idx| format!("{:03}", idx))
                    .unwrap_or_else(|| "DIR".to_string());

                let context = TaskContext::new(client, config.clone(), db.clone());
                let result = tokio::time::timeout(context.timeout, task.run(&context)).await;
                let duration = start.elapsed();

                let (success, tx_hash, error) = match result {
                    Ok(Ok(res)) => (
                        res.success,
                        res.tx_hash,
                        if res.success { None } else { Some(res.message) },
                    ),
                    Ok(Err(e)) => (false, None, Some(format!("{:?}", e))),
                    Err(_) => (false, None, Some("Timeout".to_string())),
                };

                let run_result = TaskRunResult {
                    id,
                    name: task_name,
                    success,
                    duration,
                    wallet_idx: current_wallet_idx,
                    proxy_url: format!("Proxy {}", proxy_idx_str),
                    tx_hash,
                    error,
                };

                if success {
                    return run_result;
                }

                // Identify transient errors for retry
                let err_str = run_result
                    .error
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase();
                let is_transient = err_str.contains("proxy")
                    || err_str.contains("tunnel")
                    || err_str.contains("nonce")
                    || err_str.contains("timeout")
                    || err_str.contains("auth")
                    || err_str.contains("connect")
                    || err_str.contains("balance")
                    || err_str.contains("insufficient");

                if !is_transient {
                    return run_result;
                }

                last_run = Some(run_result);

                // If it was a balance issue, pick a different wallet for the next try
                if err_str.contains("balance") || err_str.contains("insufficient") {
                    current_wallet_idx = rand::thread_rng().gen_range(0..total_wallets);
                }

                // Simple backoff
                tokio::time::sleep(tokio::time::Duration::from_millis(1000 * attempt as u64)).await;
            }

            last_run.expect("Loop should run at least once")
        }
    });

    // buffered(10) ensures 10 run in parallel, but results are yielded in order
    let mut stream = task_stream.buffered(10);

    while let Some(res) = stream.next().await {
        print!(
            "Running Task {:02}: {:<30} | Wallet {:02} | {}... ",
            res.id, res.name, res.wallet_idx, res.proxy_url
        );
        std::io::stdout().flush()?;

        if res.success {
            println!("\x1b[32m✅ {:.2}s\x1b[0m", res.duration.as_secs_f64());
        } else {
            println!(
                "\x1b[31m❌ {:.2}s - Error: {}\x1b[0m",
                res.duration.as_secs_f64(),
                res.error.as_deref().unwrap_or_default()
            );
        }

        results.push(res);
    }

    // Console Summary
    let total_tasks = results.len();
    let passed_tasks = results.iter().filter(|r| r.success).count();
    let success_rate = (passed_tasks as f64 / total_tasks as f64) * 100.0;
    let avg_duration = results
        .iter()
        .map(|r| r.duration.as_secs_f64())
        .sum::<f64>()
        / total_tasks as f64;
    let longest_task = results.iter().max_by_key(|r| r.duration).unwrap();

    let rate_color = if success_rate >= 90.0 {
        "\x1b[32m"
    } else if success_rate >= 70.0 {
        "\x1b[33m"
    } else {
        "\x1b[31m"
    };
    let cyan = "\x1b[36m";
    let magenta = "\x1b[35m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    println!(
        "\n{}================ REPORT ================{}",
        bold, reset
    );
    println!(
        "Task success rate: {}{:.2}% ({} of {}){}",
        rate_color, success_rate, passed_tasks, total_tasks, reset
    );
    println!(
        "Average task duration: {}{:.2}s{}",
        cyan, avg_duration, reset
    );
    println!(
        "Longest task duration: {}{}Task {:02} ({:.2}s){}",
        magenta,
        bold,
        longest_task.id,
        longest_task.duration.as_secs_f64(),
        reset
    );
    println!("------------------------------------------");
    println!("Full report saved to: debug_report.md\n");

    // Generate Report File
    let mut file = std::fs::File::create("debug_report.md")?;
    writeln!(file, "# Tempo Debug Report")?;
    writeln!(
        file,
        "*Generated: {}*",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    )?;
    writeln!(
        file,
        "\n| # | Task Name | Status | Duration | Wallet | Proxy | Tx Hash | Error |"
    )?;
    writeln!(file, "|---|---|---|---|---|---|---|---|")?;

    for res in &results {
        let status = if res.success { "✅" } else { "❌" };
        let hash_short = res
            .tx_hash
            .as_deref()
            .map(|h| {
                if h.len() > 10 {
                    format!("{}...", &h[0..10])
                } else {
                    h.to_string()
                }
            })
            .unwrap_or("-".to_string());
        let error_msg = res.error.as_deref().unwrap_or("-");

        writeln!(
            file,
            "| {:02} | {} | {} | {:.2}s | {} | {} | {} | {} |",
            res.id,
            res.name,
            status,
            res.duration.as_secs_f64(),
            res.wallet_idx,
            res.proxy_url,
            hash_short,
            error_msg
        )?;
    }

    Ok(())
}
