use crate::config::SepoliaConfig;
use crate::task::t01_check_balance::SepoliaCheckBalanceTask;
use crate::task::t02_mint_usdt_plus::MintUsdtPlusTask;
use crate::task::t03_mint_usdc_plus::MintUsdcPlusTask;
use crate::task::t04_redeem_usdt_plus::RedeemUsdtPlusTask;
use crate::task::t05_redeem_usdc_plus::RedeemUsdcPlusTask;
use crate::task::t06_stake_usdt_plus::StakeUsdtPlusTask;
use crate::task::t07_stake_usdc_plus::StakeUsdcPlusTask;
use crate::task::t08_unstake_tplus::UnstakeTplusTask;
use crate::task::t09_unstake_cplus::UnstakeCplusTask;
use crate::task::t10_aave_usdt_faucet::AaveUsdtFaucetTask;
use crate::task::t11_aave_usdc_faucet::AaveUsdcFaucetTask;
use crate::task::t12_bridge_tplus::BridgeTplusTask;
use crate::task::t13_bridge_cplus::BridgeCplusTask;
use crate::task::t14_send_random_usdt_plus::SendRandomUsdtPlusTask;
use crate::task::t15_send_random_usdc_plus::SendRandomUsdcPlusTask;
use crate::task::t16_bridge_back_tplus::BridgeBackTplusTask;
use crate::task::t17_bridge_back_cplus::BridgeBackCplusTask;
use crate::task::t18_receive_tplus::ReceiveTplusTask;
use crate::task::t19_receive_cplus::ReceiveCplusTask;
use crate::task::t20_aave_wbtc_faucet::AaveWbtcFaucetTask;
use crate::task::t21_redeem_to_ausdt::RedeemToAusdtTask;
use crate::task::t22_redeem_to_ausdc::RedeemToAusdcTask;
use crate::task::{SepoliaTask, TaskContext};
use anyhow::{Context, Result};
use async_trait::async_trait;
use colored::Colorize;
use core_logic::config::SpamConfig;
use core_logic::traits::Spammer;
use core_logic::WalletManager;
use ethers::prelude::*;
use rand::distributions::{Distribution, WeightedIndex};
use rand::rngs::OsRng;
use rand::Rng;
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
    tasks: Vec<Box<dyn SepoliaTask>>,
    sepolia_config: SepoliaConfig,
    wallet_id: String,
    proxy_pool: Arc<tokio::sync::RwLock<Vec<core_logic::config::ProxyConfig>>>,
    proxy_health: Arc<core_logic::ProxyHealthManager>,
    proxy_rate_limiter: Arc<core_logic::ProxyRateLimiter>,
    db: Option<Arc<DatabaseManager>>,
    gas_manager: Arc<crate::utils::gas::GasManager>,
    dist: WeightedIndex<u32>,
    total_wallets: usize,
    busy_wallets: Arc<Mutex<HashSet<usize>>>,
    /// Base Sepolia RPC URL for bridge-back tasks (t16, t17)
    base_rpc_url: Option<String>,
    /// Base Sepolia gas manager
    base_gas_manager: Option<Arc<crate::utils::gas::GasManager>>,
    /// Base Sepolia config
    base_config: Option<SepoliaConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_returns_error_with_message() {
        let config = SpamConfig {
            rpc_url: "http://localhost:8545".into(),
            chain_id: 1,
            target_tps: 10,
            duration_seconds: None,
            wallet_source: core_logic::config::WalletSource::File {
                path: "wallet.json".into(),
                encrypted: true,
            },
        };
        let result = EvmSpammer::new(config).await;
        assert!(result.is_err());
        match result {
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("new_with_signer"),
                    "Error should mention new_with_signer: {}",
                    msg
                );
            }
            _ => panic!("Expected Err"),
        }
    }
}

