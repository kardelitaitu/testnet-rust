use anyhow::Result;
use clap::Parser;
use core_logic::database::DatabaseManager;
use core_logic::traits::TaskResult;
use dialoguer::{theme::ColorfulTheme, Password, Select};
use dotenv::dotenv;
use ethers::prelude::*;
use std::env;
use std::sync::Arc;
use tempo_project::config::TempoConfig;
use tempo_project::tasks::{TaskContext, TempoTask};
use tempo_project::utils::gas_manager::GasManager;
use tracing::{error, info};

struct DummyTask;
#[async_trait::async_trait]
impl TempoTask for DummyTask {
    fn name(&self) -> &str {
        "00_placeholder"
    }
    async fn run(&self, _ctx: TaskContext) -> anyhow::Result<TaskResult> {
        Ok(TaskResult {
            success: true,
            message: "This is a placeholder task.".to_string(),
            tx_hash: None,
        })
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "chains/tempo/config.toml")]
    config: String,

    #[arg(short, long)]
    task: Option<usize>,

    #[arg(long)]
    wallet: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenv().ok();

    println!("--- Tempo Debugger (Ethers) ---");

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

    if total_wallets > 0 {
        print!("Found {} wallet files in wallet-json. ", total_wallets);
        if let Err(_) = manager.get_wallet(0, password.as_deref()) {
            println!("\n⚠️  Decryption failed with env KEY (or KEY unset/wrong).");
            let input = Password::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter wallet password")
                .interact()?;
            password = Some(input);
            if let Err(e) = manager.get_wallet(0, password.as_deref()) {
                error!("Decryption failed: {}", e);
                return Ok(());
            }
        } else {
            println!("Decryption test passed.");
        }
    } else {
        println!("No wallet files found.");
        return Ok(());
    }

    // Interactive Wallet Selection
    let selected_index = if let Some(idx) = args.wallet {
        idx
    } else {
        println!("\n? Select Wallet to debug ›");
        let wallet_names = manager.list_wallets();
        let mut wallet_choices = vec!["Pick Random Wallet".to_string()];

        for (i, name) in wallet_names.iter().enumerate() {
            wallet_choices.push(format!("Wallet {}: {}", i, name));
        }

        let wallet_selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select Wallet")
            .default(0)
            .items(&wallet_choices)
            .interact()?; // Fixed semicolon

        if wallet_selection == 0 {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            rng.gen_range(0..total_wallets)
        } else {
            wallet_selection - 1
        }
    };

    let decrypted = manager.get_wallet(selected_index, password.as_deref())?;
    let wallet = decrypted
        .evm_private_key
        .parse::<LocalWallet>()?
        .with_chain_id(cfg.chain_id);

    // Create Ethers Provider
    let provider = Provider::<Http>::try_from(&cfg.rpc_url)?;
    let provider_arc = Arc::new(provider);

    let gas_manager = Arc::new(GasManager);

    info!("Debugging with wallet address: {:?}", wallet.address());

    // Define Tasks
    let tasks: Vec<Box<dyn TempoTask>> =
        vec![
        Box::new(DummyTask), // Task 0: Placeholder
        Box::new(tempo_project::tasks::t01_deploy_contract::DeployContractTask), // Task 1
        Box::new(tempo_project::tasks::t02_claim_faucet::ClaimFaucetTask),      // Task 2
        Box::new(tempo_project::tasks::t03_send_token::SendTokenTask),        // Task 3
        Box::new(tempo_project::tasks::t04_create_stable::CreateStableTask),    // Task 4
        Box::new(tempo_project::tasks::t05_swap_stable::SwapStableTask),      // Task 5
        Box::new(tempo_project::tasks::t06_add_liquidity::AddLiquidityTask),   // Task 6
        Box::new(tempo_project::tasks::t07_mint_stable::MintStableTask),      // Task 7
        Box::new(tempo_project::tasks::t08_burn_stable::BurnStableTask),      // Task 8
        Box::new(tempo_project::tasks::t09_transfer_token::TransferTokenTask), // Task 9
        Box::new(tempo_project::tasks::t10_transfer_memo::TransferMemoTask),   // Task 10
        Box::new(tempo_project::tasks::t11_limit_order::LimitOrderTask),     // Task 11
        Box::new(tempo_project::tasks::t12_remove_liquidity::RemoveLiquidityTask), // Task 12
        Box::new(tempo_project::tasks::t13_grant_role::GrantRoleTask),        // Task 13
        Box::new(tempo_project::tasks::t14_nft_create_mint::NftCreateMintTask), // Task 14
        Box::new(tempo_project::tasks::t15_mint_domain::MintDomainTask),      // Task 15
        Box::new(tempo_project::tasks::t16_retrieve_nft::RetrieveNftTask),      // Task 16
        Box::new(tempo_project::tasks::t17_batch_eip7702::BatchEip7702Task),    // Task 17
        Box::new(tempo_project::tasks::t18_tip403_policies::Tip403PoliciesTask), // Task 18
        Box::new(tempo_project::tasks::t19_wallet_analytics::WalletAnalyticsTask), // Task 19
        Box::new(tempo_project::tasks::t20_wallet_activity::WalletActivityTask), // Task 20
        Box::new(tempo_project::tasks::t21_create_meme::CreateMemeTask),        // Task 21
        Box::new(tempo_project::tasks::t22_mint_meme::MintMemeTask),          // Task 22
        Box::new(tempo_project::tasks::t23_transfer_meme::TransferMemeTask),    // Task 23
        Box::new(tempo_project::tasks::t24_batch_swap::BatchSwapTask),        // Task 24
        Box::new(tempo_project::tasks::t25_batch_system_token::BatchSystemTokenTask), // Task 25
        Box::new(tempo_project::tasks::t26_batch_stable_token::BatchStableTokenTask), // Task 26
        Box::new(tempo_project::tasks::t27_batch_meme_token::BatchMemeTokenTask),   // Task 27
        Box::new(tempo_project::tasks::t28_multi_send_disperse::MultiSendDisperseTask), // Task 28
        Box::new(tempo_project::tasks::t29_multi_send_disperse_stable::MultiSendDisperseStableTask), // Task 29
        Box::new(tempo_project::tasks::t30_multi_send_disperse_meme::MultiSendDisperseMemeTask), // Task 30
        Box::new(tempo_project::tasks::t31_multi_send_concurrent::MultiSendConcurrentTask), // Task 31
        Box::new(tempo_project::tasks::t32_multi_send_concurrent_stable::MultiSendConcurrentStableTask), // Task 32
        Box::new(tempo_project::tasks::t33_multi_send_concurrent_meme::MultiSendConcurrentMemeTask), // Task 33
        Box::new(tempo_project::tasks::t34_batch_send_transaction::BatchSendTransactionTask), // Task 34
        Box::new(tempo_project::tasks::t35_batch_send_transaction_stable::BatchSendTransactionStableTask), // Task 35
        Box::new(tempo_project::tasks::t36_batch_send_transaction_meme::BatchSendTransactionMemeTask), // Task 36
        Box::new(tempo_project::tasks::t37_transfer_later::TransferLaterTask), // Task 37
        Box::new(tempo_project::tasks::t38_transfer_later_stable::TransferLaterStableTask), // Task 38
        Box::new(tempo_project::tasks::t39_transfer_later_meme::TransferLaterMemeTask), // Task 39
        Box::new(tempo_project::tasks::t40_distribute_shares::DistributeSharesTask), // Task 40
        Box::new(tempo_project::tasks::t41_distribute_shares_stable::DistributeSharesStableTask), // Task 41
        Box::new(tempo_project::tasks::t42_distribute_shares_meme::DistributeSharesMemeTask), // Task 42
        Box::new(tempo_project::tasks::t43_batch_mint_stable::BatchMintStableTask), // Task 43
        Box::new(tempo_project::tasks::t44_batch_mint_meme::BatchMintMemeTask), // Task 44
        Box::new(tempo_project::tasks::t45_deploy_viral_faucet::DeployViralFaucetTask), // Task 45
        Box::new(tempo_project::tasks::t46_claim_viral_faucet::ClaimViralFaucetTask), // Task 46
        Box::new(tempo_project::tasks::t47_deploy_viral_nft::DeployViralNftTask), // Task 47
        Box::new(tempo_project::tasks::t48_mint_viral_nft::MintViralNftTask), // Task 48
        Box::new(tempo_project::tasks::t49_time_bomb::TimeBombTask), // Task 49
        Box::new(tempo_project::tasks::t50_deploy_storm::DeployStormTask), // Task 50
    ];

    if tasks.is_empty() {
        println!("No tasks implemented yet.");
        return Ok(());
    }

    let items: Vec<&str> = tasks.iter().map(|t| t.name()).collect();

    let selection = if let Some(t_id) = args.task {
        if t_id < items.len() {
            t_id
        } else {
            error!("Invalid task ID");
            return Ok(());
        }
    } else {
        Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select task to debug")
            .default(0)
            .items(&items)
            .interact()? // Fixed semicolon
    };

    let selected_task = &tasks[selection];
    println!("Debugging Task: {}", selected_task.name());

    // Initialize Database
    let db_manager = Arc::new(DatabaseManager::new("tempo.db").await.unwrap());

    let ctx = TaskContext {
        provider: provider_arc.clone(),
        wallet: wallet.clone(),
        config: cfg.clone(),
        gas_manager: gas_manager.clone(),
        db: Some(db_manager),
    };

    println!("Wallet Address: {:?}", wallet.address());
    println!("Running...");

    let start_time = std::time::Instant::now();
    let result = selected_task.run(ctx).await;
    let duration = start_time.elapsed();

    match result {
        Ok(res) => {
            if res.success {
                println!("\n✅ Success: {}", res.message);
            } else {
                println!("\n❌ Failed: {}", res.message);
            }
            if let Some(tx) = res.tx_hash {
                println!("🔗 Transaction: {}", tx);
            }
        }
        Err(e) => {
            println!("\n💥 Task Error: {:?}", e);
        }
    }
    println!("⏱️ Duration: {:.2?}", duration);

    Ok(())
}
