use anyhow::{Context, Result};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

/// Lightweight database for tracking daily task completions.
///
/// **Schema**: each successful task execution creates a row with `success=1`.
/// Multiple rows per (wallet, task, date) are allowed, so a task with
/// `limit=5` can succeed 5 times. Failed attempts create rows with
/// `success=0` and are never counted toward completion.
///
/// **Daily reset**: all queries filter by `date = YYYY-MM-DD`, so when
/// UTC midnight passes all counters reset automatically.
#[derive(Debug, Clone)]
pub struct DailyDb {
    pub(crate) pool: SqlitePool,
}

impl DailyDb {
    /// Number of defined tasks. Used for validation.
    pub const TASKS_PER_WALLET: usize = 17;

    /// Open or create the database, initialising the schema.
    pub async fn new(db_path: &str) -> Result<Self> {
        if !Path::new(db_path).exists() {
            std::fs::File::create(db_path)
                .with_context(|| format!("Failed to create database file: {db_path}"))?;
            info!("Created new daily database file: {db_path}");
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect(&format!("sqlite://{db_path}"))
            .await
            .context("Failed to connect to daily database")?;

        let db = Self { pool };
        db.init_schema().await?;
        Ok(db)
    }

    pub(crate) async fn init_schema(&self) -> Result<()> {
        // Migration: drop old schema that used composite PRIMARY KEY
        // (wallet_idx, task_name, date) which prevented multiple runs
        // per task per day. New schema uses auto-increment PK.
        let has_old_schema: bool = sqlx::query_scalar::<_, i32>(
            "SELECT COUNT(*) FROM pragma_table_info('daily_task_completions') WHERE name = 'id'",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0)
            == 0;

        if has_old_schema {
            info!("Migrating daily_task_completions schema (old → new)");
            sqlx::query("DROP TABLE IF EXISTS daily_task_completions")
                .execute(&self.pool)
                .await
                .context("Failed to drop old daily_task_completions table")?;
        }

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS daily_task_completions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                wallet_idx INTEGER NOT NULL,
                task_name TEXT NOT NULL,
                date TEXT NOT NULL,
                completed_at INTEGER NOT NULL,
                success INTEGER NOT NULL DEFAULT 0,
                message TEXT DEFAULT ''
            );",
        )
        .execute(&self.pool)
        .await
        .context("Failed to create daily_task_completions table")?;

