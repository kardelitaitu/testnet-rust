//! Decrypt Arc wallets, fetch EVM balances (USDC native gas token), and dump `address,balance` rows.

use anyhow::{Context, Result};
use arc_project::config::ArcConfig;
use clap::Parser;
use core_logic::WalletManager;
use dialoguer::{theme::ColorfulTheme, Password};
use ethers::prelude::{Address, Http, LocalWallet, Provider, U256};
use ethers::providers::Middleware;
use ethers::signers::Signer;
use ethers::utils::format_units;
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::str::FromStr;

#[derive(Parser, Debug)]
#[command(
    name = "arc-balance-dump",
    about = "Decrypt Arc wallets and dump EVM address/balance rows (USDC native gas token)"
)]
struct Args {
    /// Path to the arc config.toml
    #[arg(short, long, default_value = "chains/arc/config.toml")]
    config: String,

    /// Output file written in the repo root unless an absolute path is given
    #[arg(short, long, default_value = "arc-wallet-balances.txt")]
    output: String,
}

fn load_wallet_manager(config: &ArcConfig) -> Result<WalletManager> {
    if let Some(ref dir) = config.wallet_dir {
        WalletManager::with_wallet_dir(dir).context("Failed to load wallets from configured dir")
    } else {
        WalletManager::new().context("Failed to load wallets")
    }
}

async fn resolve_wallet_password(
    manager: &WalletManager,
    total_wallets: usize,
) -> Result<Option<String>> {
    if total_wallets == 0 {
        return Ok(None);
    }

    let mut password = env::var("WALLET_PASSWORD").ok();

    if password.is_none() || manager.get_wallet(0, password.as_deref()).await.is_err() {
        if password.is_none() {
            eprintln!("WALLET_PASSWORD environment variable is not set.");
        } else {
            eprintln!("Wallet decryption failed with the provided password.");
        }

        match Password::with_theme(&ColorfulTheme::default())
            .with_prompt("Enter wallet password")
            .interact()
        {
            Ok(input) => {
                password = Some(input);
                manager
                    .get_wallet(0, password.as_deref())
                    .await
                    .context("Interactive password also failed")?;
            }
            Err(_) => {
                anyhow::bail!(
                    "Cannot prompt for password. Set WALLET_PASSWORD before running this binary."
                );
            }
        }
    }

    Ok(password)
}

fn format_balance_4dp(balance: U256) -> Result<String> {
    // USDC on Arc has 18 decimals (per Arc docs)
    let balance_usdc: f64 = format_units(balance, 18)
        .context("Failed to format wei into USDC")?
        .parse()
        .context("Failed to parse balance as floating point")?;
    Ok(format!("{balance_usdc:.4}"))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let output_path = {
        let output = Path::new(&args.output);
        if output.is_absolute() {
            output.to_path_buf()
        } else {
            workspace_root.join(output)
        }
    };

    if let Some(parent) = Path::new(&args.config).parent() {
        let env_path = parent.join(".env");
        if env_path.exists() {
            let _ = dotenv::from_path(&env_path);
        }
    }

    let config = ArcConfig::load(&args.config).context("Failed to load config")?;
    let manager = load_wallet_manager(&config)?;
    let total_wallets = manager.count();

    let password = resolve_wallet_password(&manager, total_wallets).await?;
    let provider = Provider::new(Http::new(
        reqwest::Url::parse(&config.rpc_url).context("Invalid RPC URL")?,
    ));

    let file = File::create(&output_path).context("Failed to create output file")?;
    let mut writer = BufWriter::new(file);

    let mut written = 0usize;
    for idx in 0..total_wallets {
        let wallet = match manager.get_wallet(idx, password.as_deref()).await {
            Ok(wallet) => wallet,
            Err(e) => {
                eprintln!("[arc-balance-dump] skipping wallet {}: {}", idx, e);
                continue;
            }
        };

        let address = if !wallet.address().trim().is_empty() {
            match Address::from_str(wallet.address().trim()) {
                Ok(address) => address,
                Err(e) => {
                    eprintln!("[arc-balance-dump] skipping wallet {}: {}", idx, e);
                    continue;
                }
            }
        } else if !wallet.private_key().trim().is_empty() {
            match wallet.private_key().parse::<LocalWallet>() {
                Ok(signer) => signer.address(),
                Err(e) => {
                    eprintln!("[arc-balance-dump] skipping wallet {}: {}", idx, e);
                    continue;
                }
            }
        } else {
            eprintln!(
                "[arc-balance-dump] skipping wallet {}: missing EVM address and private key",
                idx
            );
            continue;
        };

        let balance = match provider.get_balance(address, None).await {
            Ok(balance) => balance,
            Err(e) => {
                eprintln!(
                    "[arc-balance-dump] skipping wallet {} balance check: {}",
                    idx, e
                );
                continue;
            }
        };

        writeln!(writer, "{:?},{}", address, format_balance_4dp(balance)?)?;
        written += 1;
    }

    writer.flush()?;
    println!(
        "Wrote {} wallet balance rows to {}",
        written,
        output_path.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::types::U256;
    use ethers::utils::parse_units;

    #[test]
    fn test_format_balance_4dp_keeps_four_decimals() {
        let balance = parse_units(1.234567f64, "ether").unwrap().into();
        assert_eq!(format_balance_4dp(balance).unwrap(), "1.2346");
    }

    #[test]
    fn test_format_balance_4dp_formats_zero() {
        assert_eq!(format_balance_4dp(U256::zero()).unwrap(), "0.0000");
    }
}
