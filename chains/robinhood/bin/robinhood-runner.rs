use anyhow::{Context, Result};
use clap::Parser;
use core_logic::database::DatabaseManager;
use core_logic::WalletManager;
use dotenv::dotenv;
use ethers::prelude::*;
use futures::StreamExt;
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
    #[arg(long, default_value = "false")]
    no_db: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let args = Args::parse();

    let config = EvmConfig::load(&args.config).context("Failed to load config")?;
    let wallet_manager = WalletManager::new()?;
    let total_wallets = wallet_manager.count();
    let wallet_password = env::var("WALLET_PASSWORD").ok();

    let proxies = load_proxies("proxies.txt").unwrap_or_default();
    let db = if !args.no_db {
        DatabaseManager::new("robinhood.db").await.ok().map(Arc::new)
    } else {
        None
    };

    let tasks: Vec<Box<RobinhoodTask>> = vec![
        Box::new(robinhood_spammer::task::t01_check_balance::CheckBalanceTask),
        Box::new(robinhood_spammer::task::t02_simple_eth_transfer::SimpleEthTransferTask),
    ];

    println!("🚀 Starting Robinhood Runner...");

    let task_stream = futures::stream::iter(tasks)
        .enumerate()
        .map(|(id, task): (usize, Box<RobinhoodTask>)| {
            let config = config.clone();
            let db = db.clone();
            let wallet_manager = &wallet_manager;
            let wallet_password = wallet_password.clone();
            let proxies = &proxies;

            async move {
                let wallet_idx = id % total_wallets;
                let decrypted = wallet_manager
                    .get_wallet(wallet_idx, wallet_password.as_deref())
                    .await
                    .unwrap();
                let wallet: LocalWallet = decrypted
                    .evm_private_key
                    .parse::<LocalWallet>()
                    .unwrap()
                    .with_chain_id(config.chain_id);

                let proxy_conf = if proxies.is_empty() {
                    None
                } else {
                    Some(&proxies[id % proxies.len()])
                };

                let mut client_builder = reqwest::Client::builder();
                if let Some(p) = proxy_conf {
                    let mut proxy = reqwest::Proxy::all(&p.url).unwrap();
                    if let (Some(u), Some(pw)) = (&p.username, &p.password) {
                        proxy = proxy.basic_auth(u, pw);
                    }
                    client_builder = client_builder.proxy(proxy);
                }
                let client = client_builder.build().unwrap();
                let provider = Provider::new(Http::new_with_client(
                    reqwest::Url::parse(&config.rpc_url).unwrap(),
                    client,
                ));
                let provider_arc = Arc::new(provider);

                let ctx = TaskContext {
                    provider: provider_arc.clone(),
                    wallet,
                    config,
                    proxy: proxy_conf.map(|p| p.url.clone()),
                    db,
                    gas_manager: Arc::new(GasManager::new(provider_arc.clone())),
                };

                let start = std::time::Instant::now();
                let task_name = task.name().to_string();
                let res = task.run(ctx).await;
                (id, task_name, start.elapsed(), res)
            }
        });

    let mut stream = task_stream.buffered(5);
    while let Some((id, name, duration, res)) = stream.next().await {
        match res {
            Ok(r) => println!(
                "Task {:02}: {:<25} | {} | {:.2}s",
                id + 1,
                name,
                if r.success { "✅" } else { "❌" },
                duration.as_secs_f64()
            ),
            Err(e) => println!("Task {:02}: {:<25} | ❌ Error: {:?}", id + 1, name, e),
        }
    }

    Ok(())
}
