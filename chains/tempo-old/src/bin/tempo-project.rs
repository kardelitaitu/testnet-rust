use anyhow::Result;
use dotenv::dotenv;
use ethers::prelude::*;
use rand::Rng;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, Instrument};

use core_logic::database::DatabaseManager;
use tempo_project::config::TempoConfig;
use tempo_project::tasks::{TaskContext, TempoTask};
use tempo_project::utils::gas_manager::GasManager;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    // Use Risechain-style logger
    core_logic::utils::setup_logger();

    // We use println! for startup messages to bypass the strict filter in setup_logger
    println!("🚀 Tempo Project Spammer - Starting...");

    // 1. Load Config
    let cfg = TempoConfig {
        rpc_url: env::var("TEMPO_RPC_URL")
            .unwrap_or_else(|_| "https://rpc.moderato.tempo.xyz".to_string()),
        chain_id: 42431,
        worker_count: env::var("WORKER_COUNT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1),
    };
    println!("Connected to RPC: {}", cfg.rpc_url);

    // 2. Load Wallets
    let password = env::var("WALLET_PASSWORD").ok();
    let wallet_manager = core_logic::utils::WalletManager::new()?;
    let total_wallets = wallet_manager.count();

    if total_wallets == 0 {
        error!("No wallets found in wallet-json directory via Manager.");
        return Ok(());
    }
    println!("Loaded {} wallets.", total_wallets);

    // 3. Load Proxies
    let proxies = core_logic::utils::ProxyManager::load_proxies()?;
    if !proxies.is_empty() {
        println!("Loaded {} proxies.", proxies.len());
    } else {
        println!("No proxies found (running direct).");
    }

    // 4. Setup Dependencies (Base)
    let gas_manager = Arc::new(GasManager);
    let db_manager = Arc::new(DatabaseManager::new("tempo.db").await.unwrap());

    // 5. Define Tasks
    let tasks: Vec<Box<dyn TempoTask>> = vec![
        Box::new(tempo_project::tasks::t01_deploy_contract::DeployContractTask),
        Box::new(tempo_project::tasks::t02_claim_faucet::ClaimFaucetTask),
        Box::new(tempo_project::tasks::t03_send_token::SendTokenTask),
        Box::new(tempo_project::tasks::t04_create_stable::CreateStableTask),
        Box::new(tempo_project::tasks::t05_swap_stable::SwapStableTask),
        // Box::new(tempo_project::tasks::t06_add_liquidity::AddLiquidityTask),
        // Box::new(tempo_project::tasks::t07_mint_stable::MintStableTask),
        // Box::new(tempo_project::tasks::t08_burn_stable::BurnStableTask),
        // Box::new(tempo_project::tasks::t09_transfer_token::TransferTokenTask),
        // Box::new(tempo_project::tasks::t10_transfer_memo::TransferMemoTask),
        // Box::new(tempo_project::tasks::t11_limit_order::LimitOrderTask),
        // Box::new(tempo_project::tasks::t12_remove_liquidity::RemoveLiquidityTask),
        // Box::new(tempo_project::tasks::t13_grant_role::GrantRoleTask),
        // Box::new(tempo_project::tasks::t14_nft_create_mint::NftCreateMintTask),
        // Box::new(tempo_project::tasks::t15_mint_domain::MintDomainTask),
        // Box::new(tempo_project::tasks::t22_mint_meme::MintMemeTask),
        // Box::new(tempo_project::tasks::t28_disperse_system::DisperseSystemTask),
        // Box::new(tempo_project::tasks::t32_concurrent_stable::ConcurrentStableTask),
    ];

    if tasks.is_empty() {
        error!("No tasks registered.");
        return Ok(());
    }

    println!("Registered {} tasks.", tasks.len());

    // 6. Main Loop
    println!("Starting {} workers...", cfg.worker_count);

    // Import colors
    use nu_ansi_term::{Color, Style};
    use url::Url;

    // Wrap shared state
    let wallet_manager = Arc::new(wallet_manager);
    let proxies = Arc::new(proxies);
    let tasks = Arc::new(tasks);
    let db_manager = db_manager.clone();

    let mut handles = Vec::new();

    for _worker_id in 0..cfg.worker_count {
        let wm = wallet_manager.clone();
        let px = proxies.clone();
        let ts = tasks.clone();
        let gm = gas_manager.clone();
        let db = db_manager.clone();
        let cf = cfg.clone();
        let pass = password.clone(); // Capture password

        let handle = tokio::spawn(async move {
            let initial_sleep = {
                let mut rng = rand::thread_rng();
                rng.gen_range(0..2000)
            };
            tokio::time::sleep(Duration::from_millis(initial_sleep)).await;

            loop {
                let wallet_idx = {
                    let mut rng = rand::thread_rng();
                    rng.gen_range(0..wm.count())
                };

                // Decrypt
                let wallet_res = wm.get_wallet(wallet_idx, pass.as_deref());
                match wallet_res {
                    Ok(decrypted) => {
                        match decrypted.evm_private_key.parse::<LocalWallet>() {
                            Ok(w) => {
                                let wallet = w.with_chain_id(cf.chain_id);

                                // Pick Random Task
                                let task_idx = {
                                    let mut rng = rand::thread_rng();
                                    rng.gen_range(0..ts.len())
                                };
                                let task = &ts[task_idx];

                                // Proxy Selection
                                let (proxy_config, proxy_id_str) = if !px.is_empty() {
                                    let idx = {
                                        let mut rng = rand::thread_rng();
                                        rng.gen_range(0..px.len())
                                    };
                                    (Some(&px[idx]), format!("{:03}", idx + 1))
                                } else {
                                    (None, "000".to_string())
                                };

                                // Build Provider
                                let provider_res = if let Some(p) = proxy_config {
                                    let mut proxy_builder = reqwest::Proxy::all(&p.url).unwrap(); // Unwrap or handle
                                    if let (Some(u), Some(pass)) = (&p.username, &p.password) {
                                        proxy_builder = proxy_builder.basic_auth(u, pass);
                                    }
                                    let client = reqwest::Client::builder()
                                        .proxy(proxy_builder)
                                        .build()
                                        .unwrap();
                                    Provider::new(Http::new_with_client(
                                        Url::parse(&cf.rpc_url).unwrap(),
                                        client,
                                    ))
                                } else {
                                    Provider::<Http>::try_from(&cf.rpc_url).unwrap()
                                };
                                let provider_arc = Arc::new(provider_res);

                                let ctx = TaskContext {
                                    provider: provider_arc.clone(),
                                    wallet: wallet.clone(),
                                    config: cf.clone(),
                                    gas_manager: gm.clone(),
                                    db: Some(db.clone()),
                                };

                                // Logging Identifiers
                                let wk_str = format!("{:03}", wallet_idx + 1);
                                let prefix =
                                    format!("[WK:{}][WL:{}][P:{}]", wk_str, wk_str, proxy_id_str);
                                let prefix_colored =
                                    Style::new().dimmed().paint(prefix).to_string();

                                // Context Span
                                let span = tracing::info_span!("task", worker_id = %wk_str, wallet_id = %wk_str, proxy_id = %proxy_id_str);
                                let start = std::time::Instant::now();

                                // Run Task
                                let run_future = task.run(ctx).instrument(span.clone());
                                let timeout_duration = Duration::from_secs(60);

                                match tokio::time::timeout(timeout_duration, run_future).await {
                                    Ok(run_res) => {
                                        match run_res {
                                            Ok(res) => {
                                                let _enter = span.enter();
                                                let duration = start.elapsed();
                                                let dur_secs = duration.as_secs_f64();

                                                // Duration Color
                                                let dur_colored = if dur_secs < 5.0 {
                                                    Style::new()
                                                        .fg(Color::LightGreen)
                                                        .paint(format!("{:.1}s", dur_secs))
                                                } else if dur_secs < 10.0 {
                                                    Style::new()
                                                        .fg(Color::Yellow)
                                                        .paint(format!("{:.1}s", dur_secs))
                                                } else {
                                                    Style::new()
                                                        .fg(Color::LightRed)
                                                        .paint(format!("{:.1}s", dur_secs))
                                                };

                                                // Message Processing
                                                let msg_final = if let Some(tx_hash) = res.tx_hash {
                                                    let raw_hash = tx_hash.trim_matches('"');
                                                    let colored_hash = Style::new()
                                                        .fg(Color::LightYellow)
                                                        .paint(raw_hash)
                                                        .to_string();
                                                    if res.message.contains(raw_hash) {
                                                        res.message.replace(raw_hash, &colored_hash)
                                                    } else {
                                                        res.message.clone()
                                                    }
                                                } else {
                                                    res.message
                                                };

                                                if res.success {
                                                    let _ = msg_final; // unused warning
                                                    info!(target: "task_result", "{} Success [{}] {} in {}", prefix_colored, task.name(), msg_final, dur_colored);
                                                } else {
                                                    info!(target: "task_result", "{} Failed  [{}] {} in {}", prefix_colored, task.name(), msg_final, dur_colored);
                                                }
                                            }
                                            Err(e) => {
                                                let _enter = span.enter();
                                                error!("💥 Task Error: {:?}", e);
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        let _enter = span.enter();
                                        error!("⏰ Task timed out (limit: 60s)");
                                    }
                                }
                            }
                            Err(e) => {
                                error!(
                                    "Failed to parse private key for wallet {}: {}",
                                    wallet_idx, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to decrypt wallet {}: {}", wallet_idx, e);
                        if pass.is_none() {
                            break;
                        }
                    }
                } // end match wallet_res

                let sleep_sec = {
                    let mut rng = rand::thread_rng();
                    rng.gen_range(5..15)
                };
                tokio::time::sleep(Duration::from_secs(sleep_sec)).await;
            }
        });
        handles.push(handle);
    }

    futures::future::join_all(handles).await;
    Ok(())
}
