use anyhow::Result;
use core_logic::WalletManager;
use std::env;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🔍 Wallet Diagnostic Tool");
    println!("========================\n");

    let wallet_manager = WalletManager::new()?;
    let total_wallets = wallet_manager.count();

    println!("Found {} wallets\n", total_wallets);

    // Get password from environment variable
    let password = env::var("WALLET_PASSWORD").expect("Set WALLET_PASSWORD environment variable");
    println!("Using password from environment variable\n");

    println!("\n🔐 Testing wallet decryption...\n");

    let mut success_count = 0;
    let mut failed_wallets = Vec::new();
    let start = Instant::now();

    for i in 0..total_wallets {
        match wallet_manager.get_wallet(i, Some(&password)).await {
            Ok(wallet) => {
                success_count += 1;
                if i < 5 || i % 50 == 0 {
                    println!("✅ Wallet {}: {} - OK", i, &wallet.evm_address[..20]);
                }
            }
            Err(e) => {
                failed_wallets.push((i, e.to_string()));
                println!("❌ Wallet {}: FAILED - {}", i, e);
            }
        }
    }

    let duration = start.elapsed();

    println!("\n========================");
    println!("📊 Results:");
    println!("   Total wallets: {}", total_wallets);
    println!("   Successful: {}", success_count);
    println!("   Failed: {}", failed_wallets.len());
    println!("   Time: {:.2}s", duration.as_secs_f32());

    if !failed_wallets.is_empty() {
        println!("\n❌ Failed wallets:");
        for (idx, err) in &failed_wallets {
            println!("   Wallet {}: {}", idx, err);
        }
    }

    Ok(())
}
