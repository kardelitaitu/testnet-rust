use core_logic::database::{
    AsyncDbConfig, DatabaseManager, DbMetrics, DbMetricsSnapshot, FallbackStrategy, QueuedTaskResult,
};

// ─── Database config/constants ──────────────────────────

#[test]
fn test_database_manager_constants() {
    assert_eq!(DatabaseManager::DEFAULT_MAX_CONNECTIONS, 20);
    assert_eq!(DatabaseManager::DEFAULT_TIMEOUT_MS, 30000);
}

#[test]
fn test_async_db_config_defaults() {
    let config = AsyncDbConfig::default();
    assert_eq!(config.channel_capacity, 1000);
    assert_eq!(config.batch_size, 200);
    assert_eq!(config.flush_interval_ms, 200);
}

#[test]
fn test_async_db_config_custom() {
    let config = AsyncDbConfig {
        channel_capacity: 500,
        batch_size: 50,
        flush_interval_ms: 500,
    };
    assert_eq!(config.channel_capacity, 500);
}

#[test]
fn test_fallback_strategy_variants() {
    // Verify all variants can be constructed and matched
    let drop_s = FallbackStrategy::Drop;
    let sync_s = FallbackStrategy::Sync;
    let hybrid_s = FallbackStrategy::Hybrid;

    match drop_s {
        FallbackStrategy::Drop => {}
        _ => panic!("expected Drop"),
    }
    match sync_s {
        FallbackStrategy::Sync => {}
        _ => panic!("expected Sync"),
    }
    match hybrid_s {
        FallbackStrategy::Hybrid => {}
        _ => panic!("expected Hybrid"),
    }
}

#[test]
fn test_queued_task_result_debug() {
    let result = QueuedTaskResult {
        worker_id: "w1".into(),
        wallet_address: "0xabc".into(),
        task_name: "mint".into(),
        success: true,
        message: "ok".into(),
        duration_ms: 100,
        timestamp: 1234567890,
    };
    assert_eq!(result.worker_id, "w1");
    assert!(result.success);
    let debug = format!("{:?}", result);
    assert!(debug.contains("mint"));
    assert!(debug.contains("ok"));
}

#[test]
fn test_queued_task_result_failure() {
    let result = QueuedTaskResult {
        worker_id: "w2".into(),
        wallet_address: "0xdef".into(),
        task_name: "redeem".into(),
        success: false,
        message: "fail".into(),
        duration_ms: 200,
        timestamp: 0,
    };
    assert!(!result.success);
    let debug = format!("{:?}", result);
    assert!(debug.contains("redeem"));
    assert!(debug.contains("fail"));
}

#[test]
fn test_db_metrics_debug() {
    let metrics = DbMetrics::default();
    let debug = format!("{:?}", metrics);
    assert!(debug.contains("total_queries"));
}

#[test]
fn test_db_metrics_snapshot_error_rate() {
    let snapshot = DbMetricsSnapshot {
        total_queries: 100,
        total_errors: 5,
        total_inserts: 80,
        total_selects: 20,
    };
    assert_eq!(snapshot.total_queries, 100);
    assert!((snapshot.error_rate() - 5.0).abs() < 0.001);
}

#[test]
fn test_db_metrics_snapshot_zero_error_rate() {
    let snapshot = DbMetricsSnapshot {
        total_queries: 0,
        total_errors: 0,
        total_inserts: 0,
        total_selects: 0,
    };
    assert_eq!(snapshot.error_rate(), 0.0);
}

// ─── MemoryMonitor init ─────────────────────────────────

#[test]
fn test_init_memory_monitoring() {
    let result = core_logic::init_memory_monitoring();
    let _ = result;
}

// ─── Retry utils ────────────────────────────────────────

#[test]
fn test_retry_config_construction() {
    let config = core_logic::RetryConfig::new(3, 100);
    let no_jitter = config.without_jitter();
    // Just verify no panic
    let _ = no_jitter;
}

// ─── Database queries on empty DB ───────────────────────

#[tokio::test]
async fn test_database_manager_new_with_async_config() {
    let dir = std::env::temp_dir().join(format!("db_edge_cfg_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();
    let count = db.get_transaction_count("nonexistent").await;
    assert!(count.is_ok());
    assert_eq!(count.unwrap(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_manager_counts_initially_zero() {
    let dir = std::env::temp_dir().join(format!("db_edge_zero_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    assert_eq!(db.get_transaction_count("w1").await.unwrap(), 0);
    assert_eq!(db.get_success_count("w1").await.unwrap(), 0);

    let has = db.has_task_succeeded("w1", "task").await;
    assert!(has.is_ok());
    assert!(!has.unwrap());

    let _ = std::fs::remove_dir_all(&dir);
}
