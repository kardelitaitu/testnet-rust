use anyhow::Result;
use arc_project::config::ArcConfig;
use arc_project::task::{
    t01_check_balance::ArcCheckBalanceTask, t02_send_usdc::SendUsdcTask,
    t03_send_eurc::SendEurcTask, t04_send_cirbtc::SendCirbtcTask, ArcTask, TaskContext,
};
use clap::Parser;
use dialoguer::{theme::ColorfulTheme, Select};
use ethers::prelude::*;
use std::env;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "chains/arc/config.toml")]
    config: String,

    #[arg(short, long)]
    task: Option<usize>,

    #[arg(long)]
    wallet: Option<usize>,

    #[arg(long)]
    all: bool,

    #[arg(long)]
    no_proxy: bool,

    #[arg(long, default_value_t = 0.2)]
    min_gwei: f64,

    #[arg(long, default_value_t = 0.5)]
    max_gwei: f64,

    /// Quiet mode. Repeat to suppress more: -q (ERROR only), -qq (+no diagnostics), -qqq (silent file too)
    #[arg(short, long, action = clap::ArgAction::Count)]
    quiet: u8,
}

/// Build a tracing subscriber for the debugger.
///
/// Default: console shows INFO + task_result (good for debugging).
/// -q: suppress to ERROR only.
/// -qq: also skip pre/post-flight diagnostics.
/// -qqq: also silence the file logger.
fn setup_debug_logger(quiet: u8) -> Option<WorkerGuard> {
    use tracing_subscriber::prelude::*;

    std::fs::create_dir_all("logs").ok();

    let file_appender = tracing_appender::rolling::hourly("logs", "debug");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Default: console shows INFO (debug tool). -q dials down to ERROR.
    let console_default = match quiet {
        0 => tracing::Level::INFO,
        _ => tracing::Level::ERROR,
    };

    let file_filter = match quiet {
        0..=2 => tracing_subscriber::filter::Targets::new()
            .with_target("task_result", tracing::Level::INFO)
            .with_default(tracing::Level::WARN),
        _ => tracing_subscriber::filter::Targets::new()
            .with_target("task_result", tracing::Level::INFO)
            .with_default(tracing::Level::ERROR),
    };

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_filter(file_filter);

    let console_filter = tracing_subscriber::filter::Targets::new()
        .with_target("task_result", tracing::Level::INFO)
        .with_default(console_default);

    let console_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .with_filter(console_filter);

    tracing_subscriber::registry()
        .with(file_layer)
        .with(console_layer)
        .init();

    Some(guard)
}

