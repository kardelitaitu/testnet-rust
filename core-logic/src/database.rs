use anyhow::{Context, Result};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

use crate::error::{ConfigError, DatabaseError};
use smallvec::SmallVec;

/// Configuration for async database logging
#[derive(Debug, Clone, Copy)]
pub struct AsyncDbConfig {
    pub channel_capacity: usize,
    pub batch_size: usize,
    pub flush_interval_ms: u64,
}

impl Default for AsyncDbConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 1000,
            batch_size: 200,
            flush_interval_ms: 200,
        }
    }
}

/// Queued task result for async logging
#[derive(Debug, Clone)]
pub struct QueuedTaskResult {
    pub worker_id: String,
    pub wallet_address: String,
    pub task_name: String,
    pub success: bool,
    pub message: String,
    pub duration_ms: u64,
    pub timestamp: i64,
}

/// Fallback strategy when channel is full
#[derive(Debug, Clone, Copy)]
pub enum FallbackStrategy {
    /// Silently drop the log entry
    Drop,
    /// Block and write synchronously (not recommended in async context)
    Sync,
    /// Drop but log a warning (recommended)
    Hybrid,
}

/// Database manager with optional async logging support
///
/// Note: This struct is not Clone because it contains a JoinHandle.
/// Use Arc<DatabaseManager> for shared ownership.
#[derive(Debug)]
pub struct DatabaseManager {
    pool: SqlitePool,
    metrics: Arc<DbMetrics>,
    /// Async logging channel sender (None if sync mode)
    log_sender: Option<mpsc::Sender<QueuedTaskResult>>,
    /// Background flush task handle
    flush_handle: Option<JoinHandle<()>>,
    /// Async configuration
    async_config: Option<AsyncDbConfig>,
    /// Fallback strategy for channel full
    fallback_strategy: Option<FallbackStrategy>,
}

#[derive(Debug, Default)]
pub struct DbMetrics {
    pub total_queries: AtomicU64,
    pub total_errors: AtomicU64,
    pub total_inserts: AtomicU64,
    pub total_selects: AtomicU64,
    pub avg_query_time_ms: AtomicU64,
    pub query_count_for_avg: AtomicU64,
    /// Async-specific metrics
    pub queued_entries: AtomicU64,
    pub dropped_entries: AtomicU64,
    pub batch_flush_count: AtomicU64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DexOrder {
    pub id: i32,
    pub wallet_address: String,
    pub order_id: String,
    pub base_token: String,
    pub quote_token: String,
    pub amount: String,
    pub is_bid: i32,
    pub tick: i32,
    pub tx_hash: String,
    pub status: String,
    pub timestamp: i64,
}

impl DatabaseManager {
    pub const DEFAULT_MAX_CONNECTIONS: u32 = 20;
    pub const DEFAULT_TIMEOUT_MS: u64 = 30000;

