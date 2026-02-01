use anyhow::Result;
use clap::Parser;
use core_logic::database::DatabaseManager;
use dotenv::dotenv;
use ethers::prelude::*;
use nu_ansi_term::{Color, Style};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tempo_project::config::TempoConfig;
use tempo_project::tasks::{TaskContext, TempoTask};
use tempo_project::utils::gas_manager::GasManager;
use tracing::error;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "chains/tempo/config.toml")]
    config: String,

    #[arg(long)]
    wallet: Option<usize>,

    /// Filter tasks by name (contains)
    #[arg(short, long)]
    filter: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup minimal logging
    tracing_subscriber::fmt().with_env_filter("info").init();
    dotenv().ok();

    println!(
        "{}",
        Style::new()
            .bold()
            .paint("\n🚀 Tempo Task Tester - Sequential Execution\n")
    );

    let args = Args::parse();

    // 1. Load Config
    let cfg = TempoConfig {
        rpc_url: env::var("TEMPO_RPC_URL")
            .unwrap_or_else(|_| "https://rpc.moderato.tempo.xyz".to_string()),
        chain_id: 42431,
        worker_count: 1,
    };

    // 2. Load Wallets
    let mut password = env::var("WALLET_PASSWORD").ok();
    let manager = core_logic::utils::WalletManager::new()?;
    let total_wallets = manager.count();

    if total_wallets == 0 {
        error!("No wallets found.");
        return Ok(());
    }

    // Select Wallet (Default to 0 or use arg)
    let wallet_idx = args.wallet.unwrap_or(0);
    if wallet_idx >= total_wallets {
        error!(
            "Wallet index {} out of range (total: {})",
            wallet_idx, total_wallets
        );
        return Ok(());
    }

    // Decrypt Wallet
    let decrypted = match manager.get_wallet(wallet_idx, password.as_deref()) {
        Ok(w) => w,
        Err(_) => {
            // Try prompt if env var failed
            use dialoguer::{theme::ColorfulTheme, Password};
            println!("⚠️  Decryption failed. Please enter password:");
            let input = Password::with_theme(&ColorfulTheme::default())
                .with_prompt("Wallet Password")
                .interact()?;
            password = Some(input);
            manager.get_wallet(wallet_idx, password.as_deref())?
        }
    };

    let wallet = decrypted
        .evm_private_key
        .parse::<LocalWallet>()?
        .with_chain_id(cfg.chain_id);

    println!(
        "👤 Using Wallet: {}",
        Style::new()
            .fg(Color::Cyan)
            .paint(format!("{:?}", wallet.address()))
    );

    // 3. Setup Provider
    let provider = Provider::<Http>::try_from(&cfg.rpc_url)?;
    let provider_arc = Arc::new(provider);
    let gas_manager = Arc::new(GasManager);
    let db_manager = Arc::new(DatabaseManager::new("tempo.db").await.unwrap());

    // 4. Register Tasks
    let all_tasks: Vec<Box<dyn TempoTask>> = vec![
        Box::new(tempo_project::tasks::t01_deploy_contract::DeployContractTask),
        Box::new(tempo_project::tasks::t02_claim_faucet::ClaimFaucetTask),
        Box::new(tempo_project::tasks::t03_send_token::SendTokenTask),
        Box::new(tempo_project::tasks::t04_create_stable::CreateStableTask),
        Box::new(tempo_project::tasks::t05_swap_stable::SwapStableTask),
        // Box::new(tempo_project::tasks::t06_add_liquidity::AddLiquidityTask),
        // Box::new(tempo_project::tasks::t07_mint_stable::MintStableTask),
        // Box::new(tempo_project::tasks::t08_burn_stable::BurnStableTask),
        // Box::new(tempo_project::tasks::t09_transfer_token::TransferTokenTask),
        // Box::new(tempo_project::tasks::t10_transfer_memo::TransferMemoTask),
        // Box::new(tempo_project::tasks::t11_limit_order::LimitOrderTask),
        // Box::new(tempo_project::tasks::t12_remove_liquidity::RemoveLiquidityTask),
        // Box::new(tempo_project::tasks::t13_grant_role::GrantRoleTask),
        // Box::new(tempo_project::tasks::t14_nft_create_mint::NftCreateMintTask),
        // Box::new(tempo_project::tasks::t15_mint_domain::MintDomainTask),
        // Box::new(tempo_project::tasks::t16_retrieve_nft::RetrieveNFTTask),
        // Box::new(tempo_project::tasks::t17_batch_eip7702::BatchEIP7702Task),
        // Box::new(tempo_project::tasks::t18_tip403_policies::TIP403PoliciesTask),
        // Box::new(tempo_project::tasks::t19_wallet_analytics::WalletAnalyticsTask),
        // Box::new(tempo_project::tasks::t20_wallet_activity::WalletActivityTask),
        // Box::new(tempo_project::tasks::t21_create_meme::CreateMemeTask),
        // Box::new(tempo_project::tasks::t22_mint_meme::MintMemeTask),
        // Box::new(tempo_project::tasks::t23_transfer_meme::TransferMemeTask),
        // Box::new(tempo_project::tasks::t24_batch_swap::BatchSwapTask),
        // Box::new(tempo_project::tasks::t25_batch_transfer::BatchTransferTask),
        // Box::new(tempo_project::tasks::t26_batch_stable::BatchStableTask),
        // Box::new(tempo_project::tasks::t27_batch_meme::BatchMemeTask),
        // Box::new(tempo_project::tasks::t28_disperse_system::DisperseSystemTask),
        // Box::new(tempo_project::tasks::t29_disperse_stable::DisperseStableTask),
        // Box::new(tempo_project::tasks::t30_disperse_meme::DisperseMemeTask),
        // Box::new(tempo_project::tasks::t31_concurrent_system::ConcurrentSystemTask),
        // Box::new(tempo_project::tasks::t32_concurrent_stable::ConcurrentStableTask),
        // Box::new(tempo_project::tasks::t33_concurrent_meme::ConcurrentMemeTask),
    ];

    let tasks_to_run: Vec<&Box<dyn TempoTask>> = if let Some(filter) = args.filter {
        all_tasks
            .iter()
            .filter(|t| t.name().contains(&filter))
            .collect()
    } else {
        all_tasks.iter().collect()
    };

    println!("📋 Found {} tasks to run.\n", tasks_to_run.len());

    // 5. Execution Loop
    let mut results = Vec::new();

    for task in tasks_to_run {
        let name = task.name();
        print!("⏳ Running {} ... ", name);
        use std::io::{self, Write};
        io::stdout().flush().unwrap();

        let ctx = TaskContext {
            provider: provider_arc.clone(),
            wallet: wallet.clone(),
            config: cfg.clone(),
            gas_manager: gas_manager.clone(),
            db: Some(db_manager.clone()),
        };

        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(120); // 2 min timeout per task

        // Execute with timeout
        let res = tokio::time::timeout(timeout, task.run(ctx)).await;
        let duration = start.elapsed();

        match res {
            Ok(Ok(task_res)) => {
                if task_res.success {
                    println!("{}", Style::new().fg(Color::Green).paint("✅ PASS"));
                } else {
                    println!("{}", Style::new().fg(Color::Red).paint("❌ FAIL"));
                    println!("   Error: {}", task_res.message);
                }
                results.push((name, true, Some(task_res), duration));
            }
            Ok(Err(e)) => {
                println!("{}", Style::new().fg(Color::Red).paint("💥 ERROR"));
                println!("   Exception: {:?}", e);
                results.push((name, false, None, duration));
            }
            Err(_) => {
                println!("{}", Style::new().fg(Color::Yellow).paint("⏰ TIMEOUT"));
                results.push((name, false, None, duration));
            }
        }

        // Brief pause between tasks
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // 6. Summary Report
    println!(
        "\n📊 {} 📊\n",
        Style::new().bold().underline().paint("Execution Summary")
    );

    println!(
        "{:<35} | {:<8} | {:<10} | {:<40}",
        "Task Name", "Status", "Duration", "Message/Tx"
    );
    println!("{}", "-".repeat(100));

    let mut success_count = 0;
    for (name, ran_ok, res_opt, dur) in &results {
        let (status_str, color) = if *ran_ok {
            if let Some(r) = res_opt {
                if r.success {
                    success_count += 1;
                    ("PASS", Color::Green)
                } else {
                    ("FAIL", Color::Red)
                }
            } else {
                ("FAIL", Color::Red)
            }
        } else {
            ("ERROR", Color::Red)
        };

        let msg = if let Some(r) = res_opt {
            if let Some(tx) = &r.tx_hash {
                tx.chars().take(38).collect::<String>() + "..."
            } else {
                r.message.chars().take(38).collect::<String>()
            }
        } else {
            "-".to_string()
        };

        println!(
            "{:<35} | {} | {:<10.2?} | {}",
            name,
            Style::new().fg(color).paint(status_str),
            dur,
            msg
        );
    }

    println!("\nScore: {}/{}", success_count, results.len());

    Ok(())
}
