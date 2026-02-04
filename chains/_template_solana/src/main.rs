mod config;
mod spammer;

use anyhow::Result;
use clap::Parser;
use core_logic::utils::{setup_logger, WorkerRunner};
use core_logic::security::SecurityUtils;
use config::SolanaConfig;
use spammer::SolanaSpammer;
use tracing::{info, error};
use dotenv::dotenv;
use std::env;
use solana_sdk::signature::Keypair;

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
    info!("Loading Solana config from: {}", args.config);

    let config = match SolanaConfig::load(&args.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load config: {}", e);
            return Ok(());
        }
    };

    // Load decrypted wallets (EVM, SOL, SUI)
    let password = env::var("WALLET_PASSWORD").ok();
    let wallets = core_logic::utils::WalletManager::get_decrypted_wallets(password)?;
    
    info!("Loaded {} wallets.", wallets.len());

    // Load proxies via ProxyManager (Standardized)
    let proxies = core_logic::utils::ProxyManager::load_proxies()?;
    if !proxies.is_empty() {
        info!("Loaded {} proxies for rotation.", proxies.len());
    }

    // Create spammers
    let mut spammers = Vec::new();
    for (i, wallet_data) in wallets.iter().enumerate() {
        // Use sol_private_key from decrypted wallet
        // Note: wallet_data.sol_private_key should be base58 string
        if wallet_data.sol_private_key.is_empty() {
            tracing::warn!("Wallet {} has no Solana key, skipping.", i);
            continue;
        }

        let keypair = Keypair::from_base58_string(&wallet_data.sol_private_key);
        
        // Assign proxy round-robin
        let proxy_config = if !proxies.is_empty() {
            Some(proxies[i % proxies.len()].clone())
        } else {
            None
        };

        let spammer = SolanaSpammer::new_with_keypair(
            config.to_spam_config(),
            keypair,
            proxy_config
        )?;
        spammers.push(Box::new(spammer) as Box<dyn core_logic::traits::Spammer>);
    }

    // Run
    WorkerRunner::run_spammers(spammers).await?;

    Ok(())
}
