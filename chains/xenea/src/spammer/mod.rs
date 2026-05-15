use crate::config::XeneaConfig;
use crate::task::t01_check_balance::XeneaCheckBalanceTask;
use crate::task::t02_simple_native_transfer::SimpleEthTransferTask;
use crate::task::t03_deploy_contract::XeneaDeployContractTask;
use crate::task::t04_interact_contract::XeneaInteractContractTask;
use crate::task::t05_self_transfer::SelfTransferTask;
use crate::task::t06_send_meme::SendMemeTokenTask;
use crate::task::t07_create_meme::CreateMemeTask;
use crate::task::t11_batch_transfer::BatchTransferTask;
use crate::task::t12_nft_mint::NftMintTask;
use crate::task::t13_nft_transfer::NftTransferTask;
use crate::task::t14_approve_token::ApproveTokenTask;
use crate::task::t16_multicall::MulticallTask;
use crate::task::t18_contract_call_raw::ContractCallRawTask;
use crate::task::t19_high_gas_limit::HighGasLimitTask;
use crate::task::t20_gas_price_test::GasPriceTestTask;
use crate::task::t21_erc1155_mint::Erc1155MintTask;
use crate::task::t22_erc1155_transfer::Erc1155TransferTask;
use crate::task::t24_create2_deploy::Create2DeployTask;
use crate::task::t26_verify_signature::VerifySignatureTask;
use crate::task::t27_permit_token::PermitTokenTask;
use crate::task::t28_delegatecall::DelegatecallTask;
use crate::task::t29_cross_contract_call::CrossContractCallTask;
use crate::task::t30_revert_test::RevertTestTask;
use crate::task::t31_event_emission::EventEmissionTask;
use crate::task::t32_eth_with_data::EthWithDataTask;
use crate::task::t33_batch_approve::BatchApproveTask;
use crate::task::t36_create2_factory::Create2FactoryTask;
use crate::task::t37_uups_proxy::UUPSProxyTask;
use crate::task::t38_transparent_proxy::TransparentProxyTask;
use crate::task::t42_erc721_mint::ERC721MintTask;
use crate::task::t43_erc1155_batch::ERC1155BatchTask;
use crate::task::t54_gas_price_zero::GasPriceZeroTask;
use crate::task::t61_mint_meme::MintMemeTask;
use crate::task::t62_batch_send_meme::BatchSendCreatedMemeTask;
use crate::task::t63_burn_meme::BurnMemeTask;
use crate::task::{TaskContext, XeneaTask};
use anyhow::Result;
use async_trait::async_trait;
use core_logic::config::SpamConfig;
use core_logic::traits::Spammer;
use core_logic::WalletManager;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;

use rand::distributions::{Distribution, WeightedIndex};
use reqwest::Client;
use tokio::time::{sleep, Duration};
use tracing::{info, warn, Instrument};

use core_logic::database::DatabaseManager;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub struct EvmSpammer {
    config: SpamConfig,
    provider: Provider<Http>,
    wallet_manager: Arc<WalletManager>,
    wallet_password: Option<String>,
    tasks: Vec<Box<XeneaTask>>,
    xenea_config: XeneaConfig,
    wallet_id: String,
    /// Shared proxy pool for rotation
    proxy_pool: Arc<tokio::sync::RwLock<Vec<core_logic::config::ProxyConfig>>>,
    /// Proxy health manager for tracking failures
    proxy_health: Arc<core_logic::ProxyHealthManager>,
    /// Proxy rate limiter
    proxy_rate_limiter: Arc<core_logic::ProxyRateLimiter>,
    db: Option<Arc<DatabaseManager>>,
    gas_manager: Arc<crate::utils::gas::GasManager>,
    dist: WeightedIndex<u32>,
    total_wallets: usize,
    /// Shared lock to prevent nonce conflicts between workers
    busy_wallets: Arc<Mutex<HashSet<usize>>>,
}

fn get_task_weight(name: &str) -> u32 {
    // All tasks have equal weight - native token already distributed evenly
    let _ = name; // Suppress unused variable warning
    1
}