/// Format proxy for display (mask credentials).
fn display_proxy(url: &str) -> String {
    if let Some(at) = url.rfind('@') {
        let host = &url[at + 1..];
        format!("***@{}", host)
    } else {
        url.to_string()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Setup logger — capture guard so file writer stays alive
    let _guard = setup_debug_logger(args.quiet);

    // Print console header regardless of level
    println!("\n  ╔═══════════════════════════════════╗");
    println!("  ║       ARC Debugger v0.1           ║");
    println!("  ╚═══════════════════════════════════╝");
    println!();

    // Load .env from same directory as config
    if let Some(parent) = Path::new(&args.config).parent() {
        let env_path = parent.join(".env");
        if env_path.exists() {
            let _ = dotenv::from_path(&env_path);
        }
    }

    // 1. Load Config
    let cfg = match ArcConfig::load(&args.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load config: {}", e);
            return Ok(());
        }
    };
    info!(
        "Loaded config | chain_id={} | rpc={} | symbol={} | explorer={}",
        cfg.chain_id, cfg.rpc_url, cfg.symbol, cfg.explorer
    );
    debug!(
        "Config details | tps={} | min_delay={:?} | max_delay={:?} | wallet_dir={:?}",
        cfg.tps, cfg.min_delay_ms, cfg.max_delay_ms, cfg.wallet_dir
    );

    // 2. Load Wallets
    let password = env::var("WALLET_PASSWORD").ok();
    let manager = if let Some(ref dir) = cfg.wallet_dir {
        core_logic::WalletManager::with_wallet_dir(dir)?
    } else {
        core_logic::WalletManager::new()?
    };
    let total_wallets = manager.count();
    info!(
        "Wallet manager loaded | total_wallets={} | dir={:?}",
        total_wallets, cfg.wallet_dir
    );
    if total_wallets == 0 {
        warn!("No wallet files found.");
        println!("No wallet files found.");
        return Ok(());
    }

    // Initialize Address Cache
    arc_project::utils::address_cache::AddressCache::init()?;
    debug!("Address cache initialized.");

    // 3. Select wallet
    let wallet_idx = if let Some(idx) = args.wallet {
        let clamped = idx.min(total_wallets - 1);
        if clamped != idx {
            warn!("Wallet index {} out of range, clamped to {}", idx, clamped);
        }
        clamped
    } else if args.all {
        0
    } else {
        println!("\n? Select Wallet to debug >");
        let wallet_names = manager.list_wallets();
        let mut choices = vec!["Pick Random Wallet".to_string()];
        for (i, name) in wallet_names.iter().enumerate() {
            choices.push(format!("Wallet {}: {}", i, name));
        }

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select Wallet")
            .default(0)
            .items(&choices)
            .interact()?;

        if selection == 0 {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            rng.gen_range(0..total_wallets)
        } else {
            selection - 1
        }
    };
    debug!("Selected wallet index: {}", wallet_idx);

    // 4. Verify decryption
    if let Err(e) = manager.get_wallet(wallet_idx, password.as_deref()).await {
        error!("Decryption failed for wallet {}: {}", wallet_idx, e);
        println!("\n??  Decryption failed for wallet {}: {}", wallet_idx, e);
        println!("Please check WALLET_PASSWORD environment variable.");
        return Ok(());
    }
    info!("Wallet {} decryption verified.", wallet_idx);

    // 5. Create provider
    let proxy_url = if args.no_proxy {
        None
    } else {
        let proxies = core_logic::ProxyManager::load_proxies()?;
        if !proxies.is_empty() {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let p = &proxies[rng.gen_range(0..proxies.len())];
            let url = if let (Some(user), Some(pass)) = (&p.username, &p.password) {
                let host = p.url.trim_start_matches("http://");
                format!("http://{}:{}@{}", user, pass, host)
            } else {
                p.url.clone()
            };
            // Masked version for display only
            let display_str = if let (Some(user), Some(_pass)) = (&p.username, &p.password) {
                let host = p.url.trim_start_matches("http://");
                format!("http://{}:***@{}", user, host)
            } else {
                p.url.clone()
            };
            debug!("Using proxy: {}", display_str);
            Some(url)
        } else {
            None
        }
    };

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        ),
    );
    let client_builder = reqwest::Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(30));
    let client = if let Some(ref proxy_str) = proxy_url {
        debug!(
            "Building HTTP client with proxy: {}",
            display_proxy(proxy_str)
        );
        match reqwest::Proxy::all(proxy_str) {
            Ok(p) => client_builder.proxy(p).build().unwrap_or_default(),
            Err(_) => client_builder.build().unwrap_or_default(),
        }
    } else {
        client_builder.build().unwrap_or_default()
    };

    let provider = Provider::new(Http::new_with_client(Url::parse(&cfg.rpc_url)?, client));
    info!("RPC provider created | url={}", cfg.rpc_url);

    // 6. Create task list
    let tasks: Vec<Box<dyn ArcTask>> = vec![
        Box::new(ArcCheckBalanceTask),
        Box::new(SendUsdcTask),
        Box::new(SendEurcTask),
        Box::new(SendCirbtcTask),
    ];
    let items: Vec<&str> = tasks.iter().map(|t| t.name()).collect();
    debug!("Available tasks: {:?}", items);

    // 7. Select task
    let task_idx = if let Some(t_id) = args.task {
        let prefix1 = format!("{}_", t_id);
        let prefix2 = format!("{:02}_", t_id);
        if let Some(pos) = tasks
            .iter()
            .position(|t| t.name().starts_with(&prefix1) || t.name().starts_with(&prefix2))
        {
            pos
        } else if t_id < tasks.len() {
            warn!("Task ID {} not found by name, using index {}", t_id, t_id);
            t_id
        } else {
            error!("Invalid task ID: {}", t_id);
            return Ok(());
        }
    } else {
        Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select task to debug")
            .default(0)
            .items(&items)
            .interact()?
    };

    let selected_task = &tasks[task_idx];
    println!("\n  Task: {}", selected_task.name());

    // 8. Get wallet
    let decrypted = manager.get_wallet(wallet_idx, password.as_deref()).await?;
    let key = decrypted.evm_private_key.clone();
    let wallet = key.parse::<LocalWallet>()?.with_chain_id(cfg.chain_id);
    let addr = wallet.address();
    info!("Using wallet | idx={} | address={:?}", wallet_idx, addr);

    // 9. Pre-flight diagnostics
    if args.quiet < 2 {
        match provider.get_block_number().await {
            Ok(bn) => info!("Pre-flight | block_number={}", bn),
            Err(e) => warn!("Pre-flight | failed to get block number: {}", e),
        }
        match provider.get_gas_price().await {
            Ok(gp) => info!(
                "Pre-flight | gas_price={} gwei",
                ethers::utils::format_units(gp, "gwei").unwrap_or_default()
            ),
            Err(e) => warn!("Pre-flight | failed to get gas price: {}", e),
        }
        match provider.get_balance(addr, None).await {
            Ok(bal) => info!(
                "Pre-flight | balance={} {}",
                ethers::utils::format_ether(bal),
                cfg.symbol
            ),
            Err(e) => warn!("Pre-flight | failed to get balance: {}", e),
        }
        match provider.get_transaction_count(addr, None).await {
            Ok(n) => info!("Pre-flight | nonce={}", n),
            Err(e) => warn!("Pre-flight | failed to get nonce: {}", e),
        }
        info!(
            "Gas config | min_gwei={} | max_gwei={}",
            args.min_gwei, args.max_gwei
        );
    }

    // 10. Create GasManager
    let gas_manager = Arc::new(arc_project::utils::gas::GasManager::with_max(
        Arc::new(provider.clone()),
        args.min_gwei,
        args.max_gwei,
    ));
    debug!(
        "GasManager created | min_gwei={} | max_gwei={}",
        args.min_gwei, args.max_gwei
    );

    // 11. Create TaskContext
    let ctx = TaskContext {
        provider: provider.clone(),
        wallet,
        config: cfg.clone(),
        proxy: proxy_url,
        db: None,
        gas_manager,
    };

    // 12. Run task
    println!("  Running...\n");
    let start = std::time::Instant::now();

    match selected_task.run(ctx).await {
        Ok(res) => {
            let elapsed = start.elapsed();
            if res.success {
                println!("  ✅ Success ({:.1}s)", elapsed.as_secs_f64());
                println!("  {}", res.message);
                info!(
                    "Task completed | success=true | duration={:.1}s | output={}",
                    elapsed.as_secs_f64(),
                    res.message
                );
            } else {
                println!("  ❌ Failed ({:.1}s)", elapsed.as_secs_f64());
                println!("  {}", res.message);
                warn!(
                    "Task completed | success=false | duration={:.1}s | output={}",
                    elapsed.as_secs_f64(),
                    res.message
                );
            }
        }
        Err(e) => {
            let elapsed = start.elapsed();
            println!("  💥 Error ({:.1}s)", elapsed.as_secs_f64());
            println!("  {:#}", e);
            error!(
                "Task error | duration={:.1}s | error={:#}",
                elapsed.as_secs_f64(),
                e
            );
        }
    }

    // 13. Post-flight block info
    if args.quiet < 2 {
        if let Ok(bn) = provider.get_block_number().await {
            info!("Post-flight | block_number={}", bn);
        }
    }

    println!();
    Ok(())
}
