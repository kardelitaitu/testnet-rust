mod config;
mod spammer;

use anyhow::Result;
use clap::Parser;
use core_logic::{setup_logger, WorkerRunner};
// use core_logic::security::SecurityUtils;
use config::EvmConfig;
use dotenv::dotenv;
use spammer::EvmSpammer;
use std::env;
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "config.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logger();
    dotenv().ok();

    let args = Args::parse();
    info!("Loading config from: {}", args.config);

    let config = match EvmConfig::load(&args.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load config: {}", e);
            return Ok(());
        }
    };

    info!("Configuration loaded for chain ID: {}", config.chain_id);

    // Load wallets using WalletManager
    let password = env::var("WALLET_PASSWORD").ok(); // Optional, will fail if encrypted and missing
    let keys = core_logic::WalletManager::get_private_keys(password).await?;

    info!("Loaded {} keys.", keys.len());

    // Load proxies
    let proxies = core_logic::ProxyManager::load_proxies()?;
    if !proxies.is_empty() {
        info!("Loaded {} proxies for rotation.", proxies.len());
    }

    // Create spammers
    let mut spammers = Vec::new();
    for (i, key) in keys.iter().enumerate() {
        let wallet = key.parse::<ethers::signers::LocalWallet>()?;

        // Assign proxy round-robin if available
        let proxy_config = if !proxies.is_empty() {
            Some(proxies[i % proxies.len()].clone())
        } else {
            None
        };

        let spammer = EvmSpammer::new_with_signer(config.to_spam_config(), wallet, proxy_config)?;
        spammers.push(Box::new(spammer) as Box<dyn core_logic::traits::Spammer>);
    }

    // Run
    WorkerRunner::run_spammers(spammers).await?;

    Ok(())
}
