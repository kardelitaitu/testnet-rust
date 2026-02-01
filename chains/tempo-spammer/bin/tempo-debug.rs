use anyhow::{Context, Result};
use clap::Parser;
use core_logic::WalletManager;
use core_logic::database::DatabaseManager;
use core_logic::traits::TaskResult;
use dotenv::dotenv;
use rand::Rng;
use std::env;
use std::sync::Arc;
use tempo_spammer::TempoClient;
use tempo_spammer::config::TempoSpammerConfig;
use tempo_spammer::tasks::{TaskContext, TempoTask, load_proxies};
use tracing;
use tracing_subscriber;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to config.toml
    #[arg(short, long, default_value = "config/config.toml")]
    config: String,

    /// Task to run (name or number)
    #[arg(short, long)]
    task: String,

    /// Wallet index to use
    #[arg(short, long, default_value = "0")]
    wallet: usize,

    /// Proxy index (optional, random if not specified, 0 for direct)
    #[arg(short, long)]
    proxy: Option<usize>,

    /// Skip database logging
    #[arg(long, default_value = "false")]
    no_db: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    // Initialize tracing for debug mode (show everything)
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false) // cleaner output without targets
        .init();

    let args = Args::parse();

    // Load config
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
    println!(
        "Loaded config: {} (chain {})",
        config.rpc_url, config.chain_id
    );

    // Load wallet
    let wallet_password = env::var("WALLET_PASSWORD").ok();
    let wallet_manager = WalletManager::new()?;
    let total_wallets = wallet_manager.count();

    if total_wallets == 0 {
        println!("❌ No wallets found");
        return Ok(());
    }

    if args.wallet >= total_wallets {
        println!(
            "❌ Wallet {} not found (have {})",
            args.wallet, total_wallets
        );
        return Ok(());
    }

    println!("Using wallet {} of {}", args.wallet, total_wallets);

    // Get the directory containing the config file
    let config_path_obj = std::path::Path::new(&config_path);
    let config_dir = config_path_obj
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let proxies_path = config_dir.join("proxies.txt");

    // Load proxies
    let proxy_path_str = proxies_path.to_str().unwrap_or("config/proxies.txt");
    let proxies = load_proxies(proxy_path_str)?;
    println!(
        "Loaded {} proxies from {}",
        proxies.len(),
        proxies_path.display()
    );

    // Select proxy (random by default)
    let (proxy_idx, proxy) = if proxies.is_empty() {
        // No proxies available, use direct
        println!("No proxies available, using direct connection");
        (None, None)
    } else if let Some(idx) = args.proxy {
        if idx == 0 {
            // Explicit direct connection via --proxy 0
            println!("Using direct connection (--proxy 0)");
            (None, None)
        } else if idx > 0 && idx <= proxies.len() {
            // Specific proxy (1-indexed)
            (Some(idx - 1), Some(&proxies[idx - 1]))
        } else {
            println!(
                "⚠️  Proxy index {} out of range (have {}), using random proxy",
                idx,
                proxies.len()
            );
            let random_idx = rand::thread_rng().gen_range(0..proxies.len());
            (Some(random_idx), Some(&proxies[random_idx]))
        }
    } else {
        // Random proxy by default
        let random_idx = rand::thread_rng().gen_range(0..proxies.len());
        (Some(random_idx), Some(&proxies[random_idx]))
    };

    // Get private key
    let decrypted = wallet_manager
        .get_wallet(args.wallet, wallet_password.as_deref())
        .await?;

    // Create client with proxy
    let client = TempoClient::new(
        &config.rpc_url,
        &decrypted.evm_private_key,
        proxy,
        proxy_idx,
    )
    .await?;
    println!("Wallet address: {:?}", client.address());
    println!(
        "Using proxy {}: {}",
        proxy_idx.unwrap_or(0) + 1,
        proxy.map(|p| &p.url).unwrap_or(&"direct".to_string())
    );

    // Define available tasks
    let tasks: Vec<(usize, &str, &str, Box<dyn TempoTask>)> = vec![
        (
            1,
            "01_deploy_contract",
            "Deploy Counter Contract",
            Box::new(tempo_spammer::tasks::t01_deploy_contract::DeployContractTask::new())
                as Box<dyn TempoTask>,
        ),
        (
            2,
            "02_claim_faucet",
            "Claim Faucet",
            Box::new(tempo_spammer::tasks::t02_claim_faucet::ClaimFaucetTask::new()),
        ),
        (
            3,
            "03_send_token",
            "Send Token",
            Box::new(tempo_spammer::tasks::t03_send_token::SendTokenTask::new()),
        ),
        (
            4,
            "04_create_stable",
            "Create Stablecoin",
            Box::new(tempo_spammer::tasks::t04_create_stable::CreateStableTask::new()),
        ),
        (
            5,
            "05_swap_stable",
            "Swap Stablecoin",
            Box::new(tempo_spammer::tasks::t05_swap_stable::SwapStableTask::new()),
        ),
        (
            6,
            "06_add_liquidity",
            "Add Liquidity",
            Box::new(tempo_spammer::tasks::t06_add_liquidity::AddLiquidityTask::new()),
        ),
        (
            7,
            "07_mint_stable",
            "Mint Stablecoin",
            Box::new(tempo_spammer::tasks::t07_mint_stable::MintStableTask::new()),
        ),
        (
            8,
            "08_burn_stable",
            "Burn Stablecoin",
            Box::new(tempo_spammer::tasks::t08_burn_stable::BurnStableTask::new()),
        ),
        (
            9,
            "09_transfer_token",
            "Transfer Token",
            Box::new(tempo_spammer::tasks::t09_transfer_token::TransferTokenTask::new()),
        ),
        (
            10,
            "10_transfer_memo",
            "Transfer with Memo",
            Box::new(tempo_spammer::tasks::t10_transfer_memo::TransferMemoTask::new()),
        ),
        (
            11,
            "11_limit_order",
            "Limit Order",
            Box::new(tempo_spammer::tasks::t11_limit_order::LimitOrderTask::new()),
        ),
        (
            12,
            "12_remove_liquidity",
            "Remove Liquidity",
            Box::new(tempo_spammer::tasks::t12_remove_liquidity::RemoveLiquidityTask::new()),
        ),
        (
            13,
            "13_grant_role",
            "Grant Role",
            Box::new(tempo_spammer::tasks::t13_grant_role::GrantRoleTask::new()),
        ),
        (
            14,
            "14_nft_create_mint",
            "NFT Create & Mint",
            Box::new(tempo_spammer::tasks::t14_nft_create_mint::NftCreateMintTask::new()),
        ),
        (
            15,
            "15_mint_domain",
            "Mint Domain",
            Box::new(tempo_spammer::tasks::t15_mint_domain::MintDomainTask::new()),
        ),
        (
            16,
            "16_mint_random_nft",
            "Mint Random NFT",
            Box::new(tempo_spammer::tasks::t16_mint_random_nft::MintRandomNftTask::new()),
        ),
        (
            17,
            "17_batch_eip7702",
            "Batch EIP-7702 Simulation",
            Box::new(tempo_spammer::tasks::t17_batch_eip7702::BatchEip7702Task::new()),
        ),
        (
            18,
            "18_tip403_policies",
            "TIP-403 Policies",
            Box::new(tempo_spammer::tasks::t18_tip403_policies::Tip403PoliciesTask::new()),
        ),
        (
            20,
            "20_wallet_activity",
            "Wallet Activity",
            Box::new(tempo_spammer::tasks::t20_wallet_activity::WalletActivityTask::new()),
        ),
        (
            21,
            "21_create_meme",
            "Create Meme",
            Box::new(tempo_spammer::tasks::t21_create_meme::CreateMemeTask::new()),
        ),
        (
            22,
            "22_mint_meme",
            "Mint Meme",
            Box::new(tempo_spammer::tasks::t22_mint_meme::MintMemeTask::new()),
        ),
        (
            19,
            "19_wallet_analytics",
            "Wallet Analytics",
            Box::new(tempo_spammer::tasks::t19_wallet_analytics::WalletAnalyticsTask::new()),
        ),
        (
            23,
            "23_transfer_meme",
            "Transfer Meme",
            Box::new(tempo_spammer::tasks::t23_transfer_meme::TransferMemeTask::new()),
        ),
        (
            24,
            "24_batch_swap",
            "Batch Swap",
            Box::new(tempo_spammer::tasks::t24_batch_swap::BatchSwapTask::new()),
        ),
        (
            25,
            "25_batch_system_token",
            "Batch System Token",
            Box::new(tempo_spammer::tasks::t25_batch_system_token::BatchSystemTokenTask::new()),
        ),
        (
            26,
            "26_batch_stable_token",
            "Batch Stable Token",
            Box::new(tempo_spammer::tasks::t26_batch_stable_token::BatchStableTokenTask::new()),
        ),
        (
            27,
            "27_batch_meme_token",
            "Batch Meme Token",
            Box::new(tempo_spammer::tasks::t27_batch_meme_token::BatchMemeTokenTask::new()),
        ),
        (
            28,
            "28_multi_send_disperse",
            "Multi-Send Disperse",
            Box::new(tempo_spammer::tasks::t28_multi_send_disperse::MultiSendDisperseTask::new()),
        ),
        (
            29,
            "29_multi_send_disperse_stable",
            "Multi-Send Disperse Stable",
            Box::new(tempo_spammer::tasks::t29_multi_send_disperse_stable::MultiSendDisperseStableTask::new()),
        ),
        (
            30,
            "30_multi_send_disperse_meme",
            "Multi-Send Disperse Meme",
            Box::new(tempo_spammer::tasks::t30_multi_send_disperse_meme::MultiSendDisperseMemeTask::new()),
        ),
        (
            31,
            "31_multi_send_concurrent",
            "Multi-Send Concurrent",
            Box::new(tempo_spammer::tasks::t31_multi_send_concurrent::MultiSendConcurrentTask::new()),
        ),
        (
            32,
            "32_multi_send_concurrent_stable",
            "Multi-Send Concurrent Stable",
            Box::new(tempo_spammer::tasks::t32_multi_send_concurrent_stable::MultiSendConcurrentStableTask::new()),
        ),
        (
            33,
            "33_multi_send_concurrent_meme",
            "Multi-Send Concurrent Meme",
            Box::new(tempo_spammer::tasks::t33_multi_send_concurrent_meme::MultiSendConcurrentMemeTask::new()),
        ),
        (
            34,
            "34_batch_send_transaction",
            "Batch Send Transaction",
            Box::new(tempo_spammer::tasks::t34_batch_send_transaction::BatchSendTransactionTask::new()),
        ),
        (
            35,
            "35_batch_send_transaction_stable",
            "Batch Send Transaction Stable",
            Box::new(tempo_spammer::tasks::t35_batch_send_transaction_stable::BatchSendTransactionStableTask::new()),
        ),
        (
            36,
            "36_batch_send_transaction_meme",
            "Batch Send Transaction Meme",
            Box::new(tempo_spammer::tasks::t36_batch_send_transaction_meme::BatchSendTransactionMemeTask::new()),
        ),
        (
            37,
            "37_transfer_later",
            "Transfer Later",
            Box::new(tempo_spammer::tasks::t37_transfer_later::TransferLaterTask::new()),
        ),
        (
            38,
            "38_transfer_later_stable",
            "Transfer Later Stable",
            Box::new(tempo_spammer::tasks::t38_transfer_later_stable::TransferLaterStableTask::new()),
        ),
        (
            39,
            "39_transfer_later_meme",
            "Transfer Later Meme",
            Box::new(tempo_spammer::tasks::t39_transfer_later_meme::TransferLaterMemeTask::new()),
        ),
        (
            40,
            "40_distribute_shares",
            "Distribute Shares",
            Box::new(tempo_spammer::tasks::t40_distribute_shares::DistributeSharesTask::new()),
        ),
        (
            41,
            "41_distribute_shares_stable",
            "Distribute Shares Stable",
            Box::new(tempo_spammer::tasks::t41_distribute_shares_stable::DistributeSharesStableTask::new()),
        ),
        (
            42,
            "42_distribute_shares_meme",
            "Distribute Shares Meme",
            Box::new(tempo_spammer::tasks::t42_distribute_shares_meme::DistributeSharesMemeTask::new()),
        ),
        (
            43,
            "43_batch_mint_stable",
            "Batch Mint Stable",
            Box::new(tempo_spammer::tasks::t43_batch_mint_stable::BatchMintStableTask::new()),
        ),
        (
            44,
            "44_batch_mint_meme",
            "Batch Mint Meme",
            Box::new(tempo_spammer::tasks::t44_batch_mint_meme::BatchMintMemeTask::new()),
        ),
        (
            45,
            "45_deploy_viral_faucet",
            "Deploy Viral Faucet",
            Box::new(tempo_spammer::tasks::t45_deploy_viral_faucet::DeployViralFaucetTask::new()),
        ),
        (
            46,
            "46_claim_viral_faucet",
            "Claim Viral Faucet",
            Box::new(tempo_spammer::tasks::t46_claim_viral_faucet::ClaimViralFaucetTask::new()),
        ),
        (
            47,
            "47_deploy_viral_nft",
            "Deploy Viral NFT",
            Box::new(tempo_spammer::tasks::t47_deploy_viral_nft::DeployViralNftTask::new()),
        ),
        (
            48,
            "48_mint_viral_nft",
            "Mint Viral NFT",
            Box::new(tempo_spammer::tasks::t48_mint_viral_nft::MintViralNftTask::new()),
        ),
        (
            49,
            "49_time_bomb",
            "Time Bomb",
            Box::new(tempo_spammer::tasks::t49_time_bomb::TimeBombTask::new()),
        ),
        (
            50,
            "50_deploy_storm",
            "Deploy Storm",
            Box::new(tempo_spammer::tasks::t50_deploy_storm::DeployStormTask::new()),
        ),
        (
            999,
            "check_native_balance",
            "Check Native Balance",
            Box::new(tempo_spammer::tasks::check_native_balance::CheckNativeBalanceTask::default()),
        ),
    ];

    // Find task
    #[allow(noop_method_call)]
    let task_input = args.task.to_lowercase();
    let _max_task_idx = tasks.iter().map(|(i, _, _, _)| *i).max().unwrap_or(5);
    let (task_idx, _task_key, task_desc, task) = if let Ok(idx) = task_input.parse::<usize>() {
        // Numeric index
        if idx == 0 || (idx > 50 && idx != 999) {
            panic!("Task index {} not found. Available tasks: 1-50, 999", idx);
        }
        tasks.iter().find(|(i, _, _, _)| *i == idx).unwrap()
    } else {
        // Name match
        tasks
            .iter()
            .find(|(_, name, _, _)| name.to_lowercase() == task_input || name.contains(&task_input))
            .expect("Task not found. Available tasks: 01-50")
    };

    println!("Running task {}: {}", task_idx, task_desc);

    // Initialize database if not disabled
    let db = if !args.no_db {
        match DatabaseManager::new("tempo-spammer.db").await {
            Ok(db) => Some(Arc::new(db)),
            Err(e) => {
                println!(
                    "⚠️  Failed to open database: {:?}. Continuing without DB.",
                    e
                );
                None
            }
        }
    } else {
        None
    };

    // Create context with optional database
    let ctx = TaskContext::new(client.clone(), config, db);

    // Run task with timeout
    let start_time = std::time::Instant::now();
    let result = tokio::time::timeout(ctx.timeout, task.run(&ctx)).await;

    match result {
        Ok(Ok(task_result)) => {
            let duration = start_time.elapsed();
            if task_result.success {
                println!("✅ Success: {}", task_result.message);
            } else {
                println!("⚠️  Failed: {}", task_result.message);
            }
            if let Some(hash) = task_result.tx_hash {
                println!("📎 Transaction: {}", hash);
            }
            println!("⏱️  Duration: {:.1}s", duration.as_secs_f64());
        }
        Ok(Err(e)) => {
            let duration = start_time.elapsed();
            println!("Proxy: {:?}", client.proxy_config.as_ref().map(|p| &p.url));
            println!("❌ Error: {:?}", e);
            println!("⏱️  Duration: {:.1}s", duration.as_secs_f64());
        }
        Err(_) => {
            let duration = start_time.elapsed();
            let timed_out_result = TaskResult {
                success: false,
                message: format!("Task timed out after 60s"),
                tx_hash: None,
            };
            println!("⚠️  Failed: {}", timed_out_result.message);
            println!("⏱️  Duration: {:.1}s", duration.as_secs_f64());
        }
    }

    Ok(())
}