    pub async fn new(db_path: &str) -> Result<Self> {
        if !Path::new(db_path).exists() {
            std::fs::File::create(db_path).map_err(|e| ConfigError::IoError {
                path: db_path.to_string(),
                msg: e.to_string(),
            })?;
            info!("Created new database file: {}", db_path);
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(Self::DEFAULT_MAX_CONNECTIONS)
            .acquire_timeout(Duration::from_millis(Self::DEFAULT_TIMEOUT_MS))
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query("PRAGMA journal_mode=WAL;")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA synchronous=NORMAL;")
                        .execute(&mut *conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(&format!("sqlite://{}", db_path))
            .await
            .map_err(|e| DatabaseError::TransactionFailed { msg: e.to_string() })?;

        let manager = Self {
            pool,
            metrics: Arc::new(DbMetrics::default()),
            log_sender: None,
            flush_handle: None,
            async_config: None,
            fallback_strategy: None,
        };
        manager.init_schema().await?;
        info!(
            "Database initialized with pool size {} (WAL Mode)",
            Self::DEFAULT_MAX_CONNECTIONS
        );
        Ok(manager)
    }

    /// Create a new DatabaseManager with async logging enabled
    pub async fn new_with_async(
        db_path: &str,
        config: AsyncDbConfig,
        fallback: FallbackStrategy,
    ) -> Result<Self> {
        if !Path::new(db_path).exists() {
            std::fs::File::create(db_path).map_err(|e| ConfigError::IoError {
                path: db_path.to_string(),
                msg: e.to_string(),
            })?;
            info!("Created new database file: {}", db_path);
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(Self::DEFAULT_MAX_CONNECTIONS)
            .acquire_timeout(Duration::from_millis(Self::DEFAULT_TIMEOUT_MS))
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query("PRAGMA journal_mode=WAL;")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA synchronous=NORMAL;")
                        .execute(&mut *conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(&format!("sqlite://{}", db_path))
            .await
            .map_err(|e| DatabaseError::TransactionFailed { msg: e.to_string() })?;

        let metrics = Arc::new(DbMetrics::default());

        // Create channel for async logging
        let (tx, rx) = mpsc::channel(config.channel_capacity);

        // Spawn background flush task
        let pool_clone = pool.clone();
        let flush_handle = tokio::spawn(async move {
            db_flush_worker(rx, pool_clone, config).await;
        });

        let manager = Self {
            pool,
            metrics,
            log_sender: Some(tx),
            flush_handle: Some(flush_handle),
            async_config: Some(config),
            fallback_strategy: Some(fallback),
        };

        manager.init_schema().await?;
        info!(
            "Database initialized with async logging (channel: {}, batch: {}, interval: {}ms)",
            config.channel_capacity, config.batch_size, config.flush_interval_ms
        );

        Ok(manager)
    }

    async fn init_schema(&self) -> Result<()> {
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|_| DatabaseError::PoolExhausted {
                max_size: Self::DEFAULT_MAX_CONNECTIONS,
            })?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS task_metrics (
                id INTEGER PRIMARY KEY,
                worker_id TEXT,
                wallet_address TEXT,
                task_name TEXT,
                status TEXT,
                message TEXT,
                duration_ms INTEGER,
                timestamp INTEGER
            );
            CREATE TABLE IF NOT EXISTS created_counter_contracts (
                id INTEGER PRIMARY KEY,
                wallet_address TEXT,
                contract_address TEXT,
                chain_id INTEGER,
                timestamp INTEGER
            );
            CREATE TABLE IF NOT EXISTS created_assets (
                id INTEGER PRIMARY KEY,
                wallet_address TEXT,
                asset_address TEXT,
                asset_type TEXT,
                name TEXT,
                symbol TEXT,
                timestamp INTEGER
            );
            CREATE TABLE IF NOT EXISTS proxy_stats (
                id INTEGER PRIMARY KEY,
                proxy_url TEXT UNIQUE,
                success_count INTEGER DEFAULT 0,
                fail_count INTEGER DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS dex_orders (
                id INTEGER PRIMARY KEY,
                wallet_address TEXT,
                order_id TEXT,
                base_token TEXT,
                quote_token TEXT,
                amount TEXT,
                is_bid INTEGER,
                tick INTEGER,
                tx_hash TEXT,
                status TEXT,
                timestamp INTEGER
            );",
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| DatabaseError::TransactionFailed { msg: e.to_string() })?;

        self.create_indexes().await?;

        info!("Database schema initialized with indexes.");
        Ok(())
    }

    async fn create_indexes(&self) -> Result<()> {
        let indexes = [
            "CREATE INDEX IF NOT EXISTS idx_task_metrics_wallet ON task_metrics(wallet_address);",
            "CREATE INDEX IF NOT EXISTS idx_task_metrics_task ON task_metrics(task_name);",
            "CREATE INDEX IF NOT EXISTS idx_task_metrics_timestamp ON task_metrics(timestamp);",
            "CREATE INDEX IF NOT EXISTS idx_contracts_wallet ON created_counter_contracts(wallet_address);",
            "CREATE INDEX IF NOT EXISTS idx_assets_wallet_type ON created_assets(wallet_address, asset_type);",
            "CREATE INDEX IF NOT EXISTS idx_proxy_stats_url ON proxy_stats(proxy_url);",
            "CREATE INDEX IF NOT EXISTS idx_dex_orders_wallet ON dex_orders(wallet_address);",
        ];

        for idx_sql in indexes {
            if let Err(e) = sqlx::query(idx_sql).execute(&self.pool).await {
                debug!("Index creation skipped (may exist): {}", e);
            }
        }
        Ok(())
    }

    pub async fn log_task_result(
        &self,
        worker_id: &str,
        wallet: &str,
        task: &str,
        success: bool,
        message: &str,
        duration_ms: u64,
    ) -> Result<()> {
        let start = std::time::Instant::now();
        let status = if success { "SUCCESS" } else { "FAILED" };
        let timestamp = chrono::Utc::now().timestamp();

        let result = sqlx::query(
            "INSERT INTO task_metrics (worker_id, wallet_address, task_name, status, message, duration_ms, timestamp) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(worker_id)
        .bind(wallet)
        .bind(task)
        .bind(status)
        .bind(message)
        .bind(duration_ms as i64)
        .bind(timestamp)
        .execute(&self.pool)
        .await;

        self.metrics.total_inserts.fetch_add(1, Ordering::SeqCst);
        self.record_query_time(start, result.is_ok());

        match result {
            Ok(_) => {
                self.metrics.total_queries.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                error!("Failed to log task execution: {}", e);
                Err(e).context("Failed to insert task metric")
            }
        }
    }

    /// Queue a task result for async logging (non-blocking)
    ///
    /// This method returns immediately and does not wait for the database write.
    /// The entry is queued and will be flushed in batches by the background task.
    ///
    /// # Arguments
    /// * `result` - The task result to log
    ///
    /// # Returns
    /// * `Ok(())` - Successfully queued (or dropped based on fallback strategy)
    /// * `Err` - Channel is closed (database shutting down)
    pub fn queue_task_result(&self, result: QueuedTaskResult) -> Result<()> {
        if let Some(sender) = &self.log_sender {
            match sender.try_send(result) {
                Ok(_) => {
                    self.metrics.queued_entries.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    // Channel full - apply fallback strategy
                    self.metrics.dropped_entries.fetch_add(1, Ordering::SeqCst);

                    if let Some(strategy) = self.fallback_strategy {
                        match strategy {
                            FallbackStrategy::Drop => {
                                debug!("Dropped task result (channel full)");
                                Ok(())
                            }
                            FallbackStrategy::Sync => {
                                // For sync fallback, we'd need to block, which defeats the purpose
                                // Log a warning and continue
                                warn!(
                                    "Channel full - would block with sync strategy (not implemented)"
                                );
                                Ok(())
                            }
                            FallbackStrategy::Hybrid => {
                                warn!("Dropped task result (channel full), continuing execution");
                                Ok(())
                            }
                        }
                    } else {
                        Ok(())
                    }
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    Err(anyhow::anyhow!("Database channel closed - shutting down"))
                }
            }
        } else {
            // Async logging not enabled - fall back to sync (for backward compatibility)
            // Note: This shouldn't happen in practice if properly initialized
            Err(anyhow::anyhow!("Async logging not initialized"))
        }
    }

    /// Gracefully shutdown the database, flushing any pending async writes
    ///
    /// # Returns
    /// * `Ok(())` - Shutdown completed successfully
    /// * `Err` - Error during final flush
    pub async fn shutdown(mut self) -> Result<()> {
        info!("Shutting down database (flushing remaining entries)...");

        // Drop sender to signal shutdown to worker
        self.log_sender = None;

        // Wait for flush task to complete (with timeout)
        if let Some(handle) = self.flush_handle.take() {
            match tokio::time::timeout(Duration::from_secs(5), handle).await {
                Ok(Ok(())) => info!("Database flush completed"),
                Ok(Err(e)) => error!("Flush task error: {}", e),
                Err(_) => warn!("Flush timeout - some data may be lost"),
            }
        }

        // Close pool
        self.pool.close().await;
        info!("Database shutdown complete");

        Ok(())
    }

    pub async fn log_counter_contract_creation(
        &self,
        wallet: &str,
        contract: &str,
        chain_id: u64,
    ) -> Result<()> {
        let start = std::time::Instant::now();
        let timestamp = chrono::Utc::now().timestamp();

        let result = sqlx::query(
            "INSERT INTO created_counter_contracts (wallet_address, contract_address, chain_id, timestamp) VALUES (?, ?, ?, ?)"
        )
        .bind(wallet)
        .bind(contract)
        .bind(chain_id as i64)
        .bind(timestamp)
        .execute(&self.pool)
        .await;

        self.metrics.total_inserts.fetch_add(1, Ordering::SeqCst);
        self.record_query_time(start, result.is_ok());

        match result {
            Ok(_) => {
                self.metrics.total_queries.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                error!("Failed to log contract creation: {}", e);
                Err(e).context("Failed to insert contract")
            }
        }
    }

    pub async fn log_asset_creation(
        &self,
        wallet: &str,
        asset_addr: &str,
        asset_type: &str,
        name: &str,
        symbol: &str,
    ) -> Result<()> {
        let start = std::time::Instant::now();
        let timestamp = chrono::Utc::now().timestamp();

        let result = sqlx::query(
            "INSERT INTO created_assets (wallet_address, asset_address, asset_type, name, symbol, timestamp) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(wallet)
        .bind(asset_addr)
        .bind(asset_type)
        .bind(name)
        .bind(symbol)
        .bind(timestamp)
        .execute(&self.pool)
        .await;

        self.metrics.total_inserts.fetch_add(1, Ordering::SeqCst);
        self.record_query_time(start, result.is_ok());

        match result {
            Ok(_) => {
                self.metrics.total_queries.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                error!("Failed to log asset creation: {}", e);
                Err(e).context("Failed to insert asset")
            }
        }
    }

    pub async fn update_proxy_stats(&self, proxy_url: &str, success: bool) -> Result<()> {
        let start = std::time::Instant::now();
        let query = if success {
            "INSERT INTO proxy_stats (proxy_url, success_count, fail_count) VALUES (?, 1, 0)
             ON CONFLICT(proxy_url) DO UPDATE SET success_count = success_count + 1"
        } else {
            "INSERT INTO proxy_stats (proxy_url, success_count, fail_count) VALUES (?, 0, 1)
             ON CONFLICT(proxy_url) DO UPDATE SET fail_count = fail_count + 1"
        };

        let result = sqlx::query(query).bind(proxy_url).execute(&self.pool).await;

        self.metrics.total_inserts.fetch_add(1, Ordering::SeqCst);
        self.record_query_time(start, result.is_ok());

        match result {
            Ok(_) => {
                self.metrics.total_queries.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                error!("Failed to update proxy stats: {}", e);
                Err(e).context("Failed to update proxy stats")
            }
        }
    }

    pub async fn get_assets_by_type(&self, wallet: &str, asset_type: &str) -> Result<Vec<String>> {
        let start = std::time::Instant::now();

        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT asset_address FROM created_assets WHERE wallet_address = ? AND asset_type = ?",
        )
        .bind(wallet)
        .bind(asset_type)
        .fetch_all(&self.pool)
        .await;

        self.metrics.total_selects.fetch_add(1, Ordering::SeqCst);
        self.record_query_time(start, rows.is_ok());

        match rows {
            Ok(rows) => {
                self.metrics.total_queries.fetch_add(1, Ordering::SeqCst);
                Ok(rows.into_iter().map(|r| r.0).collect())
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                Err(e).context("Failed to query assets by type")
            }
        }
    }

    pub async fn get_all_assets_by_type(&self, asset_type: &str) -> Result<Vec<String>> {
        let start = std::time::Instant::now();

        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT asset_address FROM created_assets WHERE asset_type = ? ORDER BY id DESC LIMIT 100",
        )
        .bind(asset_type)
        .fetch_all(&self.pool)
        .await;

        self.metrics.total_selects.fetch_add(1, Ordering::SeqCst);
        self.record_query_time(start, rows.is_ok());

        match rows {
            Ok(rows) => {
                self.metrics.total_queries.fetch_add(1, Ordering::SeqCst);
                Ok(rows.into_iter().map(|r| r.0).collect())
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                Err(e).context("Failed to query all assets by type")
            }
        }
    }

    pub async fn get_deployed_counter_contracts(
        &self,
        wallet: &str,
        chain_id: u64,
    ) -> Result<Vec<String>> {
        let start = std::time::Instant::now();

        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT contract_address FROM created_counter_contracts WHERE wallet_address = ? AND chain_id = ?"
        )
        .bind(wallet)
        .bind(chain_id as i64)
        .fetch_all(&self.pool)
        .await;

        self.metrics.total_selects.fetch_add(1, Ordering::SeqCst);
        self.record_query_time(start, rows.is_ok());

        match rows {
            Ok(rows) => {
                self.metrics.total_queries.fetch_add(1, Ordering::SeqCst);
                Ok(rows.into_iter().map(|r| r.0).collect())
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                Err(e).context("Failed to query deployed contracts")
            }
        }
    }

    pub async fn get_asset_count_by_address(&self, wallet: &str, asset_type: &str) -> Result<i32> {
        let start = std::time::Instant::now();

        let row = sqlx::query_as::<_, (i32,)>(
            "SELECT COUNT(*) FROM created_assets WHERE wallet_address = ? AND asset_type = ?",
        )
        .bind(wallet)
        .bind(asset_type)
        .fetch_one(&self.pool)
        .await;

        self.metrics.total_selects.fetch_add(1, Ordering::SeqCst);
        self.record_query_time(start, row.is_ok());

        match row {
            Ok((count,)) => {
                self.metrics.total_queries.fetch_add(1, Ordering::SeqCst);
                Ok(count)
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                Err(e).context("Failed to count assets")
            }
        }
    }

    pub async fn get_transaction_count(&self, wallet: &str) -> Result<i32> {
        let start = std::time::Instant::now();

        let row = sqlx::query_as::<_, (i32,)>(
            "SELECT COUNT(*) FROM task_metrics WHERE wallet_address = ?",
        )
        .bind(wallet)
        .fetch_one(&self.pool)
        .await;

        self.metrics.total_selects.fetch_add(1, Ordering::SeqCst);
        self.record_query_time(start, row.is_ok());

        match row {
            Ok((count,)) => {
                self.metrics.total_queries.fetch_add(1, Ordering::SeqCst);
                Ok(count)
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                Err(e).context("Failed to count transactions")
            }
        }
    }

    pub async fn get_success_count(&self, wallet: &str) -> Result<i32> {
        let start = std::time::Instant::now();

        let row = sqlx::query_as::<_, (i32,)>(
            "SELECT COUNT(*) FROM task_metrics WHERE wallet_address = ? AND status = 'SUCCESS'",
        )
        .bind(wallet)
        .fetch_one(&self.pool)
        .await;

        self.metrics.total_selects.fetch_add(1, Ordering::SeqCst);
        self.record_query_time(start, row.is_ok());

        match row {
            Ok((count,)) => {
                self.metrics.total_queries.fetch_add(1, Ordering::SeqCst);
                Ok(count)
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                Err(e).context("Failed to count successful transactions")
            }
        }
    }

    /// Check if a specific task has succeeded for a wallet
    pub async fn has_task_succeeded(&self, wallet: &str, task_name: &str) -> Result<bool> {
        let start = std::time::Instant::now();

        let row = sqlx::query_as::<_, (i32,)>(
            "SELECT COUNT(*) FROM task_metrics WHERE wallet_address = ? AND task_name = ? AND status = 'SUCCESS'",
        )
        .bind(wallet)
        .bind(task_name)
        .fetch_one(&self.pool)
        .await;

        self.metrics.total_selects.fetch_add(1, Ordering::SeqCst);
        self.record_query_time(start, row.is_ok());

        match row {
            Ok((count,)) => {
                self.metrics.total_queries.fetch_add(1, Ordering::SeqCst);
                Ok(count > 0)
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                Err(e).context(format!(
                    "Failed to check task success for {}: {}",
                    wallet, task_name
                ))
            }
        }
    }

    pub async fn batch_log_task_results(&self, results: &[TaskMetricBatchItem]) -> Result<usize> {
        if results.is_empty() {
            return Ok(0);
        }

        // Use SmallVec for stack allocation - typical batch size is 200
        // SmallVec<[T; 32]> stores up to 32 items on the stack before heap allocation
        type BatchRow = (String, String, String, String, String, i64, i64);
        let mut batch_params: SmallVec<[BatchRow; 32]> = SmallVec::new();

        let timestamp = chrono::Utc::now().timestamp();

        for item in results {
            let status = if item.success { "SUCCESS" } else { "FAILED" };
            batch_params.push((
                item.worker_id.clone(),
                item.wallet.clone(),
                item.task.clone(),
                status.to_string(),
                item.message.clone(),
                item.duration_ms as i64,
                timestamp,
            ));
        }

        // Batch insert in a single transaction for better performance
        let mut tx = self.pool.begin().await?;
        let mut inserted = 0;

        for param in &batch_params {
            let result = sqlx::query(
                "INSERT INTO task_metrics (worker_id, wallet_address, task_name, status, message, duration_ms, timestamp) VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&param.0)
            .bind(&param.1)
            .bind(&param.2)
            .bind(&param.3)
            .bind(&param.4)
            .bind(param.5)
            .bind(param.6)
            .execute(&mut *tx)
            .await;

            match result {
                Ok(_) => {
                    inserted += 1;
                    self.metrics.total_inserts.fetch_add(1, Ordering::SeqCst);
                }
                Err(_) => {
                    self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                }
            }
        }

        tx.commit().await?;

        self.metrics
            .total_queries
            .fetch_add(results.len() as u64, Ordering::SeqCst);

        Ok(inserted)
    }

    pub fn get_metrics(&self) -> DbMetricsSnapshot {
        DbMetricsSnapshot {
            total_queries: self.metrics.total_queries.load(Ordering::SeqCst),
            total_errors: self.metrics.total_errors.load(Ordering::SeqCst),
            total_inserts: self.metrics.total_inserts.load(Ordering::SeqCst),
            total_selects: self.metrics.total_selects.load(Ordering::SeqCst),
        }
    }

    /// Get the async database configuration (if async mode is enabled)
    pub fn get_async_config(&self) -> Option<AsyncDbConfig> {
        self.async_config
    }

    /// Check if async logging is enabled
    pub fn is_async(&self) -> bool {
        self.async_config.is_some()
    }

    /// Get async-specific metrics (queued and dropped entries)
    pub fn get_async_metrics(&self) -> (u64, u64) {
        (
            self.metrics.queued_entries.load(Ordering::SeqCst),
            self.metrics.dropped_entries.load(Ordering::SeqCst),
        )
    }

    pub async fn log_dex_order(
        &self,
        wallet: &str,
        order_id: &str,
        base_token: &str,
        quote_token: &str,
        amount: &str,
        is_bid: bool,
        tick: i16,
        tx_hash: &str,
    ) -> Result<()> {
        let start = std::time::Instant::now();
        let timestamp = chrono::Utc::now().timestamp();

        let result = sqlx::query(
            "INSERT INTO dex_orders (wallet_address, order_id, base_token, quote_token, amount, is_bid, tick, tx_hash, status, timestamp) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'ACTIVE', ?)"
        )
        .bind(wallet)
        .bind(order_id)
        .bind(base_token)
        .bind(quote_token)
        .bind(amount)
        .bind(if is_bid { 1 } else { 0 })
        .bind(tick as i32)
        .bind(tx_hash)
        .bind(timestamp)
        .execute(&self.pool)
        .await;

        self.record_query_time(start, result.is_ok());

        match result {
            Ok(_) => {
                self.metrics.total_inserts.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                error!("Failed to log DEX order: {}", e);
                Err(e).context("Failed to insert DEX order")
            }
        }
    }

    pub async fn get_active_orders(&self, wallet: &str) -> Result<Vec<DexOrder>> {
        let start = std::time::Instant::now();

        let rows = sqlx::query_as::<_, DexOrder>(
            "SELECT id, wallet_address, order_id, base_token, quote_token, amount, is_bid, tick, tx_hash, status, timestamp FROM dex_orders WHERE wallet_address = ? AND status = 'ACTIVE' ORDER BY id DESC"
        )
        .bind(wallet)
        .fetch_all(&self.pool)
        .await;

        self.metrics.total_selects.fetch_add(1, Ordering::SeqCst);
        self.record_query_time(start, rows.is_ok());

        match rows {
            Ok(orders) => {
                self.metrics.total_queries.fetch_add(1, Ordering::SeqCst);
                Ok(orders)
            }
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                Err(e).context("Failed to get DEX orders")
            }
        }
    }

    pub async fn update_order_status(&self, order_id: &str, status: &str) -> Result<()> {
        let start = std::time::Instant::now();

        let result = sqlx::query("UPDATE dex_orders SET status = ? WHERE order_id = ?")
            .bind(status)
            .bind(order_id)
            .execute(&self.pool)
            .await;

        self.record_query_time(start, result.is_ok());

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                self.metrics.total_errors.fetch_add(1, Ordering::SeqCst);
                Err(e).context("Failed to update order status")
            }
        }
    }

