use da_chain_project::config::DaChainConfig;
use anyhow::Result;
use clap::Parser;
use core_logic::WalletManager;
use dotenv::dotenv;
use ethers::prelude::*;
use std::env;
use tracing::info;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "chains/da-chain/config.toml")]
    config: String,
    #[arg(short, long)]
    task: String,
    #[arg(short, long)]
    wallet: Option<usize>,
    #[arg(long)]
    all: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let args = Args::parse();
    
    let config = DaChainConfig::load(&args.config)?;
    info!("Loaded config for chain ID: {}", config.chain_id);

    let manager = WalletManager::new()?;
    let password = env::var("WALLET_PASSWORD").ok();

    let wallet_idx = args.wallet.unwrap_or(0);
    let decrypted = manager.get_wallet(wallet_idx, password.as_deref()).await?;
    let key = decrypted.evm_private_key.clone();
    let wallet = key.parse::<LocalWallet>()?.with_chain_id(config.chain_id);

    info!("Using wallet: {:?}", wallet.address());

    // Task debugging not fully implemented yet
    info!("Task debugging not fully implemented yet. Use --task to specify task name.");
    
    Ok(())
}
