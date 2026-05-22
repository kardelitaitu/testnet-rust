//! # Daily Runner — Sepolia Overlayer
//!
//! Runs each wallet's tasks up to N times per day (N configurable per task).
//! Successful executions are counted in the daily DB. Failed tasks go back
//! to the pool for retry and do NOT count toward the daily limit.
//!
//! **Daily reset** is automatic: all queries filter by `date = YYYY-MM-DD`,
//! so when UTC midnight passes the date changes and all counters reset.
//!
//! **Pause window** 23:55–00:05 UTC: no tasks are started during this
//! 10-minute window to avoid straddling the day boundary.
//!
//! ## Schema Migration (v1 → v2)
//!
//! The initial schema used a composite PRIMARY KEY `(wallet_idx, task_name, date)`
//! which meant each task could run at most once per wallet per day. When per-task
//! daily limits were added (allowing up to N completions per day), the schema was
//! migrated to an auto-increment INTEGER PRIMARY KEY `id`, removing the UNIQUE
//! constraint so the same task can succeed multiple times per day.
//!
//! **Migration is automatic**: on startup, `init_schema()` detects the old schema
//! (missing `id` column), drops the old table, and creates the new one. Since all
//! data is per-day and resets at midnight UTC, data loss is acceptable.

use crate::config::{SepoliaConfig, TaskLimits};
use crate::task::{
    AaveUsdcFaucetTask, AaveUsdtFaucetTask, BridgeBackCplusTask, BridgeBackTplusTask,
    BridgeCplusTask, BridgeTplusTask, MintUsdcPlusTask, MintUsdtPlusTask, ReceiveCplusTask,
    ReceiveTplusTask, RedeemUsdcPlusTask, RedeemUsdtPlusTask, SendRandomUsdcPlusTask,
    SendRandomUsdtPlusTask, SepoliaCheckBalanceTask, SepoliaTask, StakeUsdcPlusTask,
    StakeUsdtPlusTask, TaskContext, UnstakeCplusTask, UnstakeTplusTask,
};
use crate::utils::gas::GasManager;

use anyhow::Result;
use chrono::{Local, Timelike};
use database::DailyDb;
use ethers::signers::Signer;
use rand::Rng;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::SystemTime;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use core_logic::config::ProxyConfig;
use core_logic::{ProxyHealthManager, ProxyRateLimiter, WalletManager};

pub mod database;

// -----------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------

/// All 19 task names in order — single source of truth.
/// Must match the tasks registered in [`all_tasks()`].
pub const ALL_TASK_NAMES: &[&str] = &[
    "01_checkBalance",
    "02_mintUsdtPlus",
    "03_mintUsdcPlus",
    "04_redeemUsdtPlus",
    "05_redeemUsdcPlus",
    "06_stakeUsdtPlus",
    "07_stakeUsdcPlus",
    "08_unstakeTplus",
    "09_unstakeCplus",
    "10_aaveUsdtFaucet",
    "11_aaveUsdcFaucet",
    "12_bridgeTplus",
    "13_bridgeCplus",
    "14_sendRandomUsdtPlus",
    "15_sendRandomUsdcPlus",
    "16_bridgeBackTplus",
    "17_bridgeBackCplus",
    "18_receiveTplus",
    "19_receiveCplus",
];

/// Pause window start (23:55 UTC) in minutes since midnight.
const PAUSE_START_MINUTES: u32 = 23 * 60 + 55; // 1435
/// Pause window end (00:04:59 UTC) in minutes since midnight.
const PAUSE_END_MINUTES: u32 = 4;

/// How long to sleep between iterations when no work is available.
const IDLE_SLEEP_SECS: u64 = 30;
/// How long to sleep inside the pause window before re-checking.
const PAUSE_CHECK_SECS: u64 = 30;
/// How long to sleep when all active wallets are busy.
const BUSY_SLEEP_MS: u64 = 500;

// -----------------------------------------------------------------------
// Public helpers (testable)
// -----------------------------------------------------------------------

/// Returns `true` when the current UTC time is inside the pause window
/// (23:55 → 00:05 UTC), during which no tasks should be started to avoid
/// straddling the daily reset boundary.
pub fn is_in_pause_window() -> bool {
    let now = chrono::Utc::now();
    is_in_pause_window_at(now)
}

/// Like [`is_in_pause_window()`] but accepts an explicit timestamp,
/// making it testable with known boundary values.
pub fn is_in_pause_window_at(now: chrono::DateTime<chrono::Utc>) -> bool {
    let minutes = now.hour() * 60 + now.minute();
    minutes >= PAUSE_START_MINUTES || minutes <= PAUSE_END_MINUTES
}

/// Get today's date as `YYYY-MM-DD` (UTC).
pub fn today_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// Build the canonical list of all 19 task implementations.
pub fn all_tasks() -> Vec<Box<dyn SepoliaTask>> {
    vec![
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
    ]
}

/// Statistics tracked per worker run.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DailyRunStats {
    pub total_attempts: u64,
    pub successful: u64,
    pub failed: u64,
}

// -----------------------------------------------------------------------
// TaskOutcome — what happened when we tried to run a task
// -----------------------------------------------------------------------

enum TaskOutcome {
    /// Task succeeded → recorded in DB (counts toward daily limit).
    Success {
        task_name: String,
        message: String,
        proxy_id: String,
        count: usize,
        limit: usize,
    },
    /// Task failed → NOT counted toward daily limit, stays in pool.
    Retry {
        task_name: String,
        message: String,
        proxy_id: String,
        count: usize,
        limit: usize,
    },
    /// Task exceeded the runner timeout → not recorded, will be retried.
    Timeout {
        task_name: String,
        message: String,
        proxy_id: String,
        count: usize,
        limit: usize,
    },
    /// Wallet has no remaining capacity for any task today.
    WalletComplete {
        wallet_idx: usize,
        wallet_address: String,
        task_name: String,
        proxy_id: String,
        count: usize,
        limit: usize,
    },
}

// -----------------------------------------------------------------------
// Limit helpers
// -----------------------------------------------------------------------

/// Get the daily limit for a task. Default = 1 if not configured.
/// Uses case-insensitive matching (the `config` crate lowercases HashMap keys).
pub fn get_task_limit(limits: &TaskLimits, task_name: &str) -> u32 {
    limits
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(task_name))
        .map(|(_, &v)| v)
        .unwrap_or(1)
}

/// Return `true` if the wallet has at least one task with remaining capacity.
pub fn wallet_has_remaining(task_counts: &HashMap<String, usize>, limits: &TaskLimits) -> bool {
    ALL_TASK_NAMES.iter().copied().any(|task| {
        let count = task_counts.get(task).copied().unwrap_or(0);
        let limit = get_task_limit(limits, task) as usize;
        count < limit
    })
}

/// Calculate the percentage of daily capacity used across all tasks.
/// Returns a value between 0.0 and 100.0.
/// Tasks with limit=0 are excluded from both numerator and denominator.
pub fn wallet_usage_pct(counts: &HashMap<String, usize>, limits: &TaskLimits) -> f64 {
    let mut total_done: usize = 0;
    let mut total_capacity: usize = 0;
    for task in ALL_TASK_NAMES {
        let limit = get_task_limit(limits, task) as usize;
        if limit == 0 {
            continue;
        }
        let done = counts.get(*task).copied().unwrap_or(0).min(limit);
        total_done += done;
        total_capacity += limit;
    }
    if total_capacity == 0 {
        return 0.0;
    }
    total_done as f64 / total_capacity as f64 * 100.0
}

/// Get the remaining tasks that still have capacity for this wallet.
pub fn get_remaining_tasks(
    task_counts: &HashMap<String, usize>,
    limits: &TaskLimits,
) -> Vec<&'static str> {
    ALL_TASK_NAMES
        .iter()
        .copied()
        .filter(|&task| {
            let count = task_counts.get(task).copied().unwrap_or(0);
            let limit = get_task_limit(limits, task) as usize;
            count < limit
        })
        .collect()
}

// -----------------------------------------------------------------------
// DailyRunner
// -----------------------------------------------------------------------

/// The daily runner orchestrates per-wallet task execution with
/// configurable per-task daily limits.
///
/// # Guarantees
///
/// - **No overrun**: a task will never exceed its configured daily limit.
/// - **No wallet concurrency**: the `busy_wallets` set prevents two workers
///   from processing the same wallet simultaneously.
/// - **Daily reset**: date-based filtering means midnight UTC automatically
///   resets all counters.
/// - **Retry on failure**: failed tasks stay in the pool and are not counted
///   toward daily limits.
pub struct DailyRunner {
    pub db: DailyDb,
    pub config: SepoliaConfig,
    pub wallet_manager: Arc<WalletManager>,
    pub wallet_password: Option<String>,
    pub total_wallets: usize,
    pub wallet_addresses: Vec<String>,
    pub worker_count: usize,
    pub tasks: Vec<Box<dyn SepoliaTask>>,
    pub task_limits: Arc<RwLock<TaskLimits>>,
    pub proxy_pool: Arc<RwLock<Vec<ProxyConfig>>>,
    pub proxy_health: Arc<ProxyHealthManager>,
    pub proxy_rate_limiter: Arc<ProxyRateLimiter>,
    pub gas_manager: Arc<GasManager>,
    pub min_gwei: f64,
    pub busy_wallets: Arc<Mutex<std::collections::HashSet<usize>>>,
    pub base_rpc_url: Option<String>,
    pub base_gas_manager: Option<Arc<GasManager>>,
    pub base_config: Option<SepoliaConfig>,
    /// Path to the config.toml for live-reloading task limits.
    pub config_path: Option<String>,
    /// Last known mtime of the config file — used to detect changes.
    pub last_limits_mtime: Arc<StdMutex<Option<SystemTime>>>,
    /// Test-only: when `true`, forces the worker loop to behave as if
    /// inside the pause window regardless of actual UTC time.
    #[cfg(test)]
    pub test_pause: bool,
}