    fn record_query_time(&self, start: std::time::Instant, success: bool) {
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let count = self.metrics.query_count_for_avg.load(Ordering::SeqCst);
        let current_avg = self.metrics.avg_query_time_ms.load(Ordering::SeqCst);

        if success {
            let new_count = count + 1;
            let new_avg = if count == 0 {
                elapsed_ms
            } else {
                (current_avg * count + elapsed_ms) / new_count
            };
            self.metrics
                .query_count_for_avg
                .store(new_count, Ordering::SeqCst);
            self.metrics
                .avg_query_time_ms
                .store(new_avg, Ordering::SeqCst);
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskMetricBatchItem {
    pub worker_id: String,
    pub wallet: String,
    pub task: String,
    pub success: bool,
    pub message: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct DbMetricsSnapshot {
    pub total_queries: u64,
    pub total_errors: u64,
    pub total_inserts: u64,
    pub total_selects: u64,
}

impl DbMetricsSnapshot {
    pub fn error_rate(&self) -> f64 {
        if self.total_queries == 0 {
            0.0
        } else {
            self.total_errors as f64 / self.total_queries as f64 * 100.0
        }
    }
}

/// Background worker that batches and flushes database writes
///
/// This function runs in a separate tokio task and handles:
/// - Receiving entries from workers via channel
/// - Batching entries up to config.batch_size
/// - Periodic flushing based on config.flush_interval_ms
/// - Graceful shutdown when channel closes
async fn db_flush_worker(
    mut rx: mpsc::Receiver<QueuedTaskResult>,
    pool: SqlitePool,
    config: AsyncDbConfig,
) {
    let mut batch = Vec::with_capacity(config.batch_size);
    let mut flush_interval = tokio::time::interval(Duration::from_millis(config.flush_interval_ms));

    info!(
        "Database flush worker started (batch: {}, interval: {}ms)",
        config.batch_size, config.flush_interval_ms
    );

    loop {
        tokio::select! {
            // Receive new entries from workers
            Some(entry) = rx.recv() => {
                batch.push(entry);

                // Flush immediately if batch is full
                if batch.len() >= config.batch_size {
                    if let Err(e) = flush_batch(&batch, &pool).await {
                        error!("Failed to flush batch: {}", e);
                    }
                    batch.clear();
                }
            }

            // Periodic flush based on time
            _ = flush_interval.tick() => {
                if !batch.is_empty() {
                    if let Err(e) = flush_batch(&batch, &pool).await {
                        error!("Failed to flush batch: {}", e);
                    }
                    batch.clear();
                }
            }

            // Channel closed (shutdown signal)
            else => {
                info!("Database channel closed, performing final flush");
                break;
            }
        }
    }

    // Final flush on shutdown
    if !batch.is_empty() {
        if let Err(e) = flush_batch(&batch, &pool).await {
            error!("Final flush failed: {}", e);
        } else {
            info!("Final flush completed: {} entries", batch.len());
        }
    }

    info!("Database flush worker stopped");
}

/// Flush a batch of entries to SQLite in a single transaction
///
/// # Arguments
/// * `batch` - Slice of entries to flush
/// * `pool` - Database connection pool
///
/// # Returns
/// * `Ok(())` - All entries flushed successfully
/// * `Err` - Database error during flush
async fn flush_batch(batch: &[QueuedTaskResult], pool: &SqlitePool) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }

    let start = Instant::now();

    // Use SmallVec for batch parameters - typical batch size is 200
    // SmallVec<[T; 64]> stores up to 64 items on the stack
    type FlushRow = (String, String, String, String, String, i64, i64);
    let mut rows: SmallVec<[FlushRow; 64]> = SmallVec::new();

    for entry in batch {
        rows.push((
            entry.worker_id.clone(),
            entry.wallet_address.clone(),
            entry.task_name.clone(),
            if entry.success {
                "SUCCESS".to_string()
            } else {
                "FAILED".to_string()
            },
            entry.message.clone(),
            entry.duration_ms as i64,
            entry.timestamp,
        ));
    }

    // Single transaction for the entire batch
    let mut tx = pool.begin().await?;

    for row in &rows {
        sqlx::query(
            "INSERT INTO task_metrics (worker_id, wallet_address, task_name, status, message, duration_ms, timestamp) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&row.0)
        .bind(&row.1)
        .bind(&row.2)
        .bind(&row.3)
        .bind(&row.4)
        .bind(row.5)
        .bind(row.6)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let elapsed = start.elapsed();
    debug!(
        target: "database",
        "Flushed {} entries in {:.2}ms ({:.0} entries/sec)",
        batch.len(),
        elapsed.as_millis(),
        batch.len() as f64 / elapsed.as_secs_f64()
    );

    Ok(())
}
