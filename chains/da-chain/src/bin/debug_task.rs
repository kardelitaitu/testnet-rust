use anyhow::Result;
use clap::Parser;
use core_logic::setup_logger;
use dialoguer::{theme::ColorfulTheme, Select};
use dotenv::dotenv;
use ethers::prelude::*;
use reqwest;
use da_chain_project::config::DaChainConfig;
use da_chain_project::task::{
    t01_check_balance::DaChainCheckBalanceTask,
    t02_simple_native_transfer::SimpleNativeTransferTask,
    DaChainTask, TaskContext,
};
use std::env;
use std::sync::Arc;
use tracing::{error, info};
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "chains/da-chain/config.toml")]
    config: String,

    #[arg(short, long)]
    task: Option<usize>,

    #[arg(long)]
    wallet: Option<usize>,

    #[arg(long)]
    all: bool,

    #[arg(long)]
    no_proxy: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logger();
    dotenv().ok();

    println!("--- DA-CHAIN Debugger ---");

    let args = Args::parse();

    // 1. Load Config
    let cfg = match DaChainConfig::load(&args.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load config: {}", e);
            return Ok(());
        }
    };
    info!("Loaded config for chain ID: {}", cfg.chain_id);

    // 2. Load Wallets
    let password = env::var("WALLET_PASSWORD").ok();
    let manager = if let Some(ref dir) = cfg.wallet_dir {
        core_logic::WalletManager::with_wallet_dir(dir)?
    } else {
        core_logic::WalletManager::new()?
    };

    // Initialize Address Cache
    da_chain_project::utils::address_cache::AddressCache::init()?;
    println!("Address cache initialized.");

    let total_wallets = manager.count();
    if total_wallets == 0 {
        println!("No wallet files found.");
        return Ok(());
    }

    // 3. Select wallet
    let wallet_idx = if let Some(idx) = args.wallet {
        idx.min(total_wallets - 1)
    } else if args.all {
        0 // Will iterate all
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

    // 4. Verify decryption with selected wallet
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
    headers.insert(reqwest::header::USER_AGENT, reqwest::header::HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"));
    let client_builder = reqwest::Client::builder().default_headers(headers);
    let client = if let Some(ref proxy_str) = proxy_url {
        println!("Using proxy: {}", proxy_str.split('@').last().unwrap_or("..."));
        match reqwest::Proxy::all(proxy_str) {
            Ok(p) => client_builder.proxy(p).build().unwrap_or(reqwest::Client::new()),
            Err(_) => client_builder.build().unwrap_or(reqwest::Client::new()),
        }
    } else {
        client_builder.build().unwrap_or(reqwest::Client::new())
    };

    let provider = Provider::new(Http::new_with_client(
        Url::parse(&cfg.rpc_url)?,
        client,
    ));

    // 6. Create task list
    let tasks: Vec<Box<dyn DaChainTask>> = vec![
        Box::new(DaChainCheckBalanceTask),
        Box::new(SimpleNativeTransferTask),
    ];
    let items: Vec<&str> = tasks.iter().map(|t| t.name()).collect();

    // 7. Select task
    let task_idx = if let Some(t_id) = args.task {
        let prefix1 = format!("{}_", t_id);
        let prefix2 = format!("{:02}_", t_id);
        if let Some(pos) = tasks.iter().position(|t| {
            t.name().starts_with(&prefix1) || t.name().starts_with(&prefix2)
        }) {
            pos
        } else if t_id < tasks.len() {
            println!("??  Warning: Task ID {} not found by name, using index {}", t_id, t_id);
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
    let gas_manager = Arc::new(da_chain_project::utils::gas::GasManager::new(
        Arc::new(provider.clone()),
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