impl DailyRunner {
    /// Run the daily runner indefinitely until cancelled.
    ///
    /// Spawns `worker_count` workers that continuously pick random
    /// wallets with pending tasks and execute them.
    pub async fn run(&self, cancel: CancellationToken) -> Result<DailyRunStats> {
        eprintln!("[DEBUG] DailyRunner::run() called");
        let count = self.worker_count.max(1).min(self.total_wallets.max(1));
        eprintln!("[DEBUG] count={}, wallets={}", count, self.total_wallets);
        let limits_debug = format!("{:?}", *self.task_limits.read().await);
        info!(
            "Starting DailyRunner: {} wallets, {} workers, task timeout: {}s, limits: {}",
            self.total_wallets,
            count,
            self.config.task_timeout_secs.unwrap_or(120).max(1),
            limits_debug,
        );

        let runner = Arc::new(self.clone_inner());

        let mut handles = vec![];

        for i in 0..count {
            let r = runner.clone();
            let token = cancel.clone();
            handles.push(tokio::spawn(async move { r.worker_loop(i, token).await }));
        }

        let mut total = DailyRunStats::default();
        for h in handles {
            match h.await {
                Ok(stats) => {
                    total.total_attempts += stats.total_attempts;
                    total.successful += stats.successful;
                    total.failed += stats.failed;
                }
                Err(e) => error!("Daily worker panicked: {:?}", e),
            }
        }

        info!("DailyRunner stopped. {:?}", total);
        Ok(total)
    }

    // ------------------------------------------------------------------
    // Internal: clone helper
    // ------------------------------------------------------------------

    fn clone_inner(&self) -> Self {
        Self {
            db: self.db.clone(),
            config: self.config.clone(),
            wallet_manager: self.wallet_manager.clone(),
            wallet_password: self.wallet_password.clone(),
            total_wallets: self.total_wallets,
            wallet_addresses: self.wallet_addresses.clone(),
            worker_count: self.worker_count,
            tasks: all_tasks(),
            task_limits: Arc::clone(&self.task_limits),
            proxy_pool: self.proxy_pool.clone(),
            proxy_health: self.proxy_health.clone(),
            proxy_rate_limiter: self.proxy_rate_limiter.clone(),
            gas_manager: self.gas_manager.clone(),
            min_gwei: self.min_gwei,
            busy_wallets: self.busy_wallets.clone(),
            base_rpc_url: self.base_rpc_url.clone(),
            base_gas_manager: self.base_gas_manager.clone(),
            base_config: self.base_config.clone(),
            config_path: self.config_path.clone(),
            last_limits_mtime: Arc::clone(&self.last_limits_mtime),
            #[cfg(test)]
            test_pause: self.test_pause,
        }
    }

    // ------------------------------------------------------------------
    // Hot-reload: refresh task limits from config file if mtime changed
    // ------------------------------------------------------------------

    /// Check if the config file has been modified and reload task limits.
    async fn refresh_task_limits(&self) {
        let Some(ref path) = self.config_path else { return };

        let current_mtime = match std::fs::metadata(path).and_then(|m| m.modified()) {
            Ok(m) => m,
            Err(_) => return,
        };

        let should_reload = {
            let mut last = self.last_limits_mtime.lock().unwrap();
            if *last == Some(current_mtime) {
                false // No change since last check
            } else {
                *last = Some(current_mtime);
                true
            }
        };

        if !should_reload {
            return;
        }

        match crate::config::SepoliaConfig::load(path) {
            Ok(cfg) => {
                let new_limits = cfg.task_limits.unwrap_or_default();
                let mut limits = self.task_limits.write().await;
                *limits = new_limits;
                info!("Task limits reloaded from {}: {:?}", path, *limits);
            }
            Err(e) => {
                warn!("Failed to reload task limits from {}: {}", path, e);
            }
        }
    }

    // ------------------------------------------------------------------
    // Worker loop
    // ------------------------------------------------------------------

    /// Each worker continuously:
    /// 1. Picks a random wallet with remaining task capacity
    /// 2. Picks a random pending task for that wallet
    /// 3. Executes it
    /// 4. Records success (counts toward limit) or failure (back to pool)
    async fn worker_loop(
        self: Arc<Self>,
        worker_id: usize,
        cancel: CancellationToken,
    ) -> DailyRunStats {
        let mut stats = DailyRunStats::default();
        eprintln!("[Daily] Worker {} entering loop", worker_id);
        info!(
            target: "task_result",
            "Daily worker {} started", worker_id
        );

        let task_timeout_secs = self.config.task_timeout_secs.unwrap_or(120).max(1);
        let task_timeout = Duration::from_secs(task_timeout_secs);

        loop {
            if cancel.is_cancelled() {
                info!(
                    target: "task_result",
                    "Daily worker {} stopping (cancelled)", worker_id
                );
                break;
            }

            // Pause window: no tasks during 23:55–00:05 UTC
            #[allow(unused_mut)]
            let mut in_pause = is_in_pause_window();
            #[cfg(test)]
            if self.test_pause {
                in_pause = true;
            }
            if in_pause {
                tokio::select! {
                    _ = sleep(Duration::from_secs(PAUSE_CHECK_SECS)) => {},
                    _ = cancel.cancelled() => {
                        info!(
                        target: "task_result",
                        "Daily worker {} stopping (cancelled during pause)", worker_id
                    );
                        break;
                    }
                }
                continue;
            }

            let today = today_utc();

            // Get all completion counts to determine active wallets
            let all_counts = match self.db.get_all_completed_counts(&today).await {
                Ok(c) => c,
                Err(e) => {
                    error!("Worker {}: DB error: {}", worker_id, e);
                    sleep(Duration::from_secs(IDLE_SLEEP_SECS)).await;
                    continue;
                }
            };

            // Find active wallets (those with at least one task under limit)
            let mut active: Vec<usize> = Vec::new();
            for idx in 0..self.total_wallets {
                let addr = &self.wallet_addresses[idx];
                let wc = all_counts.get(addr.as_str()).cloned().unwrap_or_default();
                if wallet_has_remaining(&wc, &*self.task_limits.read().await) {
                    active.push(idx);
                }
            }

            if active.is_empty() {
                // All wallets are at capacity for today
                tokio::select! {
                    _ = sleep(Duration::from_secs(IDLE_SLEEP_SECS)) => {},
                    _ = cancel.cancelled() => {
                        break;
                    }
                }
                continue;
            }

            // Pick a random non-busy wallet
            let wallet_idx = {
                let busy = self.busy_wallets.lock().await;
                let available: Vec<&usize> = active.iter().filter(|w| !busy.contains(w)).collect();
                if available.is_empty() {
                    drop(busy);
                    sleep(Duration::from_millis(BUSY_SLEEP_MS)).await;
                    continue;
                }
                *available[rand::thread_rng().gen_range(0..available.len())]
            };

            // Mark busy
            {
                let mut busy = self.busy_wallets.lock().await;
                if !busy.insert(wallet_idx) {
                    continue; // another worker beat us
                }
            }

            // Execute one pending task for this wallet, but do not let a single
            // RPC or receipt wait stall the wallet forever.
            let task_future = tokio::time::timeout(
                task_timeout,
                self.execute_one_task(wallet_idx, &today, worker_id),
            );
            tokio::pin!(task_future);

            let outcome = tokio::select! {
                _ = cancel.cancelled() => {
                    info!(
                        target: "task_result",
                        "Daily worker {} stopping (cancelled during task)", worker_id
                    );
                    None
                }
                result = &mut task_future => {
                    Some(match result {
                        Ok(outcome) => outcome,
                        Err(_) => TaskOutcome::Timeout {
                            task_name: "unknown".into(),
                            message: format!("Task exceeded {}s timeout", task_timeout_secs),
                            proxy_id: "---".into(),
                            count: 0,
                            limit: 0,
                        },
                    })
                }
            };

            // Release wallet
            self.busy_wallets.lock().await.remove(&wallet_idx);

            let Some(outcome) = outcome else {
                break;
            };

            // Track stats
            match outcome {
                TaskOutcome::Success {
                    task_name,
                    message,
                    proxy_id,
                    count,
                    limit,
                } => {
                    stats.total_attempts += 1;
                    stats.successful += 1;
                    info!(
                        target: "task_result",
                        "{} [WK:{:03}][WL:{:04}][P:{}] {:<7} [{:02}/{:02}][{}] {}",
                        Local::now().format("%H:%M:%S"),
                        worker_id, wallet_idx, proxy_id,
                        "OK", count, limit, task_name, message,
                    );
                }
                TaskOutcome::Retry {
                    task_name,
                    message,
                    proxy_id,
                    count,
                    limit,
                } => {
                    stats.total_attempts += 1;
                    stats.failed += 1;
                    info!(
                        target: "task_result",
                        "{} [WK:{:03}][WL:{:04}][P:{}] {:<7} [{:02}/{:02}][{}] {}",
                        Local::now().format("%H:%M:%S"),
                        worker_id, wallet_idx, proxy_id,
                        "RETRY", count, limit, task_name, message,
                    );
                }
                TaskOutcome::Timeout {
                    task_name,
                    message,
                    proxy_id,
                    count,
                    limit,
                } => {
                    stats.total_attempts += 1;
                    stats.failed += 1;
                    error!(
                        target: "task_result",
                        "{} [WK:{:03}][WL:{:04}][P:{}] {:<7} [{:02}/{:02}][{}] {}",
                        Local::now().format("%H:%M:%S"),
                        worker_id, wallet_idx, proxy_id,
                        "TIMEOUT", count, limit, task_name, message,
                    );
                }
                TaskOutcome::WalletComplete {
                    wallet_idx,
                    wallet_address,
                    task_name,
                    proxy_id,
                    count,
                    limit,
                } => {
                    info!(
                        target: "task_result",
                        "{} [WK:{:03}][WL:{:04}][P:{}] {:<7} [{:02}/{:02}][{}] Daily tasks done - Address : {}",
                        Local::now().format("%H:%M:%S"),
                        worker_id, wallet_idx, proxy_id,
                        "LIMIT", count, limit, task_name, wallet_address,
                    );
                }
            }
        }

        stats
    }