impl EvmSpammer {
    // Modified constructor to accept proxy pool, health manager, and rate limiter
    pub fn new_with_signer(
        spam_config: SpamConfig,
        xenea_config: XeneaConfig,
        _signer: LocalWallet,
        proxy_pool: Arc<tokio::sync::RwLock<Vec<core_logic::config::ProxyConfig>>>,
        proxy_health: Arc<core_logic::ProxyHealthManager>,
        proxy_rate_limiter: Arc<core_logic::ProxyRateLimiter>,
        wallet_id: String,
        db: Option<Arc<DatabaseManager>>,
        wallet_manager: Arc<WalletManager>,
        wallet_password: Option<String>,
        total_wallets: usize,
        busy_wallets: Arc<Mutex<HashSet<usize>>>,
    ) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json, text/plain, */*"),
        );
        headers.insert(
            reqwest::header::ACCEPT_LANGUAGE,
            reqwest::header::HeaderValue::from_static("en-US,en;q=0.9"),
        );

        let client_builder = Client::builder().default_headers(headers);
        let client = client_builder.build()?;

        let provider = Provider::new(Http::new_with_client(
            reqwest::Url::parse(&spam_config.rpc_url)?,
            client,
        ));

        let tasks: Vec<Box<XeneaTask>> = vec![
            Box::new(XeneaCheckBalanceTask),
            Box::new(SimpleEthTransferTask),
            Box::new(XeneaDeployContractTask),
            Box::new(XeneaInteractContractTask),
            Box::new(SelfTransferTask),
            Box::new(CreateMemeTask),
            Box::new(SendMemeTokenTask),
            Box::new(BatchTransferTask),
            Box::new(NftMintTask),
            Box::new(NftTransferTask),
            Box::new(ApproveTokenTask),
            Box::new(MulticallTask),
            Box::new(ContractCallRawTask),
            Box::new(HighGasLimitTask),
            Box::new(GasPriceTestTask),
            Box::new(Erc1155MintTask),
            Box::new(Erc1155TransferTask),
            Box::new(Create2DeployTask),
            Box::new(VerifySignatureTask),
            Box::new(PermitTokenTask),
            Box::new(DelegatecallTask),
            Box::new(CrossContractCallTask),
            Box::new(RevertTestTask),
            Box::new(EventEmissionTask),
            Box::new(EthWithDataTask),
            Box::new(BatchApproveTask),
            Box::new(Create2FactoryTask),
            Box::new(UUPSProxyTask),
            Box::new(TransparentProxyTask),
            Box::new(ERC721MintTask),
            Box::new(ERC1155BatchTask),
            Box::new(GasPriceZeroTask),
            Box::new(MintMemeTask),
            Box::new(BatchSendCreatedMemeTask),
            Box::new(BurnMemeTask),
        ];

        let gas_manager = Arc::new(crate::utils::gas::GasManager::new(Arc::new(
            provider.clone(),
        )));

        // Calculate weights
        let weights: Vec<u32> = tasks
            .iter()
            .map(|t| {
                let w = get_task_weight(t.name());
                info!("Task '{}': Weight {}", t.name(), w);
                w
            })
            .collect();

        // Create weighted distribution with fallback for invalid weights
        let dist = match WeightedIndex::new(&weights) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(
                    target: "smart_main",
                    "Failed to create weighted distribution for tasks, using uniform distribution: {}",
                    e
                );
                // Fallback: all tasks have equal weight
                WeightedIndex::new(&vec![1; weights.len()]).unwrap_or_else(|e| {
                    // Ultimate fallback - single task with weight 1
                    tracing::error!(target: "smart_main", "Critical error creating distribution: {}", e);
                    WeightedIndex::new(&vec![1]).expect("Failed to create fallback distribution")
                })
            }
        };

        Ok(Self {
            config: spam_config,
            provider,
            wallet_manager,
            wallet_password,
            tasks,
            xenea_config,
            wallet_id,
            proxy_pool,
            proxy_health,
            proxy_rate_limiter,
            db,
            gas_manager,
            dist,
            total_wallets,
            busy_wallets,
        })
    }

    /// Create a new provider with the given proxy config
    async fn create_provider_with_proxy(
        &self,
        proxy_config: &Option<core_logic::config::ProxyConfig>,
    ) -> Provider<Http> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json, text/plain, */*"),
        );
        headers.insert(
            reqwest::header::ACCEPT_LANGUAGE,
            reqwest::header::HeaderValue::from_static("en-US,en;q=0.9"),
        );

        let mut client_builder = Client::builder().default_headers(headers);

        if let Some(proxy_conf) = proxy_config {
            let mut proxy = reqwest::Proxy::all(&proxy_conf.url).unwrap();
            if let (Some(u), Some(p)) = (&proxy_conf.username, &proxy_conf.password) {
                proxy = proxy.basic_auth(u, p);
            }
            client_builder = client_builder.proxy(proxy);
        }

        let client = client_builder.build().expect("Failed to build HTTP client");

        Provider::new(Http::new_with_client(
            reqwest::Url::parse(&self.config.rpc_url).expect("Invalid RPC URL"),
            client,
        ))
    }
}

