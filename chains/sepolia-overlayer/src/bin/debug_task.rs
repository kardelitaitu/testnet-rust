use anyhow::Result;
use clap::Parser;
use core_logic::setup_logger;
use dialoguer::{theme::ColorfulTheme, Select};
use ethers::prelude::*;
use sepolia_overlayer::config::SepoliaConfig;
use sepolia_overlayer::task::{
    t01_check_balance::SepoliaCheckBalanceTask, t02_mint_usdt_plus::MintUsdtPlusTask,
    t03_mint_usdc_plus::MintUsdcPlusTask, t04_redeem_usdt_plus::RedeemUsdtPlusTask,
    t05_redeem_usdc_plus::RedeemUsdcPlusTask, t06_stake_usdt_plus::StakeUsdtPlusTask,
    t07_stake_usdc_plus::StakeUsdcPlusTask, t08_unstake_tplus::UnstakeTplusTask,
    t09_unstake_cplus::UnstakeCplusTask, t10_aave_usdt_faucet::AaveUsdtFaucetTask,
    t11_aave_usdc_faucet::AaveUsdcFaucetTask, t12_bridge_tplus::BridgeTplusTask,
    t13_bridge_cplus::BridgeCplusTask, t14_send_random_usdt_plus::SendRandomUsdtPlusTask,
    t15_send_random_usdc_plus::SendRandomUsdcPlusTask, t16_bridge_back_tplus::BridgeBackTplusTask,
    t17_bridge_back_cplus::BridgeBackCplusTask, t18_receive_tplus::ReceiveTplusTask,
    t19_receive_cplus::ReceiveCplusTask, SepoliaTask, TaskContext,
};
use std::env;
use std::path::Path;
use std::sync::Arc;
use tracing::{error, info};
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "chains/sepolia-overlayer/config.toml")]
    config: String,

    #[arg(long)]
    base_config: Option<String>,

    #[arg(short, long)]
    task: Option<usize>,

    #[arg(long)]
    wallet: Option<usize>,

    #[arg(long)]
    all: bool,

    #[arg(long)]
    no_proxy: bool,

    #[arg(long, default_value_t = 0.01)]
    min_gwei: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logger();

    println!("--- Sepolia Debugger ---");

    let args = Args::parse();

    // Load .env from same directory as config (skip global dotenv() to avoid root .env conflicts)
    if let Some(parent) = Path::new(&args.config).parent() {
        let env_path = parent.join(".env");
        if env_path.exists() {
            let _ = dotenv::from_path(&env_path);
        }
    }

    // 1. Load Config
    let cfg = match SepoliaConfig::load(&args.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load config: {}", e);
            return Ok(());
        }
    };
    info!("Loaded config for chain ID: {}", cfg.chain_id);

    // Load base Sepolia config if --base-config is provided
    let (_base_cfg, _base_env_loaded) = if let Some(ref base_path) = args.base_config {
        if let Some(parent) = Path::new(base_path).parent() {
            let env_path = parent.join(".env");
            if env_path.exists() {
                let _ = dotenv::from_path(&env_path);
            }
        }
        match SepoliaConfig::load(base_path) {
            Ok(c) => {
                info!("Loaded base config for chain ID: {}", c.chain_id);
                (Some(c), true)
            }
            Err(e) => {
                error!("Failed to load base config: {}", e);
                return Ok(());
            }
        }
    } else {
        (None, false)
    };

    // 2. Load Wallets
    let password = env::var("WALLET_PASSWORD").ok();
    let manager = if let Some(ref dir) = cfg.wallet_dir {
        core_logic::WalletManager::with_wallet_dir(dir)?
    } else {
        core_logic::WalletManager::new()?
    };

    let total_wallets = manager.count();
    if total_wallets == 0 {
        println!("No wallet files found.");
        return Ok(());
    }

    // 3. Select wallet
    let wallet_idx = if let Some(idx) = args.wallet {
        idx.min(total_wallets - 1)
    } else if args.all {
        0
    } else {
        println!("\n? Select Wallet to debug >");
        let wallet_names = manager.list_wallets();
        let mut choices = vec!["Pick Random Wallet".to_string()];
        for (i, name) in wallet_names.iter().enumerate() {
            choices.push(format!("Wallet {}: {}", i, name));
        }

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select Wallet")
            .default(0)
            .items(&choices)
            .interact()?;

        if selection == 0 {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            rng.gen_range(0..total_wallets)
        } else {
            selection - 1
        }
    };

    // 4. Verify decryption
    if let Err(e) = manager.get_wallet(wallet_idx, password.as_deref()).await {
        println!("\n??  Decryption failed for wallet {}: {}", wallet_idx, e);
        println!("Please check WALLET_PASSWORD environment variable.");
        return Ok(());
    }
    println!("Wallet decryption verified.");

    // 5. Create provider
    let proxy_url = if args.no_proxy {
        None
    } else if std::path::Path::new("proxies.txt").exists() {
        let content = std::fs::read_to_string("proxies.txt")?;
        let proxies: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        if !proxies.is_empty() {
            Some(proxies[wallet_idx % proxies.len()].to_string())
        } else {
            None
        }
    } else {
        None
    };

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        ),
    );
    let client_builder = reqwest::Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(30));
    let client = if let Some(ref proxy_str) = proxy_url {
        println!(
            "Using proxy: {}",
            proxy_str.split('@').last().unwrap_or("...")
        );
        match reqwest::Proxy::all(proxy_str) {
            Ok(p) => client_builder
                .proxy(p)
                .build()
                .unwrap_or(reqwest::Client::new()),
            Err(_) => client_builder.build().unwrap_or(reqwest::Client::new()),
        }
    } else {
        client_builder.build().unwrap_or(reqwest::Client::new())
    };

    let provider = Provider::new(Http::new_with_client(Url::parse(&cfg.rpc_url)?, client));

    // 6. Create task list
    let tasks: Vec<Box<dyn SepoliaTask>> = vec![
        Box::new(SepoliaCheckBalanceTask),
        Box::new(MintUsdtPlusTask),
        Box::new(MintUsdcPlusTask),
        Box::new(RedeemUsdtPlusTask),
        Box::new(RedeemUsdcPlusTask),
        Box::new(StakeUsdtPlusTask),
        Box::new(StakeUsdcPlusTask),
        Box::new(UnstakeTplusTask),
        Box::new(UnstakeCplusTask),
        Box::new(AaveUsdtFaucetTask),
        Box::new(AaveUsdcFaucetTask),
        Box::new(BridgeTplusTask),
        Box::new(BridgeCplusTask),
        Box::new(SendRandomUsdtPlusTask),
        Box::new(SendRandomUsdcPlusTask),
        Box::new(BridgeBackTplusTask),
        Box::new(BridgeBackCplusTask),
        Box::new(ReceiveTplusTask),
        Box::new(ReceiveCplusTask),
    ];
    let items: Vec<&str> = tasks.iter().map(|t| t.name()).collect();

    // 7. Select task
    let task_idx = if let Some(t_id) = args.task {
        let prefix1 = format!("{}_", t_id);
        let prefix2 = format!("{:02}_", t_id);
        if let Some(pos) = tasks
            .iter()
            .position(|t| t.name().starts_with(&prefix1) || t.name().starts_with(&prefix2))
        {
            pos
        } else if t_id < tasks.len() {
            println!(
                "??  Warning: Task ID {} not found by name, using index {}",
                t_id, t_id
            );
            t_id
        } else {
            error!("Invalid task ID: {}", t_id);
            return Ok(());
        }
    } else {
        Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select task to debug")
            .default(0)
            .items(&items)
            .interact()?
    };

    let selected_task = &tasks[task_idx];
    println!("\nDebugging Task: {}", selected_task.name());

    // 8. Get wallet
    let decrypted = manager.get_wallet(wallet_idx, password.as_deref()).await?;
    let key = decrypted.evm_private_key.clone();
    let wallet = key.parse::<LocalWallet>()?.with_chain_id(cfg.chain_id);
    println!("Using wallet: {:?}", wallet.address());

    // 9. Create GasManager
    let gas_manager = Arc::new(sepolia_overlayer::utils::gas::GasManager::new(
        Arc::new(provider.clone()),
        args.min_gwei,
    ));

    // 10. Create TaskContext
    let ctx = TaskContext {
        provider: provider.clone(),
        wallet,
        config: cfg.clone(),
        proxy: proxy_url,
        db: None,
        gas_manager,
    };

    // 11. Run task
    println!("Running...");
    let start = std::time::Instant::now();
    match selected_task.run(ctx).await {
        Ok(res) => {
            if res.success {
                println!("\n? Success:");
                println!("{}", res.message);
            } else {
                println!("\n? Failed:");
                println!("{}", res.message);
            }
        }
        Err(e) => {
            println!("\n?? Task Error: {:?}", e);
        }
    }
    println!("\n? Duration: {:?}", start.elapsed());

    Ok(())
}
