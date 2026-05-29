use anyhow::Result;
use colored::Colorize;
use ethers::providers::{Http, Middleware, Provider};
use sepolia_overlayer::config::SepoliaConfig;
use std::time::{Duration, Instant};
use tokio::time::timeout;

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = "chains/sepolia-overlayer/config.toml";
    let config = SepoliaConfig::load(config_path)?;

    let mut urls = config.rpc_urls.clone();
    if !urls.contains(&config.rpc_url) {
        urls.push(config.rpc_url.clone());
    }

    println!("{}", "=== Sepolia RPC Health Check (No Proxy) ===".bold().cyan());
    println!("Found {} RPC URLs to test.\n", urls.len());

    for url in urls {
        match test_rpc(&url).await {
            Ok((latency, block)) => {
                println!(
                    "{} {:<50} | Latency: {:>4}ms | Block: {}",
                    "✔".green(),
                    url.cyan(),
                    latency.as_millis().to_string().yellow(),
                    block.to_string().white()
                );
            },
            Err(e) => {
                println!("{} {:<50} | Error: {}", "✘".red(), url.dimmed(), e.to_string().red());
            },
        }
    }

    println!("\n{}", "=== Throughput Test (20 quick requests) ===".bold().cyan());
    for url in config.rpc_urls {
        test_throughput(&url).await;
    }

    Ok(())
}

async fn test_rpc(url: &str) -> Result<(Duration, u64)> {
    let provider = Provider::<Http>::try_from(url)?;
    let start = Instant::now();
    let block = timeout(Duration::from_secs(5), provider.get_block_number()).await??;
    let latency = start.elapsed();
    Ok((latency, block.as_u64()))
}

async fn test_throughput(url: &str) {
    let provider = Provider::<Http>::try_from(url).unwrap();
    let mut success = 0;
    let start = Instant::now();

    for _ in 0..10 {
        if let Ok(Ok(_)) = timeout(Duration::from_secs(2), provider.get_block_number()).await {
            success += 1;
        }
    }

    let elapsed = start.elapsed();
    let status = if success == 10 {
        "EXCELLENT".green()
    } else if success > 0 {
        format!("{}/10 SUCCESS", success).yellow()
    } else {
        "FAILED".red()
    };

    println!(
        "{:<50} | Status: {} | Total Time: {}ms",
        url.cyan(),
        status,
        elapsed.as_millis().to_string().yellow()
    );
}
