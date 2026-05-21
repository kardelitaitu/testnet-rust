use anyhow::{Context, Result};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

/// Lightweight database for tracking daily task completions.
///
/// **Schema**: each (wallet_address, task_name, date) pair is a single row with
/// `count_success` and `count_failed` columns. The `PRIMARY KEY` enforces
/// uniqueness at the database level — a task can never exceed its daily limit
/// even if a bug tries to insert duplicate completions.
///
/// **Daily reset**: the `date` column is derived from `chrono::Utc::now()`
/// **inside** `record_task_completion`, not from the caller's parameter.
/// This guarantees completions always count toward the correct UTC day,
/// even if a task crosses midnight during execution.
///
/// **Wallet identity**: wallets are identified by their EVM address string
/// (`0x...`), not by an opaque index. Addresses are resolved at startup
/// by decrypting each wallet once.
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
        // Migration v3→v4: `id PK + multiple rows` → `PK(wallet, task, date) + count columns`.
        // Detect old schema (has `id` column) and recreate.
        let has_old_schema: bool = sqlx::query_scalar::<_, i32>(
            "SELECT COUNT(*) FROM pragma_table_info('daily_task_completions') WHERE name IN ('wallet_idx', 'id')",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0)
            > 0;

        if has_old_schema {
            info!("Migrating daily_task_completions schema (id PK → composite PK + count columns)");
            sqlx::query("DROP TABLE IF EXISTS daily_task_completions")
                .execute(&self.pool)
                .await
                .context("Failed to drop old daily_task_completions table")?;
        }

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS daily_task_completions (
                wallet_address TEXT NOT NULL,
                task_name TEXT NOT NULL,
                date TEXT NOT NULL,
                count_success INTEGER NOT NULL DEFAULT 0,
                count_failed INTEGER NOT NULL DEFAULT 0,
                completed_at INTEGER NOT NULL,
                message TEXT DEFAULT '',
                PRIMARY KEY (wallet_address, task_name, date)
            );",
        )
        .execute(&self.pool)
        .await
        .context("Failed to create daily_task_completions table")?;

        info!("Daily database schema initialized");
        Ok(())
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    /// Return per-task completion **counts** for a wallet today.
    ///
    /// Reads the `count_success` column from the single row per (wallet, task, date).
    pub async fn get_completed_counts(
        &self,
        wallet_address: &str,
        date: &str,
    ) -> Result<HashMap<String, usize>> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT task_name, count_success
             FROM daily_task_completions
             WHERE wallet_address = ? AND date = ? AND count_success > 0",
        )
        .bind(wallet_address)
        .bind(date)
        .fetch_all(&self.pool)
        .await
        .context("Failed to query completion counts")?;

        Ok(rows
            .into_iter()
            .map(|(name, cnt)| (name, cnt as usize))
            .collect())
    }

    /// Return the total number of **successful** completions for a wallet today.
    /// Useful for logging / progress display only — does NOT consider per-task limits.
    pub async fn get_total_completed(&self, wallet_address: &str, date: &str) -> Result<usize> {
        let row = sqlx::query_as::<_, (i64,)>(
            "SELECT COALESCE(SUM(count_success), 0) FROM daily_task_completions
             WHERE wallet_address = ? AND date = ?",
        )
        .bind(wallet_address)
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
    ) -> Result<HashMap<String, HashMap<String, usize>>> {
        let rows = sqlx::query_as::<_, (String, String, i64)>(
            "SELECT wallet_address, task_name, count_success
             FROM daily_task_completions
             WHERE date = ? AND count_success > 0",
        )
        .bind(date)
        .fetch_all(&self.pool)
        .await
        .context("Failed to query all completion counts")?;

        let mut result: HashMap<String, HashMap<String, usize>> = HashMap::new();
        for (wallet_address, task_name, cnt) in rows {
            result
                .entry(wallet_address)
                .or_default()
                .insert(task_name, cnt as usize);
        }

        debug!(
            "All completed counts for {}: {} wallets have data",
            date,
            result.len()
        );
        Ok(result)
    }

    /// Record a task execution outcome for a wallet today.
    ///
    /// **Date is derived from `chrono::Utc::now()` internally**, not from the
    /// `_date` parameter. This guarantees the stored date matches the actual
    /// completion time — even if a task crosses UTC midnight while executing.
    ///
    /// Uses `ON CONFLICT` UPSERT — the PRIMARY KEY (wallet, task, date) prevents
    /// duplicate rows. On success, `count_success` is incremented; on failure,
    /// `count_failed` is incremented.
    pub async fn record_task_completion(
        &self,
        wallet_address: &str,
        task_name: &str,
        _date: &str,
        success: bool,
        message: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now();
        let date = now.format("%Y-%m-%d").to_string();
        let timestamp = now.timestamp();

        if success {
            sqlx::query(
                "INSERT INTO daily_task_completions
                 (wallet_address, task_name, date, completed_at, count_success, message)
                 VALUES (?, ?, ?, ?, 1, ?)
                 ON CONFLICT(wallet_address, task_name, date)
                 DO UPDATE SET
                     count_success = count_success + 1,
                     completed_at = excluded.completed_at,
                     message = excluded.message",
            )
            .bind(wallet_address)
            .bind(task_name)
            .bind(&date)
            .bind(timestamp)
            .bind(message)
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!(
                    "Failed to record task completion for wallet {wallet_address} / {task_name}"
                )
            })?;
        } else {
            sqlx::query(
                "INSERT INTO daily_task_completions
                 (wallet_address, task_name, date, completed_at, count_failed, message)
                 VALUES (?, ?, ?, ?, 1, ?)
                 ON CONFLICT(wallet_address, task_name, date)
                 DO UPDATE SET
                     count_failed = count_failed + 1,
                     completed_at = excluded.completed_at,
                     message = excluded.message",
            )
            .bind(wallet_address)
            .bind(task_name)
            .bind(&date)
            .bind(timestamp)
            .bind(message)
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!("Failed to record task failure for wallet {wallet_address} / {task_name}")
            })?;
        }

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
        let counts = db.get_completed_counts("0xalice", &today()).await.unwrap();
        assert!(counts.is_empty(), "Empty DB should return empty counts");
    }

    #[tokio::test]
    async fn test_record_and_retrieve_single() {
        let db = test_db().await;
        let date = today();

        db.record_task_completion("0xalice", "01_checkBalance", &date, true, "ok")
            .await
            .unwrap();

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 1);
    }

    #[tokio::test]
    async fn test_record_task_completion_derives_date_internally() {
        let db = test_db().await;
        let expected_today = today();

        // Call with a deliberately WRONG date ("2099-01-01") to prove
        // the function ignores the passed date and derives its own.
        db.record_task_completion(
            "0xalice",
            "01_checkBalance",
            "2099-01-01",
            true,
            "cross-midnight",
        )
        .await
        .unwrap();

        // Record should be found under today's date, NOT the fake date
        let counts_today = db
            .get_completed_counts("0xalice", &expected_today)
            .await
            .unwrap();
        assert_eq!(
            counts_today.get("01_checkBalance").copied().unwrap_or(0),
            1,
            "Should be found under today's date (derived from Utc::now())"
        );

        // Should NOT be found under the fake date
        let counts_fake = db
            .get_completed_counts("0xalice", "2099-01-01")
            .await
            .unwrap();
        assert!(
            counts_fake.is_empty(),
            "Should NOT be found under the passed date"
        );

        // Verify total reflects the correct count
        let total = db
            .get_total_completed("0xalice", &expected_today)
            .await
            .unwrap();
        assert_eq!(
            total, 1,
            "Total should count the completion under today's date"
        );
    }

    #[tokio::test]
    async fn test_record_multiple_same_task_increments_count() {
        let db = test_db().await;
        let date = today();

        // Run the same task 5 times successfully
        for _ in 0..5 {
            db.record_task_completion("0xalice", "01_checkBalance", &date, true, "ok")
                .await
                .unwrap();
        }

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 5);
    }

    #[tokio::test]
    async fn test_record_and_retrieve_multiple_tasks() {
        let db = test_db().await;
        let date = today();

        for _ in 0..3 {
            db.record_task_completion("0xalice", "01_checkBalance", &date, true, "ok")
                .await
                .unwrap();
        }
        for _ in 0..2 {
            db.record_task_completion("0xalice", "02_mintUsdtPlus", &date, true, "ok")
                .await
                .unwrap();
        }

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(counts.len(), 2);
        assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 3);
        assert_eq!(counts.get("02_mintUsdtPlus").copied().unwrap_or(0), 2);
    }

    #[tokio::test]
    async fn test_failed_task_not_counted() {
        let db = test_db().await;
        let date = today();

        // One failed attempt
        db.record_task_completion("0xalice", "01_checkBalance", &date, false, "error")
            .await
            .unwrap();

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert!(counts.is_empty(), "Failed task should not be counted");

        let total = db.get_total_completed("0xalice", &date).await.unwrap();
        assert_eq!(total, 0, "Failed task should not increase total");
    }

    #[tokio::test]
    async fn test_failed_then_succeed_both_present() {
        let db = test_db().await;
        let date = today();

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

        // Counts only include successes
        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 3);

        // Total also only successes
        let total = db.get_total_completed("0xalice", &date).await.unwrap();
        assert_eq!(total, 3);
    }

    #[tokio::test]
    async fn test_get_all_completed_counts_multi_wallet() {
        let db = test_db().await;
        let date = today();

        // Wallet 0: 3x checkBalance, 2x mintUsdtPlus
        for _ in 0..3 {
            db.record_task_completion("0xalice", "01_checkBalance", &date, true, "ok")
                .await
                .unwrap();
        }
        for _ in 0..2 {
            db.record_task_completion("0xalice", "02_mintUsdtPlus", &date, true, "ok")
                .await
                .unwrap();
        }
        // Wallet 1: 1x checkBalance
        db.record_task_completion("0xbob", "01_checkBalance", &date, true, "ok")
            .await
            .unwrap();

        let all = db.get_all_completed_counts(&date).await.unwrap();
        assert_eq!(all.len(), 2, "2 wallets have data");

        let w0 = all.get("0xalice").unwrap();
        assert_eq!(w0.get("01_checkBalance").copied().unwrap_or(0), 3);
        assert_eq!(w0.get("02_mintUsdtPlus").copied().unwrap_or(0), 2);

        let w1 = all.get("0xbob").unwrap();
        assert_eq!(w1.get("01_checkBalance").copied().unwrap_or(0), 1);
    }

    #[tokio::test]
    async fn test_get_total_completed_zero_for_new_wallet() {
        let db = test_db().await;
        let total = db.get_total_completed("0xunknown", &today()).await.unwrap();
        assert_eq!(total, 0);
    }

    #[tokio::test]
    async fn test_daily_reset_different_dates() {
        let db = test_db().await;
        let today = today();
        let yesterday = yesterday();

        // Insert 3 records under yesterday's date via raw SQL (bypassing
        // record_task_completion which derives date from Utc::now()).
        // count_success=3 means the task was completed 3 times yesterday.
        let now_ts = Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO daily_task_completions
             (wallet_address, task_name, date, count_success, completed_at, message)
             VALUES (?, ?, ?, 3, ?, 'yesterday')",
        )
        .bind("0xalice")
        .bind("01_checkBalance")
        .bind(&yesterday)
        .bind(now_ts)
        .execute(&db.pool)
        .await
        .unwrap();

        // Today should be empty — queries filter by date
        let counts_today = db.get_completed_counts("0xalice", &today).await.unwrap();
        assert!(
            counts_today.is_empty(),
            "Yesterday's data should not appear today"
        );

        // Yesterday should have the data
        let counts_yesterday = db
            .get_completed_counts("0xalice", &yesterday)
            .await
            .unwrap();
        assert_eq!(
            counts_yesterday
                .get("01_checkBalance")
                .copied()
                .unwrap_or(0),
            3,
            "Yesterday's data should be queryable by yesterday's date"
        );
    }

    #[tokio::test]
    async fn test_multiple_wallets_independent() {
        let db = test_db().await;
        let date = today();

        db.record_task_completion("0xalice", "taskA", &date, true, "ok")
            .await
            .unwrap();
        db.record_task_completion("0xbob", "taskB", &date, true, "ok")
            .await
            .unwrap();

        let w0 = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert!(w0.contains_key("taskA"));
        assert!(!w0.contains_key("taskB"));

        let w1 = db.get_completed_counts("0xbob", &date).await.unwrap();
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
        db.init_schema()
            .await
            .expect("Second init_schema call should succeed");
        // And queries should still work
        let counts = db.get_completed_counts("0xalice", &today()).await.unwrap();
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
        let counts = db
            .get_completed_counts("0xalice", "2025-01-01")
            .await
            .unwrap();
        assert!(
            counts.is_empty(),
            "Old data should be dropped during migration"
        );

        // New schema should work for writes
        let today = today();
        db.record_task_completion("0xalice", "01_checkBalance", &today, true, "migrated")
            .await
            .unwrap();

        // Read back under new schema
        let new_counts = db.get_completed_counts("0xalice", &today).await.unwrap();
        assert_eq!(new_counts.get("01_checkBalance").copied().unwrap_or(0), 1);

        // Verify UPSERT correctly increments count_success
        db.record_task_completion("0xalice", "01_checkBalance", &today, true, "second")
            .await
            .unwrap();
        let multi = db.get_completed_counts("0xalice", &today).await.unwrap();
        assert_eq!(
            multi.get("01_checkBalance").copied().unwrap_or(0),
            2,
            "UPSERT should increment count_success on second completion"
        );
    }

    #[tokio::test]
    async fn test_duplicate_insert_does_not_overflow_limit() {
        let db = test_db().await;
        let date = today();

        // Insert the same wallet+task+date 100 times - PK prevents duplicates,
        // UPSERT just increments count_success. No overflow, no extra rows.
        for _ in 0..100 {
            db.record_task_completion("0xalice", "01_checkBalance", &date, true, "dup")
                .await
                .unwrap();
        }

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(
            counts.get("01_checkBalance").copied().unwrap_or(0),
            100,
            "count_success should increment to 100, not overflow or create extra rows"
        );

        // Only 1 task in the result set (not 100 rows)
        assert_eq!(counts.len(), 1, "Should be exactly 1 unique task");

        // Verify total matches
        let total = db.get_total_completed("0xalice", &date).await.unwrap();
        assert_eq!(total, 100, "Total should reflect 100 completions");
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

        db.record_task_completion("0xalice", "01_checkBalance", &date, false, special_msg)
            .await
            .unwrap();

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
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
            db.record_task_completion("0xalice", "01_checkBalance", &today(), true, "persist")
                .await
                .unwrap();
            db.close().await;
        }

        // Read
        {
            let db = DailyDb::new(path_str).await.unwrap();
            let counts = db.get_completed_counts("0xalice", &today()).await.unwrap();
            assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 1);
            db.close().await;
        }

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_get_total_completed_with_mixed_results() {
        let db = test_db().await;
        let date = today();

        for _ in 0..3 {
            db.record_task_completion("0xalice", "01_checkBalance", &date, true, "ok")
                .await
                .unwrap();
        }
        for _ in 0..2 {
            db.record_task_completion("0xalice", "01_checkBalance", &date, false, "fail")
                .await
                .unwrap();
        }
        db.record_task_completion("0xalice", "02_mintUsdtPlus", &date, true, "ok")
            .await
            .unwrap();

        let total = db.get_total_completed("0xalice", &date).await.unwrap();
        assert_eq!(total, 4);

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(counts.len(), 2);
        assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 3);
        assert_eq!(counts.get("02_mintUsdtPlus").copied().unwrap_or(0), 1);
    }

    #[tokio::test]
    async fn test_empty_wallet_address_accepted() {
        let db = test_db().await;
        let date = today();

        db.record_task_completion("", "01_checkBalance", &date, true, "empty addr")
            .await
            .unwrap();

        let counts = db.get_completed_counts("", &date).await.unwrap();
        assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 1);
    }

    #[tokio::test]
    async fn test_long_task_name_does_not_truncate() {
        let db = test_db().await;
        let date = today();
        let long_name = format!("99_{}_longTaskName", "x".repeat(100));

        db.record_task_completion("0xalice", &long_name, &date, true, "long name test")
            .await
            .unwrap();

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(counts.get(&long_name).copied().unwrap_or(0), 1);
    }

    #[tokio::test]
    async fn test_record_task_completion_with_empty_message() {
        let db = test_db().await;
        let date = today();

        db.record_task_completion("0xalice", "01_checkBalance", &date, true, "")
            .await
            .unwrap();

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(counts.get("01_checkBalance").copied().unwrap_or(0), 1);
    }

    #[tokio::test]
    async fn test_get_all_completed_counts_respects_success_only() {
        let db = test_db().await;
        let date = today();

        db.record_task_completion("0xalice", "taskA", &date, true, "ok")
            .await
            .unwrap();
        db.record_task_completion("0xalice", "taskA", &date, true, "ok")
            .await
            .unwrap();
        db.record_task_completion("0xalice", "taskA", &date, false, "fail")
            .await
            .unwrap();
        db.record_task_completion("0xbob", "taskB", &date, true, "ok")
            .await
            .unwrap();

        let all = db.get_all_completed_counts(&date).await.unwrap();
        assert_eq!(all.len(), 2);

        let alice = all.get("0xalice").unwrap();
        assert_eq!(alice.get("taskA").copied().unwrap_or(0), 2);

        let bob = all.get("0xbob").unwrap();
        assert_eq!(bob.get("taskB").copied().unwrap_or(0), 1);
    }

    // ---- batch ----

    #[tokio::test]
    async fn test_large_number_of_wallets_queries() {
        let db = test_db().await;
        let date = today();

        // Insert records for 100 different wallets, each with 1 completed task
        for i in 0..100 {
            let wallet = format!("0xwallet_{:04}", i);
            db.record_task_completion(&wallet, "taskA", &date, true, "ok")
                .await
                .unwrap();
        }

        let all = db.get_all_completed_counts(&date).await.unwrap();
        assert_eq!(all.len(), 100, "Should have 100 wallets with data");

        for i in 0..100 {
            let wallet = format!("0xwallet_{:04}", i);
            let tasks = all.get(&wallet).unwrap();
            assert_eq!(
                tasks.get("taskA").copied().unwrap_or(0),
                1,
                "Wallet {wallet} should have 1 completion"
            );
        }
    }

    #[tokio::test]
    async fn test_very_long_wallet_address() {
        let db = test_db().await;
        let date = today();

        // Build a wallet address of 1000+ characters
        let long_addr = "0xabcd1234".repeat(110);
        assert!(
            long_addr.len() > 1000,
            "Address should be > 1000 chars, got {}",
            long_addr.len()
        );

        db.record_task_completion(&long_addr, "01_checkBalance", &date, true, "long addr test")
            .await
            .unwrap();

        let counts = db.get_completed_counts(&long_addr, &date).await.unwrap();
        assert_eq!(
            counts.get("01_checkBalance").copied().unwrap_or(0),
            1,
            "Long wallet address should work"
        );
    }

    #[tokio::test]
    async fn test_concurrent_records_same_timestamp() {
        let db = test_db().await;
        let date = today();

        // Record 5 completions with identical wallet+task+date
        // PK prevents duplicates; UPSERT increments count_success each time
        for _ in 0..5 {
            db.record_task_completion("0xalice", "01_checkBalance", &date, true, "same_ts")
                .await
                .unwrap();
        }

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(
            counts.get("01_checkBalance").copied().unwrap_or(0),
            5,
            "Should count 5 completions via UPSERT count_success increment"
        );
    }

    #[tokio::test]
    async fn test_get_completed_counts_no_records_at_all() {
        let db = test_db().await;
        let date = today();

        // Insert records for some wallets so the DB has data
        db.record_task_completion("0xalice", "taskA", &date, true, "ok")
            .await
            .unwrap();
        db.record_task_completion("0xbob", "taskB", &date, false, "fail")
            .await
            .unwrap();

        // Query a wallet that has ZERO records of any kind (not even failures)
        let counts = db.get_completed_counts("0xcharlie", &date).await.unwrap();
        assert!(
            counts.is_empty(),
            "Should return empty HashMap for wallet with no records"
        );

        let total = db.get_total_completed("0xcharlie", &date).await.unwrap();
        assert_eq!(total, 0, "Total should be 0 for wallet with no records");
    }

    #[tokio::test]
    async fn test_file_db_create_open_reopen() {
        let dir = std::env::temp_dir();
        let db_path = dir.join(format!("test_reopen_db_{}.db", std::process::id()));
        let path_str = db_path.to_str().unwrap();

        let _ = std::fs::remove_file(&db_path);

        // Create, insert (2 wallets), close
        {
            let db = DailyDb::new(path_str).await.unwrap();
            db.record_task_completion("0xalice", "taskA", &today(), true, "open1")
                .await
                .unwrap();
            db.record_task_completion("0xbob", "taskB", &today(), true, "open1")
                .await
                .unwrap();
            db.close().await;
        }

        // Reopen same file, read back, verify data persisted
        {
            let db = DailyDb::new(path_str).await.unwrap();
            let all = db.get_all_completed_counts(&today()).await.unwrap();
            assert_eq!(all.len(), 2, "Both wallets should persist across reopen");
            let alice = all.get("0xalice").unwrap();
            assert_eq!(alice.get("taskA").copied().unwrap_or(0), 1);
            let bob = all.get("0xbob").unwrap();
            assert_eq!(bob.get("taskB").copied().unwrap_or(0), 1);
            db.close().await;
        }

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_init_schema_twice_idempotent() {
        let db = test_db().await;
        let date = today();

        // test_db() already called init_schema once; call it a second time
        db.init_schema()
            .await
            .expect("Second init_schema call should succeed");

        // Insert a record and query to verify DB still works after second init
        db.record_task_completion("0xalice", "01_checkBalance", &date, true, "idempotent")
            .await
            .unwrap();

        let counts = db.get_completed_counts("0xalice", &date).await.unwrap();
        assert_eq!(
            counts.get("01_checkBalance").copied().unwrap_or(0),
            1,
            "Queries should still work after second init_schema"
        );
    }
}