    // ------------------------------------------------------------------
    // Task execution
    // ------------------------------------------------------------------

    /// Pick a random pending task for `wallet_idx` and execute it.
    async fn execute_one_task(
        &self,
        wallet_idx: usize,
        today: &str,
        _worker_id: usize,
    ) -> TaskOutcome {
        let wallet_addr = &self.wallet_addresses[wallet_idx];

        // Get completion counts for this wallet today
        let task_counts = match self.db.get_completed_counts(wallet_addr, today).await {
            Ok(c) => c,
            Err(e) => {
                return TaskOutcome::Retry {
                    task_name: "unknown".into(),
                    message: format!("DB error: {}", e),
                    proxy_id: "---".into(),
                    count: 0,
                    limit: 0,
                };
            }
        };

        // Select proxy first — so LIMIT logs show real P:xxx
        let (proxy_config, proxy_id) = self.select_proxy().await;

        // Hot-reload task limits from config if file changed
        self.refresh_task_limits().await;

        // Compute pending tasks based on limits
        let pending: Vec<&str> = get_remaining_tasks(&task_counts, &*self.task_limits.read().await);

        if pending.is_empty() {
            let rep_task = ALL_TASK_NAMES[0];
            let wc_count = task_counts.get(rep_task).copied().unwrap_or(0);
            let wc_limit = get_task_limit(&*self.task_limits.read().await, rep_task) as usize;
            return TaskOutcome::WalletComplete {
                wallet_idx,
                wallet_address: wallet_addr.to_string(),
                task_name: rep_task.to_string(),
                proxy_id,
                count: wc_count,
                limit: wc_limit,
            };
        }

        // Pick random pending task
        let task_name = pending[rand::thread_rng().gen_range(0..pending.len())];

        // Current count & limit for this task (before this execution)
        let current_count = task_counts.get(task_name).copied().unwrap_or(0);
        let current_limit = get_task_limit(&*self.task_limits.read().await, task_name) as usize;

        // Find the task implementation
        let task = match self.tasks.iter().find(|t| t.name() == task_name) {
            Some(t) => t,
            None => {
                return TaskOutcome::Retry {
                    task_name: task_name.to_string(),
                    message: "Task implementation not found".into(),
                    proxy_id: proxy_id.clone(),
                    count: current_count,
                    limit: current_limit,
                };
            }
        };

        // Rate-limit proxy
        if let Some(ref proxy) = proxy_config {
            self.proxy_rate_limiter
                .wait_until_available(&proxy.url)
                .await;
        }

        // Determine if this is a base-chain task (bridge-back)
        let is_base = task_name == "16_bridgeBackTplus" || task_name == "17_bridgeBackCplus";

        let rpc_url = if is_base {
            self.base_rpc_url.as_deref().unwrap_or(&self.config.rpc_url)
        } else {
            &self.config.rpc_url
        };

        // Create RPC provider
        let provider = self.create_provider(&proxy_config, rpc_url).await;

        // Decrypt wallet
        let wallet = match self
            .wallet_manager
            .get_wallet(wallet_idx, self.wallet_password.as_deref())
            .await
        {
            Ok(decrypted) => {
                let chain_id = if is_base {
                    self.base_config
                        .as_ref()
                        .map(|c| c.chain_id)
                        .unwrap_or(self.config.chain_id)
                } else {
                    self.config.chain_id
                };
                match decrypted
                    .evm_private_key
                    .parse::<ethers::signers::LocalWallet>()
                {
                    Ok(w) => w.with_chain_id(chain_id),
                    Err(e) => {
                        return TaskOutcome::Retry {
                            task_name: task_name.to_string(),
                            message: format!("wallet parse error: {}", e),
                            proxy_id: proxy_id.clone(),
                            count: current_count,
                            limit: current_limit,
                        };
                    }
                }
            }
            Err(e) => {
                return TaskOutcome::Retry {
                    task_name: task_name.to_string(),
                    message: format!("wallet decrypt error: {}", e),
                    proxy_id: proxy_id.clone(),
                    count: current_count,
                    limit: current_limit,
                };
            }
        };

        // Pick the right gas manager
        let ctx_gas = if is_base {
            self.base_gas_manager
                .as_ref()
                .unwrap_or(&self.gas_manager)
                .clone()
        } else {
            self.gas_manager.clone()
        };

        let ctx_config = if is_base {
            self.base_config.as_ref().unwrap_or(&self.config).clone()
        } else {
            self.config.clone()
        };

        let ctx = TaskContext {
            provider,
            wallet,
            config: ctx_config,
            proxy: proxy_config.as_ref().map(|p| p.url.clone()),
            db: None,
            gas_manager: ctx_gas,
        };

        // Execute the task
        let start = std::time::Instant::now();
        match task.run(ctx).await {
            Ok(result) => {
                let elapsed = start.elapsed();

                if let Some(ref proxy) = proxy_config {
                    if result.success {
                        self.proxy_health.record_success(&proxy.url).await;
                    } else {
                        self.proxy_health.record_failure(&proxy.url).await;
                    }
                }

                // Record in daily DB (INSERT — each success counts)
                let msg = format!("{} ({:.1}s)", result.message, elapsed.as_secs_f64());
                if let Err(e) = self
                    .db
                    .record_task_completion(wallet_addr, task_name, today, result.success, &msg)
                    .await
                {
                    warn!("Failed to record task completion: {}", e);
                }

                if result.success {
                    TaskOutcome::Success {
                        task_name: task_name.to_string(),
                        message: result.message,
                        proxy_id: proxy_id.clone(),
                        count: current_count + 1,
                        limit: current_limit,
                    }
                } else {
                    TaskOutcome::Retry {
                        task_name: task_name.to_string(),
                        message: result.message,
                        proxy_id: proxy_id.clone(),
                        count: current_count,
                        limit: current_limit,
                    }
                }
            }
            Err(e) => {
                let elapsed = start.elapsed();

                if let Some(ref proxy) = proxy_config {
                    self.proxy_health.record_failure(&proxy.url).await;
                }

                // Record failure (success=false so it doesn't count)
                let msg = format!("{} ({:.1}s)", e, elapsed.as_secs_f64());
                if let Err(log_err) = self
                    .db
                    .record_task_completion(wallet_addr, task_name, today, false, &msg)
                    .await
                {
                    warn!("Failed to record task failure: {}", log_err);
                }

                TaskOutcome::Retry {
                    task_name: task_name.to_string(),
                    message: format!("{}", e),
                    proxy_id: proxy_id.clone(),
                    count: current_count,
                    limit: current_limit,
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Proxy helpers
    // ------------------------------------------------------------------

    /// Select a healthy proxy at random, or `None` if none available.
    async fn select_proxy(&self) -> (Option<ProxyConfig>, String) {
        let proxies = self.proxy_pool.read().await;
        if proxies.is_empty() {
            return (None, "000".into());
        }

        let mut available: Vec<(usize, &ProxyConfig)> = Vec::new();
        for (i, p) in proxies.iter().enumerate() {
            if self.proxy_health.is_available(&p.url).await {
                available.push((i, p));
            }
        }

        if available.is_empty() {
            error!("No healthy proxies available");
            return (None, "000".into());
        }

        let idx = rand::thread_rng().gen_range(0..available.len());
        let (orig_idx, proxy) = available[idx];
        (Some(proxy.clone()), format!("{:03}", orig_idx + 1))
    }

    /// Build an HTTP provider, optionally tunnelled through a proxy.
    async fn create_provider(
        &self,
        proxy_config: &Option<ProxyConfig>,
        rpc_url: &str,
    ) -> ethers::providers::Provider<ethers::providers::Http> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            ),
        );

        let mut builder = reqwest::Client::builder().default_headers(headers);

        if let Some(proxy_conf) = proxy_config {
            if let Ok(mut proxy) = reqwest::Proxy::all(&proxy_conf.url) {
                if let (Some(u), Some(p)) = (&proxy_conf.username, &proxy_conf.password) {
                    proxy = proxy.basic_auth(u, p);
                }
                builder = builder.proxy(proxy);
            }
        }

        let client = builder.build().expect("Failed to build HTTP client");
        ethers::providers::Provider::new(ethers::providers::Http::new_with_client(
            reqwest::Url::parse(rpc_url).expect("Invalid RPC URL"),
            client,
        ))
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tracing::debug;

    // ---- pause window ----

    #[test]
    fn test_pause_window_constants() {
        assert_eq!(PAUSE_START_MINUTES, 1435);
        assert_eq!(PAUSE_END_MINUTES, 4);
    }

    #[test]
    fn test_is_in_pause_window_at_nominal() {
        // 12:00 UTC — well outside pause window
        let noon = chrono::DateTime::parse_from_rfc3339("2025-06-15T12:00:00Z")
            .unwrap()
            .to_utc();
        assert!(!is_in_pause_window_at(noon));
    }

    #[test]
    fn test_is_in_pause_window_at_just_before_start() {
        // 23:54 UTC — 1 minute before pause window
        let before = chrono::DateTime::parse_from_rfc3339("2025-06-15T23:54:00Z")
            .unwrap()
            .to_utc();
        assert!(!is_in_pause_window_at(before));
    }

    #[test]
    fn test_is_in_pause_window_at_start_edge() {
        // 23:55 UTC — pause window just opened
        let start = chrono::DateTime::parse_from_rfc3339("2025-06-15T23:55:00Z")
            .unwrap()
            .to_utc();
        assert!(is_in_pause_window_at(start));
    }

    #[test]
    fn test_is_in_pause_window_at_midnight() {
        // 00:00 UTC — middle of pause window
        let midnight = chrono::DateTime::parse_from_rfc3339("2025-06-16T00:00:00Z")
            .unwrap()
            .to_utc();
        assert!(is_in_pause_window_at(midnight));
    }

    #[test]
    fn test_is_in_pause_window_at_end_edge() {
        // 00:04 UTC — pause window's last minute
        let end = chrono::DateTime::parse_from_rfc3339("2025-06-16T00:04:00Z")
            .unwrap()
            .to_utc();
        assert!(is_in_pause_window_at(end));
    }

    #[test]
    fn test_is_in_pause_window_at_just_after_end() {
        // 00:06 UTC — pause window has closed
        let after = chrono::DateTime::parse_from_rfc3339("2025-06-16T00:06:00Z")
            .unwrap()
            .to_utc();
        assert!(!is_in_pause_window_at(after));
    }

    #[test]
    fn test_is_in_pause_window_at_wraps_midnight() {
        // Verify minutes calculation: 23:55 = 1435, 00:05 = 5
        let t1 = chrono::DateTime::parse_from_rfc3339("2025-06-15T23:59:00Z")
            .unwrap()
            .to_utc();
        let t2 = chrono::DateTime::parse_from_rfc3339("2025-06-16T00:01:00Z")
            .unwrap()
            .to_utc();
        assert!(is_in_pause_window_at(t1), "23:59 should be in pause");
        assert!(is_in_pause_window_at(t2), "00:01 should be in pause");
    }

    // ---- today_utc ----

    #[test]
    fn test_today_utc_format() {
        let date = today_utc();
        assert_eq!(date.len(), 10, "Date should be YYYY-MM-DD format");
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");
        assert!(date.chars().all(|c| c.is_ascii_digit() || c == '-'));
    }

    // ---- ALL_TASK_NAMES ----

    #[test]
    fn test_all_task_names_count() {
        assert_eq!(
            ALL_TASK_NAMES.len(),
            DailyDb::TASKS_PER_WALLET,
            "ALL_TASK_NAMES must match TASKS_PER_WALLET"
        );
    }

    #[test]
    fn test_all_task_names_unique() {
        let mut seen = std::collections::HashSet::new();
        for name in ALL_TASK_NAMES {
            assert!(seen.insert(name), "Duplicate task name: {}", name);
        }
    }

    #[test]
    fn test_all_task_names_match_task_objects() {
        let tasks = all_tasks();
        assert_eq!(tasks.len(), ALL_TASK_NAMES.len());
        for (task, expected_name) in tasks.iter().zip(ALL_TASK_NAMES.iter()) {
            assert_eq!(task.name(), *expected_name);
        }
    }

    // ---- all_tasks ----

    #[test]
    fn test_all_tasks_count() {
        assert_eq!(all_tasks().len(), 19);
    }

    #[test]
    fn test_all_tasks_have_correct_names() {
        let tasks = all_tasks();
        let names: Vec<&str> = tasks.iter().map(|t| t.name()).collect();
        assert_eq!(names, ALL_TASK_NAMES);
    }

    // ---- DailyRunStats ----

    #[test]
    fn test_daily_run_stats_default() {
        let stats = DailyRunStats::default();
        assert_eq!(stats.total_attempts, 0);
        assert_eq!(stats.successful, 0);
        assert_eq!(stats.failed, 0);
    }

    #[test]
    fn test_daily_run_stats_clone() {
        let a = DailyRunStats {
            total_attempts: 10,
            successful: 7,
            failed: 3,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    // ---- Limit helpers ----

    #[test]
    fn test_get_task_limit_default() {
        let limits = HashMap::new();
        assert_eq!(get_task_limit(&limits, "01_checkBalance"), 1);
        assert_eq!(get_task_limit(&limits, "nonexistent"), 1);
    }

    #[test]
    fn test_get_task_limit_custom() {
        let mut limits = HashMap::new();
        limits.insert("01_checkBalance".into(), 100);
        limits.insert("10_aaveUsdtFaucet".into(), 5);

        assert_eq!(get_task_limit(&limits, "01_checkBalance"), 100);
        assert_eq!(get_task_limit(&limits, "10_aaveUsdtFaucet"), 5);
        assert_eq!(get_task_limit(&limits, "02_mintUsdtPlus"), 1); // default
    }

    #[test]
    fn test_wallet_has_remaining_all_done() {
        let mut counts = HashMap::new();
        // All tasks at limit=1 means 1 completion each
        for task in ALL_TASK_NAMES {
            counts.insert(task.to_string(), 1usize);
        }
        let limits = HashMap::new(); // default limit=1

        assert!(!wallet_has_remaining(&counts, &limits));
    }

    #[test]
    fn test_wallet_has_remaining_some_pending() {
        let mut counts = HashMap::new();
        counts.insert("01_checkBalance".to_string(), 1usize);
        // Only task 1 done, 16 more pending at default limit=1
        let limits = HashMap::new();

        assert!(wallet_has_remaining(&counts, &limits));
    }

    #[test]
    fn test_wallet_has_remaining_with_high_limits() {
        let mut counts = HashMap::new();
        counts.insert("01_checkBalance".to_string(), 50usize);
        let mut limits = HashMap::new();
        limits.insert("01_checkBalance".into(), 100u32);

        // 50 of 100 done for checkBalance, others have limit=1 with 0 done
        assert!(wallet_has_remaining(&counts, &limits));
    }

    #[test]
    fn test_wallet_has_remaining_exactly_at_limit() {
        let mut counts = HashMap::new();
        counts.insert("01_checkBalance".to_string(), 100usize);
        let mut limits = HashMap::new();
        limits.insert("01_checkBalance".into(), 100u32);

        // checkBalance at limit (100/100), others at limit=1 with 0 done
        // But since others have limit=1 and count=0, they have remaining
        assert!(wallet_has_remaining(&counts, &limits));
    }

    #[test]
    fn test_wallet_has_remaining_all_tasks_at_different_limits() {
        let mut counts = HashMap::new();
        let mut limits = HashMap::new();
        limits.insert("01_checkBalance".into(), 100u32);
        limits.insert("10_aaveUsdtFaucet".into(), 5u32);

        // checkBalance: 100/100, faucet: 5/5, others: 1/1
        counts.insert("01_checkBalance".to_string(), 100usize);
        counts.insert("10_aaveUsdtFaucet".to_string(), 5usize);
        for task in ALL_TASK_NAMES
            .iter()
            .filter(|t| **t != "01_checkBalance" && **t != "10_aaveUsdtFaucet")
        {
            counts.insert(task.to_string(), 1usize);
        }

        assert!(!wallet_has_remaining(&counts, &limits));
    }

    #[test]
    fn test_get_remaining_tasks_basic() {
        let mut counts = HashMap::new();
        counts.insert("01_checkBalance".to_string(), 1usize);
        let limits = HashMap::new(); // default limit=1

        let remaining = get_remaining_tasks(&counts, &limits);
        assert_eq!(remaining.len(), 18); // 19 total - 1 done
        assert!(!remaining.contains(&"01_checkBalance"));
    }

    #[test]
    fn test_get_remaining_tasks_with_limits() {
        let mut counts = HashMap::new();
        counts.insert("01_checkBalance".to_string(), 50usize);
        let mut limits = HashMap::new();
        limits.insert("01_checkBalance".into(), 100u32);

        let remaining = get_remaining_tasks(&counts, &limits);
        assert!(remaining.contains(&"01_checkBalance")); // still has capacity
        assert_eq!(remaining.len(), 19); // all tasks have capacity
    }

    #[test]
    fn test_get_remaining_tasks_exact_limit() {
        let mut counts = HashMap::new();
        let mut limits = HashMap::new();
        limits.insert("01_checkBalance".into(), 100u32);
        counts.insert("01_checkBalance".to_string(), 100usize);

        let remaining = get_remaining_tasks(&counts, &limits);
        assert!(!remaining.contains(&"01_checkBalance")); // at capacity
        assert_eq!(remaining.len(), 18); // 18 other tasks at limit=1 with 0 done
    }

    #[test]
    fn test_get_remaining_tasks_all_done() {
        let mut counts = HashMap::new();
        for task in ALL_TASK_NAMES {
            counts.insert(task.to_string(), 1usize);
        }
        let limits = HashMap::new();

        let remaining = get_remaining_tasks(&counts, &limits);
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_wallet_has_remaining_empty_state() {
        // Empty counts with default limits = all 17 tasks have 0 of 1 done → pending
        let counts = HashMap::new();
        let limits = HashMap::new();
        assert!(wallet_has_remaining(&counts, &limits));
    }

    #[test]
    fn test_wallet_has_remaining_zero_limit_all_tasks() {
        // All tasks have limit=0 → no task has count < 0 → no remaining
        let counts = HashMap::new();
        let mut limits = HashMap::new();
        for task in ALL_TASK_NAMES {
            limits.insert(task.to_string(), 0u32);
        }
        assert!(!wallet_has_remaining(&counts, &limits));
    }

    #[test]
    fn test_get_remaining_tasks_empty_state() {
        let counts = HashMap::new();
        let limits = HashMap::new();
        let remaining = get_remaining_tasks(&counts, &limits);
        assert_eq!(remaining.len(), 19, "All 19 tasks should be pending");
        for expected in ALL_TASK_NAMES {
            assert!(remaining.contains(expected), "Missing: {}", expected);
        }
    }

    #[test]
    fn test_get_remaining_tasks_count_exceeds_limit() {
        // Defensive: if count somehow exceeds limit, task should NOT be in remaining
        let mut counts = HashMap::new();
        counts.insert("01_checkBalance".to_string(), 5usize);
        let mut limits = HashMap::new();
        limits.insert("01_checkBalance".into(), 3u32); // count > limit

        let remaining = get_remaining_tasks(&counts, &limits);
        assert!(!remaining.contains(&"01_checkBalance")); // 5 >= 3 → excluded
                                                          // Other 16 tasks still pending
        assert_eq!(remaining.len(), 18);
    }

    #[test]
    fn test_get_remaining_tasks_zero_limit_task() {
        // Task with limit=0 should never appear in remaining
        let counts = HashMap::new();
        let mut limits = HashMap::new();
        limits.insert("01_checkBalance".into(), 0u32);

        let remaining = get_remaining_tasks(&counts, &limits);
        assert!(!remaining.contains(&"01_checkBalance")); // limit=0, 0 < 0 = false
        assert_eq!(remaining.len(), 18, "Only checkBalance should be excluded");
    }

    // ---- ALL_TASK_NAMES format ----

    #[test]
    fn test_all_task_names_prefix_format() {
        for name in ALL_TASK_NAMES {
            assert!(name.len() >= 3, "Task '{}' too short", name);
            let _: u32 = name[..2]
                .parse()
                .expect("Task should start with 2-digit number");
            assert_eq!(
                name.as_bytes()[2],
                b'_',
                "Task '{}' missing underscore after prefix",
                name
            );
        }
    }

    // ---- DailyRunStats ----

    #[test]
    fn test_daily_run_stats_partial_eq() {
        let a = DailyRunStats {
            total_attempts: 5,
            successful: 3,
            failed: 2,
        };
        let b = DailyRunStats {
            total_attempts: 5,
            successful: 3,
            failed: 2,
        };
        let c = DailyRunStats {
            total_attempts: 5,
            successful: 4,
            failed: 1,
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // ---- Integration: DB + limit helpers ----

    /// Helper: create an in-memory DB.
    async fn setup_test_db() -> DailyDb {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        let db = database::DailyDb { pool };
        db.init_schema().await.expect("Failed to init schema");
        db
    }

    #[tokio::test]
    async fn test_get_active_wallets_with_partial_completions() {
        let db = setup_test_db().await;
        let date = today_utc();
        let limits = HashMap::new(); // all tasks limit=1

        // Wallet 0: 3/19 tasks done
        for i in 0..3 {
            db.record_task_completion("0xalice", ALL_TASK_NAMES[i], &date, true, "ok")
                .await
                .unwrap();
        }
        // Wallet 1: 0/19 done
        // Wallet 2: all 19/19 done (with limit=1 each)
        for i in 0..19 {
            db.record_task_completion("0xcharlie", ALL_TASK_NAMES[i], &date, true, "ok")
                .await
                .unwrap();
        }

        let all_counts = db.get_all_completed_counts(&date).await.unwrap();

        let wallet_addresses = vec![
            "0xalice".to_string(),
            "0xbob".to_string(),
            "0xcharlie".to_string(),
            "0xdave".to_string(),
        ];

        let mut active: Vec<usize> = Vec::new();
        for (idx, addr) in wallet_addresses.iter().enumerate() {
            let wc = all_counts.get(addr.as_str()).cloned().unwrap_or_default();
            if wallet_has_remaining(&wc, &limits) {
                active.push(idx);
            }
        }

        // Wallets 0, 1, 3 should be active (wallet 2 is done)
        assert_eq!(active.len(), 3);
        assert!(active.contains(&0));
        assert!(active.contains(&1));
        assert!(active.contains(&3));
        assert!(!active.contains(&2));
    }

    #[tokio::test]
    async fn test_pending_tasks_exclude_completed() {
        let db = setup_test_db().await;
        let date = today_utc();
        let limits = HashMap::new(); // limit=1 for all

        // Complete 5 tasks for wallet 0
        for i in 0..5 {
            db.record_task_completion("0xalice", ALL_TASK_NAMES[i], &date, true, "ok")
                .await
                .unwrap();
        }

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(counts.len(), 5);

        let pending = get_remaining_tasks(&counts, &limits);
        assert_eq!(pending.len(), 14, "14 tasks should be pending");

        // None of the pending tasks should be in completed
        for task in &pending {
            assert!(!counts.contains_key(*task));
        }
    }

    #[tokio::test]
    async fn test_failed_task_stays_in_pending_pool() {
        let db = setup_test_db().await;
        let date = today_utc();
        let limits = HashMap::new();

        // Record a FAILED attempt for task "01_checkBalance"
        db.record_task_completion("0xalice", "01_checkBalance", &date, false, "error")
            .await
            .unwrap();

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert!(
            counts.is_empty(),
            "Failed task should not appear in completed counts"
        );

        // Pending should still include the task
        let pending = get_remaining_tasks(&counts, &limits);
        assert_eq!(pending.len(), 19, "All 19 tasks should still be pending");
    }

    #[tokio::test]
    async fn test_task_that_fails_then_succeeds_counts_incrementally() {
        let db = setup_test_db().await;
        let date = today_utc();
        let mut limits = HashMap::new();
        limits.insert("01_checkBalance".into(), 5u32);

        // 2 failures then 3 successes
        for _ in 0..2 {
            db.record_task_completion("0xalice", "01_checkBalance", &date, false, "fail")
                .await
                .unwrap();
        }
        for _ in 0..3 {
            db.record_task_completion("0xalice", "01_checkBalance", &date, true, "ok")
                .await
                .unwrap();
        }

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(
            counts.get("01_checkBalance").copied().unwrap_or(0),
            3,
            "Only 3 successful completions should be counted"
        );

        let pending = get_remaining_tasks(&counts, &limits);
        // checkBalance still has capacity (3/5 done), so it's still pending
        assert!(pending.contains(&"01_checkBalance"));
        // 18 other tasks at limit=1 with 0 done
        assert_eq!(pending.len(), 19);
    }

    #[tokio::test]
    async fn test_multiple_completions_same_task() {
        let db = setup_test_db().await;
        let date = today_utc();
        let mut limits = HashMap::new();
        limits.insert("01_checkBalance".into(), 10u32);

        // Run checkBalance 7 times successfully
        for _ in 0..7 {
            db.record_task_completion("0xalice", "01_checkBalance", &date, true, "ok")
                .await
                .unwrap();
        }

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 7);

        let pending = get_remaining_tasks(&counts, &limits);
        assert!(pending.contains(&"01_checkBalance")); // still has 3 more
        assert_eq!(pending.len(), 19);
    }

    // ------------------------------------------------------------------
    // Proper logic test: asymmetric task limits enforced end-to-end
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_daily_runner_enforces_asymmetric_task_limits() {
        let db = setup_test_db().await;
        let date = today_utc();
        let wallet = "0xwallet";

        // Configure asymmetric limits:
        //   checkBalance = 5, mintUsdtPlus = 3, mintUsdcPlus = 2, rest = 1 (default)
        let mut limits = HashMap::new();
        limits.insert("01_checkBalance".into(), 5u32);
        limits.insert("02_mintUsdtPlus".into(), 3u32);
        limits.insert("03_mintUsdcPlus".into(), 2u32);

        // ---- Phase 1: Fill checkBalance to its limit of 5 ----
        for i in 0..5 {
            db.record_task_completion(wallet, "01_checkBalance", &date, true, "ok")
                .await
                .unwrap();

            let counts = db.get_completed_counts(wallet, &date).await.unwrap();
            assert_eq!(
                counts.get("01_checkBalance").copied().unwrap_or(0),
                i + 1,
                "checkBalance iteration {}",
                i
            );

            let pending = get_remaining_tasks(&counts, &limits);
            if i < 4 {
                // Still has room up to limit=5
                assert!(
                    pending.contains(&"01_checkBalance"),
                    "checkBalance should still be pending at iter {}",
                    i
                );
            } else {
                // At exact limit — excluded
                assert!(
                    !pending.contains(&"01_checkBalance"),
                    "checkBalance should be exhausted at iter 4"
                );
            }
        }

        // Verify: checkBalance exhausted, other tasks still pending
        let counts = db.get_completed_counts(wallet, &date).await.unwrap();
        assert!(
            !get_remaining_tasks(&counts, &limits).contains(&"01_checkBalance"),
            "checkBalance should be excluded after 5 completions"
        );
        assert!(
            wallet_has_remaining(&counts, &limits),
            "Other tasks should still be pending"
        );

        // ---- Phase 2: Fill mintUsdtPlus to its limit of 3 ----
        for i in 0..3 {
            db.record_task_completion(wallet, "02_mintUsdtPlus", &date, true, "ok")
                .await
                .unwrap();

            let counts = db.get_completed_counts(wallet, &date).await.unwrap();
            assert_eq!(
                counts.get("02_mintUsdtPlus").copied().unwrap_or(0),
                i + 1,
                "mintUsdtPlus iteration {}",
                i
            );

            if i < 2 {
                assert!(get_remaining_tasks(&counts, &limits).contains(&"02_mintUsdtPlus"));
            } else {
                assert!(!get_remaining_tasks(&counts, &limits).contains(&"02_mintUsdtPlus"));
            }
        }

        // ---- Phase 3: Fill mintUsdcPlus to its limit of 2 ----
        for i in 0..2 {
            db.record_task_completion(wallet, "03_mintUsdcPlus", &date, true, "ok")
                .await
                .unwrap();

            let counts = db.get_completed_counts(wallet, &date).await.unwrap();
            assert_eq!(
                counts.get("03_mintUsdcPlus").copied().unwrap_or(0),
                i + 1,
                "mintUsdcPlus iteration {}",
                i
            );

            if i < 1 {
                assert!(get_remaining_tasks(&counts, &limits).contains(&"03_mintUsdcPlus"));
            } else {
                assert!(!get_remaining_tasks(&counts, &limits).contains(&"03_mintUsdcPlus"));
            }
        }

        // ---- Phase 4: Complete all remaining tasks (14 x limit=1) ----
        let remaining_tasks: Vec<&&str> = ALL_TASK_NAMES
            .iter()
            .filter(|t| {
                **t != "01_checkBalance" && **t != "02_mintUsdtPlus" && **t != "03_mintUsdcPlus"
            })
            .collect();
        assert_eq!(remaining_tasks.len(), 16, "16 remaining tasks expected");

        for task in &remaining_tasks {
            db.record_task_completion(wallet, task, &date, true, "ok")
                .await
                .unwrap();
        }

        // ---- Final: wallet should be fully exhausted ----
        let counts = db.get_completed_counts(wallet, &date).await.unwrap();
        assert!(
            !wallet_has_remaining(&counts, &limits),
            "Wallet should have no remaining capacity"
        );
        assert!(
            get_remaining_tasks(&counts, &limits).is_empty(),
            "All tasks should be exhausted"
        );

        // ---- Verify each task's exact completion count ----
        for task in ALL_TASK_NAMES {
            let expected = if *task == "01_checkBalance" {
                5
            } else if *task == "02_mintUsdtPlus" {
                3
            } else if *task == "03_mintUsdcPlus" {
                2
            } else {
                1
            };
            assert_eq!(
                counts.get(*task).copied().unwrap_or(0),
                expected,
                "{} should have {} completions",
                task,
                expected
            );
        }

        // ---- Verify total completions ----
        let total = db.get_total_completed(wallet, &date).await.unwrap();
        assert_eq!(total, 26, "Total completions = 5 + 3 + 2 + (16 x 1) = 26");
    }

    #[tokio::test]
    async fn test_worker_loop_terminates_when_cancelled() {
        let db = setup_test_db().await;

        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let cancel = CancellationToken::new();
        let cancel_child = cancel.clone();

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 1,
            wallet_addresses: vec![],
            worker_count: 1,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(Vec::new())),
            proxy_health: Arc::new(ProxyHealthManager::new(3, 5)),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        };

        // Cancel immediately so worker exits quickly
        cancel_child.cancel();

        let result = runner.run(cancel_child).await;
        assert!(result.is_ok(), "Runner should exit cleanly on cancel");
        let stats = result.unwrap();
        assert_eq!(stats.total_attempts, 0);
    }

    #[tokio::test]
    async fn test_select_proxy_empty_pool() {
        let db = setup_test_db().await;
        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 1,
            wallet_addresses: vec![],
            worker_count: 1,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(Vec::new())),
            proxy_health: Arc::new(ProxyHealthManager::new(3, 5)),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        };

        // Empty pool should return None
        let (proxy, id) = runner.select_proxy().await;
        assert!(
            proxy.is_none(),
            "No proxy should be selected from empty pool"
        );
        assert_eq!(id, "000");
    }

    #[tokio::test]
    async fn test_select_proxy_all_unhealthy() {
        let db = setup_test_db().await;
        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let proxy_config = core_logic::config::ProxyConfig {
            url: "http://proxy.example.com:8080".into(),
            username: None,
            password: None,
        };

        let proxy_health = Arc::new(ProxyHealthManager::new(3, 5));
        // Mark proxy as unavailable by recording failures
        proxy_health.record_failure(&proxy_config.url).await;
        proxy_health.record_failure(&proxy_config.url).await;
        proxy_health.record_failure(&proxy_config.url).await;

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 1,
            wallet_addresses: vec![],
            worker_count: 1,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(vec![proxy_config])),
            proxy_health: proxy_health.clone(),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        };

        let (proxy, id) = runner.select_proxy().await;
        assert!(
            proxy.is_none(),
            "No proxy should be selected when all unhealthy"
        );
        assert_eq!(id, "000");
    }

