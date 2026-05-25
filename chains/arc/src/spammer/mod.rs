use crate::config::ArcConfig;
use crate::task::t01_check_balance::ArcCheckBalanceTask;
use crate::task::t02_send_usdc::SendUsdcTask;
use crate::task::t03_send_eurc::SendEurcTask;
use crate::task::t04_send_cirbtc::SendCirbtcTask;
use crate::task::{ArcTask, TaskContext};
use anyhow::Result;
use async_trait::async_trait;
use colored::Colorize;
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
    tasks: Vec<Box<dyn ArcTask>>,
    arc_config: ArcConfig,
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

fn get_task_weight(_name: &str) -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_task_weight_always_one() {
        assert_eq!(get_task_weight("01_checkBalance"), 1);
        assert_eq!(get_task_weight("02_faucetUsdc"), 1);
        assert_eq!(get_task_weight(""), 1);
        assert_eq!(get_task_weight("anything"), 1);
    }

    #[test]
    fn test_get_task_weight_extreme_names() {
        let long = "a".repeat(10_000);
        assert_eq!(get_task_weight(&long), 1);
        assert_eq!(get_task_weight("🔥🔥🔥"), 1);
        assert_eq!(get_task_weight("ñøñçé_ßüşîñéšš"), 1);
        assert_eq!(get_task_weight("  spaced  "), 1);
    }
}

impl EvmSpammer {
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_signer(
        spam_config: SpamConfig,
        arc_config: ArcConfig,
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
        min_gwei: f64,
        max_gwei: f64,
    ) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            ),
        );

        let client_builder = Client::builder()
            .default_headers(headers)
            .pool_max_idle_per_host(10)
            .tcp_keepalive(std::time::Duration::from_secs(30));
        let client = client_builder.build()?;

        let provider = Provider::new(Http::new_with_client(
            reqwest::Url::parse(&spam_config.rpc_url)?,
            client,
        ));

        let tasks: Vec<Box<dyn ArcTask>> = vec![
            Box::new(ArcCheckBalanceTask),
            Box::new(SendUsdcTask),
            Box::new(SendEurcTask),
            Box::new(SendCirbtcTask),
        ];

        let gas_manager = Arc::new(crate::utils::gas::GasManager::with_max(
            Arc::new(provider.clone()),
            min_gwei,
            max_gwei,
        ));

        let weights: Vec<u32> = tasks
            .iter()
            .map(|t| {
                let w = get_task_weight(t.name());
                info!("Task '{}': Weight {}", t.name(), w);
                w
            })
            .collect();
        let weight_len = weights.len();

        let dist = match WeightedIndex::new(weights) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("Failed to create weighted distribution: {}", e);
                WeightedIndex::new(vec![1; weight_len]).unwrap_or_else(|e| {
                    tracing::error!("Critical error creating distribution: {}", e);
                    WeightedIndex::new(vec![1]).expect("Failed to create fallback distribution")
                })
            }
        };

        Ok(Self {
            config: spam_config,
            provider,
            wallet_manager,
            wallet_password,
            tasks,
            arc_config,
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

    async fn create_provider_with_proxy(
        &self,
        proxy_config: &Option<core_logic::config::ProxyConfig>,
    ) -> Provider<Http> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            ),
        );

        let mut client_builder = Client::builder()
            .default_headers(headers)
            .pool_max_idle_per_host(10)
            .tcp_keepalive(std::time::Duration::from_secs(30));

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
        let span = tracing::info_span!("spammer_context", wallet_id = self.wallet_id.as_str());

        async move {
            info!("ARC Spammer started for chain {}", self.config.chain_id);
            let mut stats = core_logic::traits::SpammerStats::default();

            loop {
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
                    let (proxy_config, proxy_id_str) = {
                        let proxies = self.proxy_pool.read().await;
                        if proxies.is_empty() {
                            (None, "000".to_string())
                        } else {
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

                    if let Some(ref proxy) = proxy_config {
                        self.proxy_rate_limiter
                            .wait_until_available(&proxy.url)
                            .await;
                    }

                    let provider = self.create_provider_with_proxy(&proxy_config).await;

                    let mut rng = OsRng;
                    let wallet_idx = loop {
                        let idx = rng.gen_range(0..self.total_wallets);
                        let mut busy = self.busy_wallets.lock().await;
                        if busy.insert(idx) {
                            break idx;
                        }
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
                        config: self.arc_config.clone(),
                        proxy: proxy_config.as_ref().map(|p| {
                            // Format with auth for Obscura proxy: http://user:pass@host:port
                            if let (Some(user), Some(pass)) = (&p.username, &p.password) {
                                let host = p.url.trim_start_matches("http://");
                                format!("http://{}:{}@{}", user, pass, host)
                            } else {
                                p.url.clone()
                            }
                        }),
                        db: self.db.clone(),
                        gas_manager: self.gas_manager.clone(),
                    };

                    let start_time = std::time::Instant::now();
                    match task.run(ctx).await {
                        Ok(res) => {
                            if let Some(ref proxy) = proxy_config {
                                if res.success {
                                    self.proxy_health.record_success(&proxy.url).await;
                                } else {
                                    self.proxy_health.record_failure(&proxy.url).await;
                                }
                            }

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
                            let status_str = if res.success {
                                "Success".green().bold()
                            } else {
                                "Failed ".red().bold()
                            };

                            info!(
                                target: "task_result",
                                "[WK:{}][WL:{}][P:{}] {} [{}] {} (B: {}) in {:.1}s",
                                self.wallet_id,
                                wallet_idx,
                                proxy_id_str,
                                status_str,
                                task.name(),
                                res.message,
                                block_num,
                                duration.as_secs_f64()
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
                            if let Some(ref proxy) = proxy_config {
                                self.proxy_health.record_failure(&proxy.url).await;
                            }

                            stats.failed += 1;
                            let duration = start_time.elapsed();

                            warn!(
                                target: "task_result",
                                "[WK:{}][WL:{}][P:{}] {} [{}] {} in {:.1}s",
                                self.wallet_id,
                                wallet_idx,
                                proxy_id_str,
                                "Failed ".red().bold(),
                                task.name(),
                                format!("{:#}", e),
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
                    self.busy_wallets.lock().await.remove(&wallet_idx);
                }

                let sleep_ms = if let (Some(min), Some(max)) =
                    (self.arc_config.min_delay_ms, self.arc_config.max_delay_ms)
                {
                    let mut rng = OsRng;
                    rng.gen_range(min..=max)
                } else {
                    1000 / self.config.target_tps.max(1) as u64
                };

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
        info!("ARC Spammer stopping...");
        Ok(())
    }
}
