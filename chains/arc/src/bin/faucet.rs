//! Arc Testnet Faucet — automates https://faucet.circle.com via Obscura.
//!
//! Uses [Obscura](https://github.com/h4ckf0r0day/obscura), a lightweight
//! headless browser engine written in Rust, to automate the Circle faucet.
//!
//! Obscura solves the proxy authentication issue present in `headless_chrome`
//! by supporting `http://user:pass@host:port` natively via `--proxy`.
//!
//! Usage:
//!   cargo run -p arc-project --bin arc-faucet -- --address 0x...
//!   cargo run -p arc-project --bin arc-faucet -- --address 0x... --token eurc --proxy http://user:pass@host:port
//!   cargo run -p arc-project --bin arc-faucet -- --address 0x... --token cirbtc
//!
//! Requirements:
//!   - `obscura` binary in PATH (download from https://github.com/h4ckf0r0day/obscura/releases)

use anyhow::Result;
use arc_project::utils::faucet;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Arc Testnet Faucet — automates faucet.circle.com via Obscura"
)]
struct Args {
    /// Wallet address to receive test tokens
    #[arg(short, long)]
    address: String,

    /// Token to request: usdc (default), eurc, or cirbtc
    #[arg(short, long, default_value = "usdc")]
    token: String,

    /// Proxy server (e.g. http://user:pass@host:port or socks5://host:port)
    #[arg(short, long)]
    proxy: Option<String>,

    /// Max seconds to wait for the faucet response (default 30)
    #[arg(long, default_value_t = 30)]
    timeout: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!("╔══════════════════════════════════════╗");
    println!("║   Arc Testnet Faucet (Obscura)       ║");
    println!("╚══════════════════════════════════════╝");
    println!();
    println!("  Address:  {}\n  Token:    {}", args.address, args.token);
    if let Some(ref proxy) = args.proxy {
        let masked = mask_proxy(proxy);
        println!("  Proxy:    {}", masked);
    } else {
        println!("  Proxy:    none (direct)");
    }
    println!("  Headless: Obscura (always headless)");
    println!();

    // Validate address format
    if !args.address.starts_with("0x") || args.address.len() != 42 {
        eprintln!("❌ Invalid address: must be 42-char hex starting with 0x");
        std::process::exit(1);
    }

    println!("🚀 Launching Obscura and requesting tokens...");
    println!();

    let result = faucet::request_tokens(
        &args.address,
        &args.token,
        args.proxy.as_deref(),
        true, // visible - ignored by Obscura
        args.timeout,
        None, // obscura_path - ignored by Obscura
    )?;

    println!();
    println!("╔══════════════════════════════════════╗");
    println!("║            Faucet Result             ║");
    println!("╚══════════════════════════════════════╝");
    println!();
    println!("{}", result.message);
    println!();
    println!("🔍 Explorer: https://testnet.arcscan.app/address/{}", args.address);

    if !result.success {
        std::process::exit(1);
    }

    Ok(())
}

/// Mask sensitive parts of proxy URL for display.
fn mask_proxy(proxy: &str) -> String {
    if let Some(at_pos) = proxy.rfind('@') {
        let prefix = &proxy[..at_pos];
        let suffix = &proxy[at_pos..];
        if let Some(colon_pos) = prefix.rfind(':') {
            format!("{}:***@{}\n", &prefix[..colon_pos], &suffix[1..])
        } else {
            format!("***{}\n", suffix)
        }
    } else {
        proxy.to_string()
    }
}