impl EvmSpammer {
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_signer(
        spam_config: SpamConfig,
        sepolia_config: SepoliaConfig,
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
        base_rpc_url: Option<String>,
        base_config: Option<SepoliaConfig>,
    ) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            ),
        );

        let client_builder = Client::builder().default_headers(headers);
        let client = client_builder.build()?;

        let provider = Provider::new(Http::new_with_client(
            reqwest::Url::parse(&spam_config.rpc_url)?,
            client,
        ));

        let tasks: Vec<Box<dyn SepoliaTask>> = vec![
            Box::new(SepoliaCheckBalanceTask),
            Box::new(MintUsdtPlusTask),
            Box::new(MintUsdcPlusTask),
            Box::new(RedeemUsdtPlusTask),
            Box::new(RedeemUsdcPlusTask),
            Box::new(StakeUsdtPlusTask),
            Box::new(StakeUsdcPlusTask),
            Box::new(UnstakeTplusTask),
            Box::new(UnstakeCplusTask),
            Box::new(AaveUsdtFaucetTask),
            Box::new(AaveUsdcFaucetTask),
            Box::new(BridgeTplusTask),
            Box::new(BridgeCplusTask),
            Box::new(SendRandomUsdtPlusTask),
            Box::new(SendRandomUsdcPlusTask),
            Box::new(BridgeBackTplusTask),
            Box::new(BridgeBackCplusTask),
            Box::new(ReceiveTplusTask),
            Box::new(ReceiveCplusTask),
            Box::new(AaveWbtcFaucetTask),
            Box::new(RedeemToAusdtTask),
            Box::new(RedeemToAusdcTask),
        ];

        let gas_manager = Arc::new(crate::utils::gas::GasManager::with_max(
            Arc::new(provider.clone()),
            min_gwei,
            max_gwei,
        ));

        let weights: Vec<u32> = tasks
            .iter()
            .map(|t| {
                let w = t.weight();
                info!("Task '{}': Weight {}", t.name(), w);
                w
            })
            .collect();

        let dist = match WeightedIndex::new(&weights) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("Failed to create weighted distribution: {}", e);
                WeightedIndex::new(vec![1; weights.len()]).unwrap_or_else(|e| {
                    tracing::error!("Critical error creating distribution: {}", e);
                    WeightedIndex::new(vec![1]).expect("Failed to create fallback distribution")
                })
            }
        };

        // Create base Sepolia gas manager if base config is provided
        let base_gas_manager = base_config.as_ref().map(|_| {
            let base_provider = Provider::new(Http::new_with_client(
                reqwest::Url::parse(base_rpc_url.as_deref().unwrap())
                    .expect("Invalid base RPC URL"),
                reqwest::Client::new(),
            ));
            Arc::new(crate::utils::gas::GasManager::with_max(
                Arc::new(base_provider),
                min_gwei,
                max_gwei,
            ))
        });

        Ok(Self {
            config: spam_config,
            provider,
            wallet_manager,
            wallet_password,
            tasks,
            sepolia_config,
            wallet_id,
            proxy_pool,
            proxy_health,
            proxy_rate_limiter,
            db,
            gas_manager,
            dist,
            total_wallets,
            busy_wallets,
            base_rpc_url,
            base_gas_manager,
            base_config,
        })
    }

    async fn create_provider_with_proxy(
        &self,
        proxy_config: &Option<core_logic::config::ProxyConfig>,
        rpc_url: &str,
    ) -> Result<Provider<Http>> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            ),
        );

        let mut client_builder = Client::builder().default_headers(headers);

        if let Some(proxy_conf) = proxy_config {
            let mut proxy = reqwest::Proxy::all(&proxy_conf.url)
                .context("Invalid proxy URL")?;
            if let (Some(u), Some(p)) = (&proxy_conf.username, &proxy_conf.password) {
                proxy = proxy.basic_auth(u, p);
            }
            client_builder = client_builder.proxy(proxy);
        }

        let client = client_builder
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Provider::new(Http::new_with_client(
            reqwest::Url::parse(rpc_url).context("Invalid RPC URL")?,
            client,
        )))
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
            info!(
                "Sepolia Spammer started for chain {} (base: {})",
                self.config.chain_id,
                self.base_config
                    .as_ref()
                    .map(|c| c.chain_id.to_string())
                    .unwrap_or_else(|| "none".into())
            );
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

                    let is_base_task =
                        task.name() == "16_bridgeBackTplus" || task.name() == "17_bridgeBackCplus";

                    let rpc_url = if is_base_task {
                        self.base_rpc_url.as_deref().unwrap_or(&self.config.rpc_url)
                    } else {
                        &self.config.rpc_url
                    };

                    let provider = self
                        .create_provider_with_proxy(&proxy_config, rpc_url)
                        .await?;

                    let mut rng = OsRng;
                    let wallet_idx = loop {
                        let idx = rng.gen_range(0..self.total_wallets);
                        let mut busy = self.busy_wallets.lock().await;
                        if busy.insert(idx) {
                            break idx;
                        }
                        drop(busy);
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    };

                    let wallet = match self
                        .wallet_manager
                        .get_wallet(wallet_idx, self.wallet_password.as_deref())
                        .await
                    {
                        Ok(decrypted) => {
                            let key = decrypted.evm_private_key.clone();
                            let chain_id = if is_base_task {
                                self.base_config
                                    .as_ref()
                                    .map(|c| c.chain_id)
                                    .unwrap_or(self.config.chain_id)
                            } else {
                                self.config.chain_id
                            };
                            match key.parse::<LocalWallet>() {
                                Ok(w) => w.with_chain_id(chain_id),
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

                    let ctx_gas_manager = if is_base_task {
                        self.base_gas_manager
                            .as_ref()
                            .unwrap_or(&self.gas_manager)
                            .clone()
                    } else {
                        self.gas_manager.clone()
                    };

                    let ctx_config = if is_base_task {
                        self.base_config
                            .as_ref()
                            .unwrap_or(&self.sepolia_config)
                            .clone()
                    } else {
                        self.sepolia_config.clone()
                    };

                    let ctx = TaskContext {
                        provider,
                        wallet,
                        config: ctx_config,
                        proxy: proxy_config.as_ref().map(|p| p.url.clone()),
                        db: self.db.clone(),
                        gas_manager: ctx_gas_manager,
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

                let sleep_ms = if let (Some(min), Some(max)) = (
                    self.sepolia_config.min_delay_ms,
                    self.sepolia_config.max_delay_ms,
                ) {
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
        info!("Sepolia Spammer stopping...");
        Ok(())
    }
}