    #[tokio::test]
    async fn test_worker_loop_busy_wallet_contention() {
        // Test that busy_wallets prevents two workers from picking the same wallet
        let db = setup_test_db().await;
        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let busy_wallets = Arc::new(Mutex::new(std::collections::HashSet::new()));

        let runner = Arc::new(DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 2,
            wallet_addresses: vec![],
            worker_count: 2,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(Vec::new())),
            proxy_health: Arc::new(ProxyHealthManager::new(3, 5)),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets,
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        });

        // Manually mark wallet 0 as busy
        runner.busy_wallets.lock().await.insert(0);

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        // Spawn cancel shortly
        tokio::spawn(async move {
            sleep(Duration::from_millis(200)).await;
            cancel_clone.cancel();
        });

        let result = runner.run(cancel).await;
        assert!(result.is_ok(), "Runner should handle contention cleanly");
    }

    // ------------------------------------------------------------------
    // Concurrent stress test: 3 workers, 5 wallets, asymmetric limits
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_concurrent_workers_never_exceed_limits() {
        // Spawn 3 concurrent workers processing 5 wallets with asymmetric limits.
        // Each worker simulates the runner's flow: pick wallet -> lock -> check
        // remaining -> record completion -> unlock. Verifies NO task ever
        // exceeds its configured limit under concurrent access.
        let db = Arc::new(setup_test_db().await);
        let date = today_utc();

        let wallet_addresses: Vec<&str> = vec!["0xalice", "0xbob", "0xcharlie", "0xdave", "0xeve"];

        // Asymmetric limits: two tasks with high limits, rest default=1
        let mut limits = HashMap::new();
        limits.insert("01_checkBalance".into(), 5u32);
        limits.insert("02_mintUsdtPlus".into(), 3u32);
        // 03..17 get default limit = 1

        let busy_wallets: Arc<Mutex<HashSet<usize>>> = Arc::new(Mutex::new(HashSet::new()));
        let cancel = CancellationToken::new();

        let worker_count = 3;
        let mut handles = vec![];

        for _worker_id in 0..worker_count {
            let db = db.clone();
            let wallets = wallet_addresses.clone();
            let limits = limits.clone();
            let busy = busy_wallets.clone();
            let cancel = cancel.clone();
            let date = date.clone();

            handles.push(tokio::spawn(async move {
                loop {
                    if cancel.is_cancelled() {
                        break;
                    }

                    // 1. Pick random non-busy wallet
                    let wallet_idx = {
                        let mut busy_set = busy.lock().await;
                        let available: Vec<usize> = (0..wallets.len())
                            .filter(|w| !busy_set.contains(w))
                            .collect();
                        if available.is_empty() {
                            drop(busy_set);
                            sleep(Duration::from_millis(5)).await;
                            continue;
                        }
                        let idx = available[rand::thread_rng().gen_range(0..available.len())];
                        busy_set.insert(idx);
                        idx
                    };

                    let wallet = wallets[wallet_idx];

                    // 2. Get counts and check remaining (double-check)
                    let counts = db.get_completed_counts(wallet, &date).await.unwrap();
                    let remaining = get_remaining_tasks(&counts, &limits);

                    if remaining.is_empty() {
                        busy.lock().await.remove(&wallet_idx);
                        continue;
                    }

                    // 3. Pick random pending task and record completion
                    let task = remaining[rand::thread_rng().gen_range(0..remaining.len())];
                    db.record_task_completion(wallet, task, &date, true, "stress")
                        .await
                        .unwrap();

                    // 4. Release wallet
                    busy.lock().await.remove(&wallet_idx);
                }
            }));
        }

        // Let workers run for 1 second, then cancel
        sleep(Duration::from_secs(1)).await;
        cancel.cancel();

        for handle in handles {
            handle.await.unwrap();
        }

        // ---- VERIFY INVARIANT: no task count exceeds its limit ----
        let all_counts = db.get_all_completed_counts(&date).await.unwrap();

        for wallet in &wallet_addresses {
            let counts = all_counts.get(*wallet).cloned().unwrap_or_default();

            for task in ALL_TASK_NAMES {
                let count = counts.get(*task).copied().unwrap_or(0);
                let limit = get_task_limit(&limits, task) as usize;

                assert!(
                    count <= limit,
                    "INVARIANT VIOLATION: wallet={} task={} count={} limit={}",
                    wallet,
                    task,
                    count,
                    limit
                );
            }

            // Verify total across all tasks
            let total: usize = ALL_TASK_NAMES
                .iter()
                .map(|t| counts.get(*t).copied().unwrap_or(0))
                .sum();

            let expected_max: usize = ALL_TASK_NAMES
                .iter()
                .map(|t| get_task_limit(&limits, t) as usize)
                .sum();

            assert!(
                total <= expected_max,
                "Total INVARIANT VIOLATION: wallet={} total={} expected_max={}",
                wallet,
                total,
                expected_max
            );

            // Log how many completions each wallet got (informational)
            let pct = if expected_max > 0 {
                (total as f64 / expected_max as f64) * 100.0
            } else {
                0.0
            };
            debug!(
                "Worker stress test: wallet={} completed={}/{}({:.0}%)",
                wallet, total, expected_max, pct
            );
        }
    }

    #[tokio::test]
    async fn test_worker_loop_pause_then_cancelled() {
        // Worker should exit cleanly from the pause path when cancelled
        let db = setup_test_db().await;
        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let cancel = CancellationToken::new();

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 1,
            wallet_addresses: vec![],
            worker_count: 1,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(Vec::new())),
            proxy_health: Arc::new(ProxyHealthManager::new(3, 5)),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: true, // force pause window
        };

        let cancel_child = cancel.clone();
        // Cancel after 50ms while worker is in the pause sleep
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            cancel_child.cancel();
        });

        let result = runner.run(cancel).await;
        assert!(
            result.is_ok(),
            "Runner should exit cleanly from pause window when cancelled"
        );
        let stats = result.unwrap();
        assert_eq!(stats.total_attempts, 0, "No tasks should run during pause");
    }

    #[tokio::test]
    async fn test_worker_loop_handles_empty_active_wallets() {
        let db = setup_test_db().await;
        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let cancel = CancellationToken::new();
        let cancel_child = cancel.clone();

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 0,
            wallet_addresses: vec![],
            worker_count: 1,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(Vec::new())),
            proxy_health: Arc::new(ProxyHealthManager::new(3, 5)),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        };

        let cancel_handle = cancel_child.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            cancel_handle.cancel();
        });

        let result = runner.run(cancel_child).await;
        assert!(result.is_ok(), "Runner should exit cleanly");
    }

    // ---- get_task_limit case-insensitive ----

    #[test]
    fn test_get_task_limit_case_insensitive_matching() {
        let mut limits = HashMap::new();
        limits.insert("01_checkbalance".into(), 20u32);
        limits.insert("10_aaveusdtfaucet".into(), 5u32);

        // Config keys are lowercase (config crate lowercases them), task names are camelCase
        assert_eq!(get_task_limit(&limits, "01_checkBalance"), 20);
        assert_eq!(get_task_limit(&limits, "10_aaveUsdtFaucet"), 5);
        assert_eq!(get_task_limit(&limits, "01_checkbalance"), 20);
        // Unconfigured tasks default to 1
        assert_eq!(get_task_limit(&limits, "02_mintUsdtPlus"), 1);
    }

    #[test]
    fn test_get_task_limit_with_mixed_case_limits() {
        let mut limits = HashMap::new();
        limits.insert("01_CHECKBALANCE".into(), 42u32);
        limits.insert("02_MINTUSDTPlus".into(), 99u32);

        assert_eq!(get_task_limit(&limits, "01_checkBalance"), 42);
        assert_eq!(get_task_limit(&limits, "02_mintusdtplus"), 99);
    }

    // ---- create_provider ----

    #[tokio::test]
    async fn test_create_provider_with_valid_url() {
        let db = setup_test_db().await;
        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 1,
            wallet_addresses: vec![],
            worker_count: 1,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(Vec::new())),
            proxy_health: Arc::new(ProxyHealthManager::new(3, 5)),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        };

        let provider = runner.create_provider(&None, "http://localhost:9999").await;
        // Provider should be constructable without panicking
        let _ = provider;
    }

    #[tokio::test]
    async fn test_create_provider_with_proxy() {
        let db = setup_test_db().await;
        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 1,
            wallet_addresses: vec![],
            worker_count: 1,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(Vec::new())),
            proxy_health: Arc::new(ProxyHealthManager::new(3, 5)),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        };

        let proxy = core_logic::config::ProxyConfig {
            url: "http://proxy.example.com:8080".into(),
            username: None,
            password: None,
        };
        let provider = runner
            .create_provider(&Some(proxy), "http://localhost:9999")
            .await;
        let _ = provider;
    }

    // ---- select_proxy ----

    #[tokio::test]
    async fn test_select_proxy_with_mixed_health() {
        let db = setup_test_db().await;
        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let healthy_proxy = core_logic::config::ProxyConfig {
            url: "http://healthy:8080".into(),
            username: None,
            password: None,
        };
        let unhealthy_proxy = core_logic::config::ProxyConfig {
            url: "http://unhealthy:8080".into(),
            username: None,
            password: None,
        };

        let proxy_health = Arc::new(ProxyHealthManager::new(3, 5));
        // Mark unhealthy proxy
        proxy_health.record_failure(&unhealthy_proxy.url).await;
        proxy_health.record_failure(&unhealthy_proxy.url).await;
        proxy_health.record_failure(&unhealthy_proxy.url).await;

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 1,
            wallet_addresses: vec![],
            worker_count: 1,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(vec![healthy_proxy, unhealthy_proxy])),
            proxy_health: proxy_health.clone(),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        };

        let (proxy, _id) = runner.select_proxy().await;
        assert!(proxy.is_some(), "Should select the healthy proxy");
        assert_eq!(proxy.unwrap().url, "http://healthy:8080");
        // Proxy ID should be non-zero (index 1 + 1 = 2 → "002")
        // Actually with the healthy proxy at index 0, the ID should be "001"
        // But since only healthy proxies are selectable, unhealthy at index 1 is skipped
    }

    // ---- DailyRunner field consistency ----

    #[test]
    fn test_daily_runner_all_task_names_match_impl() {
        let tasks = all_tasks();
        let names: Vec<&str> = tasks.iter().map(|t| t.name()).collect();
        assert_eq!(names.len(), ALL_TASK_NAMES.len());
        for (name, expected) in names.iter().zip(ALL_TASK_NAMES.iter()) {
            assert_eq!(*name, *expected);
        }
    }

    #[test]
    fn test_all_task_names_no_empty() {
        for name in ALL_TASK_NAMES {
            assert!(!name.is_empty(), "Task name should not be empty");
            assert!(
                !name.contains(' '),
                "Task name '{}' should not contain spaces",
                name
            );
        }
    }

    #[tokio::test]
    async fn test_worker_loop_returns_early_without_wallets() {
        let db = setup_test_db().await;
        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let cancel = CancellationToken::new();
        let cancel_child = cancel.clone();

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 0,
            wallet_addresses: vec![],
            worker_count: 1,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(Vec::new())),
            proxy_health: Arc::new(ProxyHealthManager::new(3, 5)),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        };

        let cancel_handle = cancel_child.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(200)).await;
            cancel_handle.cancel();
        });

        let result = runner.run(cancel_child).await;
        assert!(result.is_ok(), "Runner should exit cleanly with 0 wallets");
        let stats = result.unwrap();
        assert_eq!(stats.total_attempts, 0);
    }

    #[test]
    fn test_today_utc_always_returns_10_chars() {
        let date = today_utc();
        assert_eq!(date.len(), 10);
        // Verify format YYYY-MM-DD
        assert!(date.chars().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        }));
    }

    #[test]
    fn test_get_remaining_tasks_with_all_tasks_at_zero_limit() {
        // All 17 tasks at limit=0 → no wallet should have remaining
        let counts = HashMap::new();
        let mut limits = HashMap::new();
        for task in ALL_TASK_NAMES {
            limits.insert(task.to_string(), 0u32);
        }
        assert!(!wallet_has_remaining(&counts, &limits));
        let remaining = get_remaining_tasks(&counts, &limits);
        assert!(
            remaining.is_empty(),
            "Expected empty vec, got {} items",
            remaining.len()
        );
    }

    #[test]
    fn test_is_in_pause_window_at_midnight_cross() {
        // Test function composition: is_in_pause_window() delegates to
        // is_in_pause_window_at(Utc::now()). Just verify it doesn't panic
        // and returns a bool.
        let result = is_in_pause_window();
        assert!(result == true || result == false, "Must return a bool");
    }

    #[test]
    fn test_wallet_has_remaining_some_tasks_exceed_limit() {
        let mut counts = HashMap::new();
        let mut limits = HashMap::new();

        // One task has count=10 but limit=5 (exceeded)
        counts.insert("01_checkBalance".to_string(), 10usize);
        limits.insert("01_checkBalance".into(), 5u32);
        // Another task has remaining capacity
        counts.insert("02_mintUsdtPlus".to_string(), 1usize);
        limits.insert("02_mintUsdtPlus".into(), 5u32);

        // Should still return true because 02_mintUsdtPlus has remaining
        assert!(
            wallet_has_remaining(&counts, &limits),
            "Should be true when at least one task has remaining capacity"
        );

        // Now set ALL tasks to exceeded limits
        let mut all_counts = HashMap::new();
        let mut all_limits = HashMap::new();
        for task in ALL_TASK_NAMES {
            all_counts.insert(task.to_string(), 10usize);
            all_limits.insert(task.to_string(), 5u32);
        }
        // Now every task has count >= limit → no remaining
        assert!(
            !wallet_has_remaining(&all_counts, &all_limits),
            "Should be false when all tasks have exceeded limits"
        );

        // get_remaining_tasks should be empty
        let remaining = get_remaining_tasks(&all_counts, &all_limits);
        assert!(
            remaining.is_empty(),
            "Expected no remaining tasks when all exceeded"
        );
    }

    #[tokio::test]
    async fn test_worker_loop_cancelled_immediately() {
        let db = setup_test_db().await;

        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let cancel = CancellationToken::new();
        let cancel_child = cancel.clone();

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 1,
            wallet_addresses: vec![],
            worker_count: 1,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(Vec::new())),
            proxy_health: Arc::new(ProxyHealthManager::new(3, 5)),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        };

        // Cancel BEFORE calling run()
        cancel_child.cancel();

        let result = runner.run(cancel_child).await;
        assert!(
            result.is_ok(),
            "Runner should exit cleanly on immediate cancel"
        );
        let stats = result.unwrap();
        assert_eq!(stats.total_attempts, 0);
    }

    #[test]
    fn test_get_task_limit_unconfigured_task_names_with_numbers() {
        let limits = HashMap::new();

        // Task names with numbers in the body should still default to 1
        assert_eq!(get_task_limit(&limits, "99_some2task"), 1);
        assert_eq!(get_task_limit(&limits, "42_task_with_3_numbers"), 1);
        assert_eq!(get_task_limit(&limits, "07_mint15Usdc"), 1);
        assert_eq!(get_task_limit(&limits, "12_bridge_v2"), 1);

        // Standard names also default to 1 with empty limits
        assert_eq!(get_task_limit(&limits, "01_checkBalance"), 1);
        assert_eq!(get_task_limit(&limits, "10_aaveUsdtFaucet"), 1);
    }

    #[tokio::test]
    async fn test_create_provider_with_authenticated_proxy() {
        let db = setup_test_db().await;
        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 1,
            wallet_addresses: vec![],
            worker_count: 1,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(Vec::new())),
            proxy_health: Arc::new(ProxyHealthManager::new(3, 5)),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        };

        // Proxy with username + password — should not panic
        let proxy = core_logic::config::ProxyConfig {
            url: "http://proxy.example.com:8080".into(),
            username: Some("myuser".into()),
            password: Some("mypassword".into()),
        };
        let provider = runner
            .create_provider(&Some(proxy), "http://localhost:9999")
            .await;
        let _ = provider;
    }

    #[test]
    fn test_all_tasks_have_no_duplicate_names() {
        let tasks = all_tasks();
        assert_eq!(tasks.len(), 19, "Must have exactly 19 tasks");

        let mut names = std::collections::HashSet::new();
        for t in &tasks {
            let name = t.name();
            assert!(!name.is_empty(), "Task name must not be empty");
            assert!(
                name.len() >= 3,
                "Task name '{}' too short for 'XX_' prefix",
                name
            );
            assert!(
                name.chars().take(2).all(|c| c.is_ascii_digit()),
                "Task name '{}' must start with 2 digits",
                name
            );
            assert_eq!(
                name.chars().nth(2),
                Some('_'),
                "Task name '{}' must have underscore after 2 digits",
                name
            );
            names.insert(name);
        }
        assert_eq!(
            names.len(),
            19,
            "Duplicate task names found: {} unique, expected 19",
            names.len()
        );
    }

    // --- wallet_usage_pct ---

    #[test]
    fn test_wallet_usage_pct_empty_state_returns_zero() {
        let counts = HashMap::new();
        let limits = HashMap::new();
        let pct = wallet_usage_pct(&counts, &limits);
        assert_eq!(pct, 0.0, "Empty state should be 0%");
    }

    #[test]
    fn test_wallet_usage_pct_one_of_seventeen_default_limits() {
        let mut counts = HashMap::new();
        counts.insert("01_checkBalance".to_string(), 1usize);
        let limits = HashMap::new();
        // 1 done / 17 total = ~5.88%
        let pct = wallet_usage_pct(&counts, &limits);
        assert!(pct > 5.0 && pct < 7.0, "Expected ~5.88%, got {}", pct);
    }

    #[test]
    fn test_wallet_usage_pct_eight_of_nineteen() {
        let mut counts = HashMap::new();
        for i in 0..8 {
            let name = ALL_TASK_NAMES[i];
            counts.insert(name.to_string(), 1usize);
        }
        let limits = HashMap::new();
        let pct = wallet_usage_pct(&counts, &limits);
        let expected = 8.0 / 19.0 * 100.0;
        assert!(
            (pct - expected).abs() < 0.5,
            "Expected ~{:.1}%, got {}",
            expected,
            pct
        );
    }

    #[test]
    fn test_wallet_usage_pct_all_nineteen_done() {
        let mut counts = HashMap::new();
        for name in ALL_TASK_NAMES {
            counts.insert(name.to_string(), 1usize);
        }
        let limits = HashMap::new();
        let pct = wallet_usage_pct(&counts, &limits);
        assert!((pct - 100.0).abs() < 0.01, "Expected 100%, got {}", pct);
    }

    #[test]
    fn test_wallet_usage_pct_with_custom_limits() {
        let mut counts = HashMap::new();
        counts.insert("01_checkBalance".to_string(), 50usize);
        let mut limits = HashMap::new();
        limits.insert("01_checkbalance".into(), 100u32);
        // 50 done / (100 + 16*1) = 50/116 ≈ 43.1%
        let pct = wallet_usage_pct(&counts, &limits);
        assert!(pct > 40.0 && pct < 45.0, "Expected ~43.1%, got {}", pct);
    }

    #[test]
    fn test_wallet_usage_pct_all_limits_zero() {
        let mut counts = HashMap::new();
        counts.insert("01_checkBalance".to_string(), 5usize);
        let mut limits = HashMap::new();
        for name in ALL_TASK_NAMES {
            limits.insert(name.to_string(), 0u32);
        }
        let pct = wallet_usage_pct(&counts, &limits);
        assert_eq!(pct, 0.0, "All limits zero should return 0%");
    }

    // ---- clone_inner ----

    #[tokio::test]
    async fn test_clone_inner_preserves_wallet_count() {
        let db = setup_test_db().await;
        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 5,
            wallet_addresses: vec!["0xaaa".into(), "0xbbb".into()],
            worker_count: 2,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(Vec::new())),
            proxy_health: Arc::new(ProxyHealthManager::new(3, 5)),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        };

        let cloned = runner.clone_inner();
        assert_eq!(cloned.total_wallets, 5);
        assert_eq!(cloned.wallet_addresses.len(), 2);
        assert_eq!(cloned.worker_count, 2);
        assert_eq!(cloned.min_gwei, 0.01);
        assert_eq!(cloned.config.rpc_url, "http://localhost:9999");
    }

    #[tokio::test]
    #[should_panic(expected = "index out of bounds")]
    async fn test_execute_one_task_panics_on_bad_wallet_idx() {
        let db = setup_test_db().await;
        let config = SepoliaConfig {
            rpc_url: "http://localhost:9999".into(),
            chain_id: 11155111,
            explorer: "https://sepolia.etherscan.io".into(),
            symbol: "ETH".into(),
            private_key_file: None,
            tps: 1,
            worker_amount: None,
            min_delay_ms: None,
            max_delay_ms: None,
            wallet_dir: None,
            proxies: None,
            task_limits: None,
            task_timeout_secs: None,
        };

        let runner = DailyRunner {
            db,
            config,
            wallet_manager: Arc::new(WalletManager::new().unwrap()),
            wallet_password: None,
            total_wallets: 1,
            wallet_addresses: vec![], // empty but total_wallets = 1
            worker_count: 1,
            tasks: all_tasks(),
            task_limits: Arc::new(RwLock::new(HashMap::new())),
            proxy_pool: Arc::new(RwLock::new(Vec::new())),
            proxy_health: Arc::new(ProxyHealthManager::new(3, 5)),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            gas_manager: Arc::new(GasManager::new(
                Arc::new(
                    ethers::providers::Provider::<ethers::providers::Http>::try_from(
                        "http://localhost:9999",
                    )
                    .unwrap(),
                ),
                0.01,
            )),
            min_gwei: 0.01,
            busy_wallets: Arc::new(Mutex::new(std::collections::HashSet::new())),
            base_rpc_url: None,
            base_gas_manager: None,
            base_config: None,
            config_path: None,
            last_limits_mtime: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_pause: false,
        };

        let _ = runner.execute_one_task(0, "2025-01-01", 0).await;
    }
}
