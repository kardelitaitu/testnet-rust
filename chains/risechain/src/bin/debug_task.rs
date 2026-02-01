use anyhow::Result;
use clap::Parser;
use core_logic::setup_logger;
use dialoguer::{theme::ColorfulTheme, Password, Select};
use dotenv::dotenv;
use ethers::prelude::*;
use reqwest;
use rise_project::config::RiseConfig;
use rise_project::task::{
    t01_check_balance::CheckBalanceTask, t02_simple_eth_transfer::SimpleEthTransferTask,
    t03_deploy_contract::DeployContractTask, t04_interact_contract::InteractContractTask,
    t05_self_transfer::SelfTransferTask, t06_send_meme::SendMemeTokenTask,
    t07_create_meme::CreateMemeTask, t09_weth_wrap::WethWrapTask, t10_weth_unwrap::WethUnwrapTask,
    t11_batch_transfer::BatchTransferTask, t12_nft_mint::NftMintTask,
    t13_nft_transfer::NftTransferTask, t14_approve_token::ApproveTokenTask,
    t16_multicall::MulticallTask, t17_read_oracle::ReadOracleTask,
    t18_contract_call_raw::ContractCallRawTask, t19_high_gas_limit::HighGasLimitTask,
    t20_gas_price_test::GasPriceTestTask, t21_erc1155_mint::Erc1155MintTask,
    t22_erc1155_transfer::Erc1155TransferTask, t23_timed_interaction::TimedInteractionTask,
    t24_create2_deploy::Create2DeployTask, t25_message_sign::MessageSignTask,
    t26_verify_signature::VerifySignatureTask, t27_permit_token::PermitTokenTask,
    t28_delegatecall::DelegatecallTask, t29_cross_contract_call::CrossContractCallTask,
    t30_revert_test::RevertTestTask, t31_event_emission::EventEmissionTask,
    t32_eth_with_data::EthWithDataTask, t33_batch_approve::BatchApproveTask,
    t34_role_based_access::RoleBasedAccessTask, t35_pausable_contract::PausableContractTask,
    t36_create2_factory::Create2FactoryTask, t37_uups_proxy::UUPSProxyTask,
    t38_transparent_proxy::TransparentProxyTask, t39_uniswap_v2_swap::UniswapV2SwapTask,
    t40_erc4626_vault::ERC4626VaultTask, t41_flash_loan::FlashLoanTestTask,
    t42_erc721_mint::ERC721MintTask, t43_erc1155_batch::ERC1155BatchTask,
    t44_storage_pattern::StoragePatternTask, t45_custom_error::CustomErrorTestTask,
    t46_revert_reason::RevertWithReasonTask, t47_assert_fail::AssertFailTask,
    t48_anonymous_event::AnonymousEventTask, t49_indexed_topics::IndexedTopicsTask,
    t50_large_event::LargeEventDataTask, t51_memory_expansion::MemoryExpansionTask,
    t52_calldata_size::CalldataSizeTask, t53_gas_stipend::GasStipendTask,
    t54_gas_price_zero::GasPriceZeroTask, t55_block_hash::BlockHashUsageTask,
    t57_eip7702_explore::Eip7702ExploreTask, t58_verify_create2::VerifyCreate2Task,
    t59_deploy_factory::DeployFactoryTask, t60_rise_to_weth::RiseToWethTask, RiseTask, Task,
    TaskContext,
};
use std::env;
use tracing::{error, info};
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "chains/risechain/config.toml")]
    config: String,

    #[arg(short, long)]
    task: Option<usize>,

    #[arg(long)]
    wallet: Option<usize>,

    #[arg(long)]
    all: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logger();
    dotenv().ok();

    // Print Header first
    println!("--- RISE Debugger ---");

    let args = Args::parse();
    // info!("Loading config from: {}", args.config);

    // 1. Load Config
    let cfg = match RiseConfig::load(&args.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load config: {}", e);
            return Ok(());
        }
    };

    // info!("Configuration loaded for chain ID: {}", cfg.chain_id);

    // 2. Load Wallets
    let mut password = env::var("WALLET_PASSWORD").ok();
    let manager = core_logic::WalletManager::new()?;
    // Init DB Manager
    let db_manager =
        std::sync::Arc::new(core_logic::database::DatabaseManager::new("rise.db").await?);

    // Load Proxies
    let proxies = if std::path::Path::new("proxies.txt").exists() {
        let content = std::fs::read_to_string("proxies.txt")?;
        content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect::<Vec<String>>()
    } else {
        vec![]
    };
    if !proxies.is_empty() {
        println!("Loaded {} proxies.", proxies.len());
    }

    let total_wallets = manager.count();

    if total_wallets > 0 {
        print!("Found {} wallet files in wallet-json. ", total_wallets);
        // Verify decryption with first wallet to ensure password is correct
        // This serves the purpose of the previous "keys.is_empty()" check
        if let Err(_) = manager.get_wallet(0, password.as_deref()).await {
            println!("\n⚠️  Decryption failed with env KEY (or KEY unset/wrong).");
            let input = Password::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter wallet password")
                .interact()?;
            password = Some(input);

            // Validate again
            if let Err(e) = manager.get_wallet(0, password.as_deref()).await {
                error!("Decryption failed even with provided password: {}", e);
                return Ok(());
            }
        } else {
            println!("Decryption test passed.");
        }
    } else {
        println!("No wallet files found.");
        return Ok(());
    }

    if args.all {
        // ... (All logic remains same, implicit skip)
        println!("Checking balance for ALL {} wallets...", total_wallets);
        // ...
        println!("{:<5} | {:<42} | {:<20}", "ID", "Address", "Balance/Result");
        println!("{}", "-".repeat(75));

        // Use Arc for shared resources across threads
        let cfg = std::sync::Arc::new(cfg);
        let password = std::sync::Arc::new(password); // Option<String> in Arc
        let manager = std::sync::Arc::new(manager);
        let db_manager = db_manager.clone();
        let proxies = std::sync::Arc::new(proxies);

        use futures::stream::{self, StreamExt};

        let bodies = stream::iter(0..total_wallets)
            .map(|i| {
                let manager = manager.clone();
                let password = password.clone();
                let cfg = cfg.clone();
                let db_manager = db_manager.clone();
                let proxies = proxies.clone();

                async move {
                    // Determine Proxy
                    let proxy_url = if !proxies.is_empty() {
                        let p = &proxies[i % proxies.len()];
                        let parts: Vec<&str> = p.split(':').collect();
                        if parts.len() == 4 {
                            Some(format!(
                                "http://{}:{}@{}:{}",
                                parts[2], parts[3], parts[0], parts[1]
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    // Create Provider
                    let client_builder = reqwest::Client::builder();
                    let client = if let Some(u) = &proxy_url {
                        match reqwest::Proxy::all(u) {
                            Ok(p) => client_builder
                                .proxy(p)
                                .build()
                                .unwrap_or(reqwest::Client::new()),
                            Err(_) => reqwest::Client::new(),
                        }
                    } else {
                        client_builder.build().unwrap_or(reqwest::Client::new())
                    };

                    let provider_url = Url::parse(&cfg.rpc_url).expect("Invalid RPC URL");
                    let provider = Provider::new(Http::new_with_client(provider_url, client));
                    let provider_arc = std::sync::Arc::new(provider);
                    let gas_manager = std::sync::Arc::new(
                        rise_project::utils::gas::GasManager::new(provider_arc.clone()),
                    );

                    let wallet_res = manager.get_wallet(i, password.as_deref()).await;
                    let (pub_addr, result_str) = match wallet_res {
                        Ok(decrypted) => {
                            let key = decrypted.evm_private_key.clone();
                            match key.parse::<LocalWallet>() {
                                Ok(w) => {
                                    let wallet = w.with_chain_id(cfg.chain_id);
                                    let addr = format!("{:?}", wallet.address());

                                    let ctx = TaskContext {
                                        provider: (*provider_arc).clone(),
                                        wallet: wallet,
                                        config: (*cfg).clone(),
                                        proxy: proxy_url,
                                        db: Some(db_manager),
                                        gas_manager: gas_manager,
                                    };

                                    let task = CheckBalanceTask;
                                    match task.run(ctx).await {
                                        Ok(res) => {
                                            if res.success {
                                                (
                                                    addr,
                                                    res.message.replace(
                                                        "Analysis complete. Balance: ",
                                                        "",
                                                    ),
                                                )
                                            } else {
                                                (addr, format!("FAILED: {}", res.message))
                                            }
                                        }
                                        Err(e) => (addr, format!("ERROR: {}", e)),
                                    }
                                }
                                Err(e) => {
                                    ("Invalid Key".to_string(), format!("Parse Error: {}", e))
                                }
                            }
                        }
                        Err(e) => (
                            "User failed decrypt".to_string(),
                            format!("Decrypt Error: {}", e),
                        ),
                    };
                    (i, pub_addr, result_str)
                }
            })
            .buffered(20); // Parallelism limit, but keeps order

        bodies
            .for_each(|(i, addr, res)| async move {
                println!("{:<5} | {:<42} | {}", i, addr, res);
            })
            .await;
        println!("{}", "-".repeat(75));
    } else {
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
                .interact()?;

            if wallet_selection == 0 {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                rng.gen_range(0..total_wallets)
            } else {
                wallet_selection - 1
            }
        };

        let decrypted = manager
            .get_wallet(selected_index, password.as_deref())
            .await?;
        let wallet = decrypted
            .evm_private_key
            .clone()
            .parse::<LocalWallet>()?
            .with_chain_id(cfg.chain_id);

        info!("Debugging with wallet: {:?}", wallet.address());

        // Determine Proxy
        let proxy_url = if !proxies.is_empty() {
            let p = &proxies[selected_index % proxies.len()];
            let parts: Vec<&str> = p.split(':').collect();
            if parts.len() == 4 {
                Some(format!(
                    "http://{}:{}@{}:{}",
                    parts[2], parts[3], parts[0], parts[1]
                ))
            } else {
                None
            }
        } else {
            None
        };

        // Create Provider
        let client_builder = reqwest::Client::builder();
        let client = if let Some(u) = &proxy_url {
            println!("Using proxy: {}", u.split('@').last().unwrap_or("..."));
            match reqwest::Proxy::all(u) {
                Ok(p) => client_builder
                    .proxy(p)
                    .build()
                    .unwrap_or(reqwest::Client::new()),
                Err(_) => reqwest::Client::new(),
            }
        } else {
            client_builder.build().unwrap_or(reqwest::Client::new())
        };

        let provider_url = Url::parse(&cfg.rpc_url).expect("Invalid RPC URL");
        let provider = Provider::new(Http::new_with_client(provider_url, client));

        let tasks: Vec<Box<RiseTask>> = vec![
            Box::new(CheckBalanceTask),
            Box::new(SimpleEthTransferTask),
            Box::new(DeployContractTask),
            Box::new(InteractContractTask),
            Box::new(SelfTransferTask),
            Box::new(SendMemeTokenTask),
            Box::new(CreateMemeTask),
            Box::new(WethWrapTask),
            Box::new(WethUnwrapTask),
            Box::new(BatchTransferTask),
            Box::new(NftMintTask),
            Box::new(NftTransferTask),
            Box::new(ApproveTokenTask),
            Box::new(MulticallTask),
            Box::new(ReadOracleTask),
            Box::new(ContractCallRawTask),
            Box::new(HighGasLimitTask),
            Box::new(GasPriceTestTask),
            Box::new(Erc1155MintTask),
            Box::new(Erc1155TransferTask),
            Box::new(TimedInteractionTask),
            Box::new(Create2DeployTask),
            Box::new(MessageSignTask),
            Box::new(VerifySignatureTask),
            Box::new(PermitTokenTask),
            Box::new(DelegatecallTask),
            Box::new(CrossContractCallTask),
            Box::new(RevertTestTask),
            Box::new(EventEmissionTask),
            Box::new(EthWithDataTask),
            Box::new(BatchApproveTask),
            Box::new(RoleBasedAccessTask),
            Box::new(PausableContractTask),
            Box::new(Create2FactoryTask),
            Box::new(UUPSProxyTask),
            Box::new(TransparentProxyTask),
            Box::new(UniswapV2SwapTask),
            Box::new(ERC4626VaultTask),
            Box::new(FlashLoanTestTask),
            Box::new(ERC721MintTask),
            Box::new(ERC1155BatchTask),
            Box::new(StoragePatternTask),
            Box::new(CustomErrorTestTask),
            Box::new(RevertWithReasonTask),
            Box::new(AssertFailTask),
            Box::new(AnonymousEventTask),
            Box::new(IndexedTopicsTask),
            Box::new(LargeEventDataTask),
            Box::new(MemoryExpansionTask),
            Box::new(CalldataSizeTask),
            Box::new(GasStipendTask),
            Box::new(GasPriceZeroTask),
            Box::new(BlockHashUsageTask),
            Box::new(Eip7702ExploreTask),
            Box::new(VerifyCreate2Task),
            Box::new(DeployFactoryTask),
            Box::new(RiseToWethTask),
        ];
        let items: Vec<&str> = tasks.iter().map(|t| t.name()).collect();

        let selection = if let Some(t_id) = args.task {
            // Try to find task starting with "{t_id}_" or "{02d}_"
            let prefix1 = format!("{}_", t_id);
            let prefix2 = format!("{:02}_", t_id);

            if let Some(pos) = tasks
                .iter()
                .position(|t| t.name().starts_with(&prefix1) || t.name().starts_with(&prefix2))
            {
                pos
            } else {
                // Fallback to index if < items.len(), but warn
                if t_id < items.len() {
                    println!(
                        "⚠️  Warning: Task ID {} not found by name prefix, using index {} ({})",
                        t_id, t_id, items[t_id]
                    );
                    t_id
                } else {
                    error!(
                        "Invalid task ID or index: {}. Available: 0 to {}",
                        t_id,
                        items.len() - 1
                    );
                    return Ok(());
                }
            }
        } else {
            Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select task to debug")
                .default(0)
                .items(&items)
                .interact()?
        };

        let selected_task = &tasks[selection];
        println!("Debugging Task: {}", selected_task.name());

        // Initialize Gas Manager
        let gas_manager = std::sync::Arc::new(rise_project::utils::gas::GasManager::new(
            std::sync::Arc::new(provider.clone()),
        ));

        // 4. Execute
        let ctx = TaskContext {
            provider,
            wallet: wallet.with_chain_id(cfg.chain_id),
            config: cfg.clone(),
            proxy: proxy_url,
            db: Some(db_manager),
            gas_manager,
        };

        println!("Running...");
        match selected_task.run(ctx).await {
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
    }

    Ok(())
}
