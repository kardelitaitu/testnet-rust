use anyhow::{Context, Result};
use clap::Parser;
use core_logic::database::DatabaseManager;
use core_logic::WalletManager;
use dialoguer::{theme::ColorfulTheme, Password};
use dotenv::dotenv;
use ethers::prelude::*;
use rand::Rng;
use robinhood_spammer::config::EvmConfig;
use robinhood_spammer::task::{RobinhoodTask, TaskContext};
use robinhood_spammer::utils::gas::GasManager;
use robinhood_spammer::utils::load_proxies;
use std::env;
use std::sync::Arc;
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    #[arg(short, long)]
    task: String,

    #[arg(short, long, default_value = "0")]
    wallet: usize,

    #[arg(short, long)]
    proxy: Option<usize>,

    #[arg(long, default_value = "false")]
    no_db: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .init();

    let args = Args::parse();

    let config = EvmConfig::load(&args.config).context("Failed to load config")?;
    println!("Loaded config for chain {}", config.chain_id);

    let mut wallet_password = env::var("WALLET_PASSWORD").ok();
    let wallet_manager = WalletManager::new()?;
    let total_wallets = wallet_manager.count();

    if total_wallets == 0 {
        println!("❌ No wallets found");
        return Ok(());
    }

    if args.wallet >= total_wallets {
        println!("❌ Wallet {} not found", args.wallet);
        return Ok(());
    }

    if wallet_manager
        .get_wallet(args.wallet, wallet_password.as_deref())
        .await
        .is_err()
    {
        let input = Password::with_theme(&ColorfulTheme::default())
            .with_prompt("Enter wallet password")
            .interact()?;
        wallet_password = Some(input);
    }

    let proxies = load_proxies("proxies.txt").unwrap_or_default();

    let (_proxy_idx, proxy_conf) = if proxies.is_empty() {
        (None, None)
    } else if let Some(idx) = args.proxy {
        if idx == 0 {
            (None, None)
        } else {
            (Some(idx - 1), Some(&proxies[(idx - 1) % proxies.len()]))
        }
    } else {
        let r_idx = rand::thread_rng().gen_range(0..proxies.len());
        (Some(r_idx), Some(&proxies[r_idx]))
    };

    let decrypted = wallet_manager
        .get_wallet(args.wallet, wallet_password.as_deref())
        .await?;
    let wallet: LocalWallet = decrypted
        .evm_private_key
        .parse::<LocalWallet>()?
        .with_chain_id(config.chain_id);

    let mut client_builder = reqwest::Client::builder();
    if let Some(p) = proxy_conf {
        let mut proxy = reqwest::Proxy::all(&p.url)?;
        if let (Some(u), Some(pw)) = (&p.username, &p.password) {
            proxy = proxy.basic_auth(u, pw);
        }
        client_builder = client_builder.proxy(proxy);
    }
    let client = client_builder.build()?;

    let provider = Provider::new(Http::new_with_client(
        reqwest::Url::parse(&config.rpc_url)?,
        client,
    ));
    let provider_arc = Arc::new(provider);

    println!("Wallet address: {:?}", wallet.address());

    let tasks: Vec<Box<RobinhoodTask>> = vec![
        Box::new(robinhood_spammer::task::t01_check_balance::CheckBalanceTask),
        Box::new(robinhood_spammer::task::t02_simple_eth_transfer::SimpleEthTransferTask),
    ];

    let task = tasks
        .iter()
        .find(|t| t.name().to_lowercase().contains(&args.task.to_lowercase()))
        .context("Task not found")?;

    let db = if !args.no_db {
        DatabaseManager::new("robinhood.db")
            .await
            .ok()
            .map(Arc::new)
    } else {
        None
    };

    let ctx = TaskContext {
        provider: provider_arc.clone(),
        wallet: wallet.clone(),
        config: config.clone(),
        proxy: proxy_conf.map(|p| p.url.clone()),
        db,
        gas_manager: Arc::new(GasManager::new(provider_arc.clone())),
    };

    println!("Running task: {}", task.name());
    let start = std::time::Instant::now();
    match task.run(ctx).await {
        Ok(res) => {
            println!(
                "{} Result: {}",
                if res.success { "✅" } else { "❌" },
                res.message
            );
            if let Some(h) = res.tx_hash {
                println!("Tx: {}", h);
            }
        }
        Err(e) => println!("❌ Error: {:?}", e),
    }
    println!("Time: {:.2}s", start.elapsed().as_secs_f64());

    Ok(())
}