#[async_trait]
impl Spammer for EvmSpammer {
    async fn new(_config: SpamConfig) -> Result<Self> {
        Err(anyhow::anyhow!("Use new_with_signer construction"))
    }

    async fn start(
        &self,
        cancellation_token: CancellationToken,
    ) -> Result<core_logic::traits::SpammerStats> {
        // Create context span
        let span = tracing::info_span!("spammer_context", wallet_id = self.wallet_id.as_str());

        async move {
            info!("XENEA Spammer started for chain {}", self.config.chain_id);
            let mut stats = core_logic::traits::SpammerStats::default();

            loop {
                // Check if cancelled before starting task
                if cancellation_token.is_cancelled() {
                    info!("Worker stopping (cancelled).");
                    break;
                }

                let task = {
                    let mut rng = OsRng;
                    let idx = self.dist.sample(&mut rng);
                    self.tasks.get(idx)
                };

                if let Some(task) = task {
                    // Get random proxy from pool for this task (filter out paused proxies)
                    let (proxy_config, proxy_id_str) = {
                        let proxies = self.proxy_pool.read().await;
                        if proxies.is_empty() {
                            (None, "000".to_string())
                        } else {
                            // Filter out paused proxies
                            let mut available_proxies: Vec<_> = Vec::new();
                            for (i, p) in proxies.iter().enumerate() {
                                if self.proxy_health.is_available(&p.url).await {
                                    available_proxies.push((i, p));
                                }
                            }

                            if available_proxies.is_empty() {
                                warn!("No healthy proxies available!");
                                (None, "000".to_string())
                            } else {
                                let mut rng = OsRng;
                                let idx = rng.gen_range(0..available_proxies.len());
                                let (original_idx, proxy) = available_proxies[idx];
                                (Some(proxy.clone()), format!("{:03}", original_idx + 1))
                            }
                        }
                    };

                    // Apply rate limit before executing task
                    if let Some(ref proxy) = proxy_config {
                        self.proxy_rate_limiter.wait_until_available(&proxy.url).await;
                    }

                    // Create provider with selected proxy
                    let provider = self.create_provider_with_proxy(&proxy_config).await;

                    // Pick random wallet for this task (with lock to prevent nonce conflicts)
                    let mut rng = OsRng;
                    let wallet_idx = loop {
                        let idx = rng.gen_range(0..self.total_wallets);
                        let mut busy = self.busy_wallets.lock().await;
                        if busy.insert(idx) {
                            break idx;
                        }
                        // Wallet is busy, try another
                        drop(busy);
                    };

                    let wallet = match self
                        .wallet_manager
                        .get_wallet(wallet_idx, self.wallet_password.as_deref())
                        .await
                    {
                        Ok(decrypted) => {
                            let key = decrypted.evm_private_key.clone();
                            match key.parse::<LocalWallet>() {
                                Ok(w) => w.with_chain_id(self.config.chain_id),
                                Err(e) => {
                                    self.busy_wallets.lock().await.remove(&wallet_idx);
                                    warn!("Failed to parse wallet {}: {}", wallet_idx, e);
                                    continue;
                                }
                            }
                        }
                        Err(e) => {
                            self.busy_wallets.lock().await.remove(&wallet_idx);
                            warn!("Failed to decrypt wallet {}: {}", wallet_idx, e);
                            continue;
                        }
                    };

                    let wallet_address = wallet.address();

                    let ctx = TaskContext {
                        provider,
                        wallet,
                        config: self.xenea_config.clone(),
                        proxy: proxy_config.as_ref().map(|p| p.url.clone()),
                        db: self.db.clone(),
                        gas_manager: self.gas_manager.clone(),
                    };

                    let start_time = std::time::Instant::now();
                    match task.run(ctx).await {
                        Ok(res) => {
                            // Track proxy health
                            if let Some(ref proxy) = proxy_config {
                                if res.success {
                                    self.proxy_health.record_success(&proxy.url).await;
                                } else {
                                    self.proxy_health.record_failure(&proxy.url).await;
                                }
                            }

                            // Only count as success if task returned success: true
                            if res.success {
                                stats.success += 1;
                            } else {
                                stats.failed += 1;
                            }
                            let duration = start_time.elapsed();
                            let block_num = match self.provider.get_block_number().await {
                                Ok(n) => n.to_string(),
                                Err(_) => "???".to_string(),
                            };

                            use colored::*;
                            // Helper for coloring
                            fn format_colored_message(msg: &str) -> String {
                                // Regex to find addresses 0x... and numbers
                                use regex::Regex;

                                // Color Addresses (Orange approx) -> using custom color if terminal supports, or Yellow/Red mix?
                                // colored crate supports .truecolor(r,g,b) or .custom("color")?
                                // Actually colored::Color::TrueColor usually works on modern terms.
                                // User asked for Orange. RGB (255, 165, 0).
                                // User asked for Orange. RGB (255, 165, 0).

                                // Replace numbers (decimals or integers) that are NOT part of address (hard with pure regex replacement on string that already has ansi codes).
                                // better approach: Regex find all tokens, colorize based on type.
                                // Simplest: Just regex numbers that are surrounded by space or start/end of string?
                                // \b\d+(\.\d+)?\b
                                // CAUTION: If we run this AFTER address coloring, the ANSI codes themselves have numbers (e.g. [38;2;...]).
                                // So we must be careful.
                                // Strategy: Capture text parts, reconstruct.
                                // OR: strict regex that excludes the ANSI patterns.

                                // Let's try to match numbers that are likely amounts/blocks.
                                // Given complexity, let's just color numbers in the raw message FIRST, BUT addresses contain numbers.
                                // Addresses start with 0x.

                                // CORRECT APPROACH:
                                // 1. Identify addresses and color them.
                                // 2. Identify numbers that are NOT inside addresses and color them.
                                // This is hard to do in two passes on string.
                                // One pass regex: (0x[a-fA-F0-9]+)|(\d+(\.\d+)?)
                                let token_regex =
                                    Regex::new(r"(0x[a-fA-F0-9]+)|(\d+(\.\d+)?)").unwrap();

                                let final_str = token_regex
                                    .replace_all(msg, |caps: &regex::Captures| {
                                        if let Some(addr) = caps.get(1) {
                                            addr.as_str().truecolor(255, 165, 0).to_string()
                                        // Orange
                                        } else {
                                            // Number
                                            caps[0].yellow().to_string()
                                        }
                                    })
                                    .to_string();

                                final_str
                            }

                            // Clip content to ensure total line length < 200 chars
                            // Overhead is ~75 chars, so 125 chars for message is safe.
                            let raw_msg = res.message.replace("\n", " | ");
                            let msg_limit = 125;
                            let clipped_msg = if raw_msg.chars().count() > msg_limit {
                                let truncated: String =
                                    raw_msg.chars().take(msg_limit - 3).collect();
                                format!("{}...", truncated)
                            } else {
                                raw_msg
                            };

                            let colored_msg = format_colored_message(&clipped_msg);
                            let colored_block = format_colored_message(&block_num); // It's just a number

                            // Smart duration color
                            let dur_secs = duration.as_secs_f64();
                            let dur_str = format!("{:.1}s", dur_secs);
                            let colored_dur = if dur_secs < 5.0 {
                                dur_str.green()
                            } else if dur_secs < 10.0 {
                                dur_str.truecolor(255, 165, 0) // Orange
                            } else {
                                dur_str.red()
                            };

                            // Status color - use "Success" only if res.success is true, else "Failed "
                            let status_str = if res.success {
                                "Success".green().bold()
                            } else {
                                "Failed ".red().bold()
                            };

                            // User requested format: Success [TaskName] Message (B: X) in Ys
                            info!(
                                target: "task_result",
                                "[WK:{}][WL:{}][P:{}] {} [{}] {} (B: {}) in {}",
                                self.wallet_id,
                                wallet_idx,
                                proxy_id_str,
                                status_str,
                                task.name(),
                                colored_msg,
                                colored_block,
                                colored_dur
                            );

                            if let Some(db) = &self.db {
                                let _ = db
                                    .log_task_result(
                                        &wallet_idx.to_string(),
                                        &format!("{:?}", wallet_address),
                                        task.name(),
                                        res.success,
                                        &format!("{} (B: {})", res.message, block_num),
                                        duration.as_millis() as u64,
                                    )
                                    .await;
                            }
                        }
                        Err(e) => {
                            // Track proxy health for errors too
                            if let Some(ref proxy) = proxy_config {
                                self.proxy_health.record_failure(&proxy.url).await;
                            }

                            stats.failed += 1;
                            let duration = start_time.elapsed();
                            use colored::*; // Ensure trait is in scope
                            let raw_err = format!("{:#}", e).replace("\n", " | ");
                            let msg_limit = 125;
                            let clipped_err = if raw_err.chars().count() > msg_limit {
                                let truncated: String =
                                    raw_err.chars().take(msg_limit - 3).collect();
                                format!("{}...", truncated)
                            } else {
                                raw_err
                            };

                            // Status color
                            // Added trailing space for alignment with "Success" (7 chars)
                            let status_str = "Failed ".red().bold();

                            warn!(
                                target: "task_result",
                                "[WK:{}][WL:{}][P:{}] {} [{}] {} in {:.1}s",
                                self.wallet_id,
                                wallet_idx,
                                proxy_id_str,
                                status_str,
                                task.name(),
                                clipped_err,
                                duration.as_secs_f64()
                            );

                            if let Some(db) = &self.db {
                                let _ = db
                                    .log_task_result(
                                        &wallet_idx.to_string(),
                                        &format!("{:?}", wallet_address),
                                        task.name(),
                                        false,
                                        &e.to_string(),
                                        duration.as_millis() as u64,
                                    )
                                    .await;
                            }
                        }
                    }
                    // Release wallet lock so another worker can use it
                    self.busy_wallets.lock().await.remove(&wallet_idx);
                }

                // Rate limit logic
                let sleep_ms = if let (Some(min), Some(max)) = (
                    self.xenea_config.min_delay_ms,
                    self.xenea_config.max_delay_ms,
                ) {
                    let mut rng = OsRng;
                    rng.gen_range(min..=max)
                } else {
                    1000 / self.config.target_tps.max(1) as u64
                };

                // Use tokio::select! to listen for cancellation DURING sleep
                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        info!("Worker stopping (cancelled during sleep).");
                        break;
                    }
                    _ = sleep(Duration::from_millis(sleep_ms)) => {}
                }
            }
            Ok(stats)
        }
        .instrument(span)
        .await
    }

    async fn stop(&self) -> Result<()> {
        info!("XENEA Spammer stopping...");
        Ok(())
    }
}