        // Index for fast lookups by wallet + date
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_daily_lookup
             ON daily_task_completions(wallet_idx, date, task_name, success)",
        )
        .execute(&self.pool)
        .await
        .ok();

        info!("Daily database schema initialized");
        Ok(())
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    /// Return per-task completion **counts** for a wallet today.
    ///
    /// Only rows with `success = 1` are counted. Failed tasks are excluded.
    pub async fn get_completed_counts(
        &self,
        wallet_idx: usize,
        date: &str,
    ) -> Result<HashMap<String, usize>> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT task_name, COUNT(*) as cnt
             FROM daily_task_completions
             WHERE wallet_idx = ? AND date = ? AND success = 1
             GROUP BY task_name",
        )
        .bind(wallet_idx as i64)
        .bind(date)
        .fetch_all(&self.pool)
        .await
        .context("Failed to query completion counts")?;

        Ok(rows.into_iter().map(|(name, cnt)| (name, cnt as usize)).collect())
    }

    /// Return the total number of **successful** completions for a wallet today.
    /// Useful for logging / progress display only — does NOT consider per-task limits.
    pub async fn get_total_completed(&self, wallet_idx: usize, date: &str) -> Result<usize> {
        let row = sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM daily_task_completions
             WHERE wallet_idx = ? AND date = ? AND success = 1",
        )
        .bind(wallet_idx as i64)
        .bind(date)
        .fetch_one(&self.pool)
        .await
        .context("Failed to count total completions")?;

        Ok(row.0 as usize)
    }

    /// Return completion counts grouped by wallet, then by task.
    /// Used by the runner to determine which wallets still have pending tasks.
    pub async fn get_all_completed_counts(
        &self,
        date: &str,
    ) -> Result<HashMap<usize, HashMap<String, usize>>> {
        let rows = sqlx::query_as::<_, (i64, String, i64)>(
            "SELECT wallet_idx, task_name, COUNT(*) as cnt
             FROM daily_task_completions
             WHERE date = ? AND success = 1
             GROUP BY wallet_idx, task_name",
        )
        .bind(date)
        .fetch_all(&self.pool)
        .await
        .context("Failed to query all completion counts")?;

        let mut result: HashMap<usize, HashMap<String, usize>> = HashMap::new();
        for (wallet_idx, task_name, cnt) in rows {
            result
                .entry(wallet_idx as usize)
                .or_default()
                .insert(task_name, cnt as usize);
        }

        debug!("All completed counts for {}: {} wallets have data", date, result.len());
        Ok(result)
    }

    /// Record a task execution outcome for a wallet today.
    ///
    /// Inserts a **new row** each time (no UNIQUE constraint on
    /// wallet+task+date), so the same task can be completed multiple
    /// times per day. Only rows with `success=1` count toward limits.
    pub async fn record_task_completion(
        &self,
        wallet_idx: usize,
        task_name: &str,
        date: &str,
        success: bool,
        message: &str,
    ) -> Result<()> {
        let timestamp = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO daily_task_completions
             (wallet_idx, task_name, date, completed_at, success, message)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(wallet_idx as i64)
        .bind(task_name)
        .bind(date)
        .bind(timestamp)
        .bind(if success { 1i32 } else { 0i32 })
        .bind(message)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!(
                "Failed to record task completion for wallet {wallet_idx} / {task_name}"
            )
        })?;

        Ok(())
    }

    /// Close the pool gracefully.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    /// Helper: create an in-memory database for testing.
    async fn test_db() -> DailyDb {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        let db = DailyDb { pool };
        db.init_schema().await.expect("Failed to init schema");
        db
    }

    fn today() -> String {
        Utc::now().format("%Y-%m-%d").to_string()
    }

    fn yesterday() -> String {
        (Utc::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string()
    }

    #[tokio::test]
    async fn test_empty_db_returns_empty_counts() {
        let db = test_db().await;
        let counts = db.get_completed_counts(0, &today()).await.unwrap();
        assert!(counts.is_empty(), "Empty DB should return empty counts");
    }

    #[tokio::test]
    async fn test_record_and_retrieve_single() {
        let db = test_db().await;
        let date = today();

        db.record_task_completion(0, "01_checkBalance", &date, true, "ok")
            .await
            .unwrap();

        let counts = db.get_completed_counts(0, &date).await.unwrap();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 1);
    }

    #[tokio::test]
    async fn test_record_multiple_same_task_increments_count() {
        let db = test_db().await;
        let date = today();

        // Run the same task 5 times successfully
        for _ in 0..5 {
            db.record_task_completion(0, "01_checkBalance", &date, true, "ok")
                .await
                .unwrap();
        }

        let counts = db.get_completed_counts(0, &date).await.unwrap();
        assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 5);
    }

    #[tokio::test]
    async fn test_record_and_retrieve_multiple_tasks() {
        let db = test_db().await;
        let date = today();

        for _ in 0..3 {
            db.record_task_completion(0, "01_checkBalance", &date, true, "ok")
                .await
                .unwrap();
        }
        for _ in 0..2 {
            db.record_task_completion(0, "02_mintUsdtPlus", &date, true, "ok")
                .await
                .unwrap();
        }

        let counts = db.get_completed_counts(0, &date).await.unwrap();
        assert_eq!(counts.len(), 2);
        assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 3);
        assert_eq!(counts.get("02_mintUsdtPlus").copied().unwrap_or(0), 2);
    }

    #[tokio::test]
    async fn test_failed_task_not_counted() {
        let db = test_db().await;
        let date = today();

        // One failed attempt
        db.record_task_completion(0, "01_checkBalance", &date, false, "error")
            .await
            .unwrap();

        let counts = db.get_completed_counts(0, &date).await.unwrap();
        assert!(counts.is_empty(), "Failed task should not be counted");

        let total = db.get_total_completed(0, &date).await.unwrap();
        assert_eq!(total, 0, "Failed task should not increase total");
    }

    #[tokio::test]
    async fn test_failed_then_succeed_both_present() {
        let db = test_db().await;
        let date = today();

        // 2 failures then 3 successes
        for _ in 0..2 {
            db.record_task_completion(0, "01_checkBalance", &date, false, "fail")
                .await
                .unwrap();
        }
        for _ in 0..3 {
            db.record_task_completion(0, "01_checkBalance", &date, true, "ok")
                .await
                .unwrap();
        }

        // Counts only include successes
        let counts = db.get_completed_counts(0, &date).await.unwrap();
        assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 3);

        // Total also only successes
        let total = db.get_total_completed(0, &date).await.unwrap();
        assert_eq!(total, 3);
    }

    #[tokio::test]
    async fn test_get_all_completed_counts_multi_wallet() {
        let db = test_db().await;
        let date = today();

        // Wallet 0: 3x checkBalance, 2x mintUsdtPlus
        for _ in 0..3 {
            db.record_task_completion(0, "01_checkBalance", &date, true, "ok")
                .await
                .unwrap();
        }
        for _ in 0..2 {
            db.record_task_completion(0, "02_mintUsdtPlus", &date, true, "ok")
                .await
                .unwrap();
        }
        // Wallet 1: 1x checkBalance
        db.record_task_completion(1, "01_checkBalance", &date, true, "ok")
            .await
            .unwrap();

        let all = db.get_all_completed_counts(&date).await.unwrap();
        assert_eq!(all.len(), 2, "2 wallets have data");

        let w0 = all.get(&0).unwrap();
        assert_eq!(w0.get("01_checkBalance").copied().unwrap_or(0), 3);
        assert_eq!(w0.get("02_mintUsdtPlus").copied().unwrap_or(0), 2);

        let w1 = all.get(&1).unwrap();
        assert_eq!(w1.get("01_checkBalance").copied().unwrap_or(0), 1);
    }

    #[tokio::test]
    async fn test_get_total_completed_zero_for_new_wallet() {
        let db = test_db().await;
        let total = db.get_total_completed(99, &today()).await.unwrap();
        assert_eq!(total, 0);
    }

    #[tokio::test]
    async fn test_daily_reset_different_dates() {
        let db = test_db().await;
        let today = today();
        let yesterday = yesterday();

        // 3 successes yesterday
        for _ in 0..3 {
            db.record_task_completion(0, "01_checkBalance", &yesterday, true, "ok")
                .await
                .unwrap();
        }

        // Today should be empty
        let counts_today = db.get_completed_counts(0, &today).await.unwrap();
        assert!(counts_today.is_empty(), "Yesterday's data should not appear today");

        // Yesterday should have the data
        let counts_yesterday = db.get_completed_counts(0, &yesterday).await.unwrap();
        assert_eq!(counts_yesterday.get("01_checkBalance").copied().unwrap_or(0), 3);
    }

    #[tokio::test]
    async fn test_multiple_wallets_independent() {
        let db = test_db().await;
        let date = today();

        db.record_task_completion(0, "taskA", &date, true, "ok")
            .await
            .unwrap();
        db.record_task_completion(1, "taskB", &date, true, "ok")
            .await
            .unwrap();

        let w0 = db.get_completed_counts(0, &date).await.unwrap();
        assert!(w0.contains_key("taskA"));
        assert!(!w0.contains_key("taskB"));

        let w1 = db.get_completed_counts(1, &date).await.unwrap();
        assert!(!w1.contains_key("taskA"));
        assert!(w1.contains_key("taskB"));
    }

    #[tokio::test]
    async fn test_tasks_per_wallet_constant() {
        assert_eq!(DailyDb::TASKS_PER_WALLET, 17);
    }

    #[tokio::test]
    async fn test_init_schema_idempotent() {
        let db = test_db().await;
        // Calling init_schema twice should not error
        db.init_schema().await.expect("Second init_schema call should succeed");
        // And queries should still work
        let counts = db.get_completed_counts(0, &today()).await.unwrap();
        assert!(counts.is_empty());
    }

    #[tokio::test]
    async fn test_schema_migration_from_old() {
        // Simulate old schema (composite PK without auto-increment id)
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        // Create the OLD schema with composite PRIMARY KEY
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS daily_task_completions (
                wallet_idx INTEGER NOT NULL,
                task_name TEXT NOT NULL,
                date TEXT NOT NULL,
                completed_at INTEGER NOT NULL,
                success INTEGER NOT NULL DEFAULT 0,
                message TEXT DEFAULT '',
                PRIMARY KEY (wallet_idx, task_name, date)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert some data under old schema
        sqlx::query(
            "INSERT INTO daily_task_completions (wallet_idx, task_name, date, completed_at, success, message)
             VALUES (0, '01_checkBalance', '2025-01-01', 1735689600, 1, 'ok')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let db = DailyDb { pool };

        // Run migration (init_schema should detect old schema and migrate)
        db.init_schema().await.expect("Migration should succeed");

        // Old data should be gone (DROP TABLE during migration)
        let counts = db.get_completed_counts(0, "2025-01-01").await.unwrap();
        assert!(
            counts.is_empty(),
            "Old data should be dropped during migration"
        );

        // New schema should work for writes
        let today = today();
        db.record_task_completion(0, "01_checkBalance", &today, true, "migrated")
            .await
            .unwrap();

        // Read back under new schema
        let new_counts = db.get_completed_counts(0, &today).await.unwrap();
        assert_eq!(
            new_counts.get("01_checkBalance").copied().unwrap_or(0),
            1
        );

        // Verify we can insert multiple rows for same wallet+task+date (new schema feature)
        db.record_task_completion(0, "01_checkBalance", &today, true, "second")
            .await
            .unwrap();
        let multi = db.get_completed_counts(0, &today).await.unwrap();
        assert_eq!(
            multi.get("01_checkBalance").copied().unwrap_or(0),
            2,
            "New schema should allow multiple completions per task per day"
        );
    }

    #[tokio::test]
    async fn test_get_all_completed_counts_no_data() {
        let db = test_db().await;
        let date = "2099-12-31"; // Date far in the future, no data

        let all = db.get_all_completed_counts(date).await.unwrap();
        assert!(
            all.is_empty(),
            "Should return empty map when no data for date"
        );
    }

    #[tokio::test]
    async fn test_message_with_special_chars() {
        let db = test_db().await;
        let date = today();
        let special_msg = "error: timeout (123ms) | status: 500 :: unicode: 你好 🎉 \"quote\"";

        db.record_task_completion(0, "01_checkBalance", &date, false, special_msg)
            .await
            .unwrap();

        let counts = db.get_completed_counts(0, &date).await.unwrap();
        assert!(counts.is_empty(), "Failed task should not count");
    }

    #[tokio::test]
    async fn test_records_persist_across_pool_connections() {
        let dir = std::env::temp_dir();
        let db_path = dir.join(format!("test_daily_db_{}.db", std::process::id()));
        let path_str = db_path.to_str().unwrap();

        let _ = std::fs::remove_file(&db_path);

        // Write
        {
            let db = DailyDb::new(path_str).await.unwrap();
            db.record_task_completion(0, "01_checkBalance", &today(), true, "persist")
                .await
                .unwrap();
            db.close().await;
        }

        // Read
        {
            let db = DailyDb::new(path_str).await.unwrap();
            let counts = db.get_completed_counts(0, &today()).await.unwrap();
            assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 1);
            db.close().await;
        }

        let _ = std::fs::remove_file(&db_path);
    }
}
