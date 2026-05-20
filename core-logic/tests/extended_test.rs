use core_logic::database::{DatabaseManager, TaskMetricBatchItem};
use core_logic::ChainBuilder;
use core_logic::ChainSpammer;
use core_logic::MemoryOptimizedLoggerConfig;

// ─── ChainBuilder extended ───────────────────────────────

#[test]
fn test_chain_builder_default() {
    let builder = ChainBuilder::new();
    let debug = format!("{:?}", builder);
    assert!(debug.contains("rpc_urls") || debug.contains("ChainBuilder"));
}

#[test]
fn test_chain_builder_with_rpc_urls() {
    let builder = ChainBuilder::new()
        .with_rpc_urls(vec!["https://rpc1.com".into(), "https://rpc2.com".into()]);
    let debug = format!("{:?}", builder);
    assert!(debug.contains("rpc1.com"));
}

#[test]
fn test_chain_builder_with_chain_id() {
    let builder = ChainBuilder::new().with_chain_id(137);
    let debug = format!("{:?}", builder);
    assert!(debug.contains("137"));
}

#[test]
fn test_chain_builder_with_tps() {
    let builder = ChainBuilder::new().with_chain_id(1).with_tps(50);
    let debug = format!("{:?}", builder);
    assert!(debug.contains("50"));
}

#[test]
fn test_chain_builder_build_evm() {
    let chain = ChainBuilder::new()
        .with_rpc_urls(vec!["https://eth.da".into()])
        .with_chain_id(21894)
        .with_tps(10)
        .build_evm();
    assert!(chain.is_ok());
    let evm = chain.unwrap();
    assert_eq!(evm.config().chain_id, 21894);
    assert_eq!(evm.config().target_tps, 10);
}

#[test]
fn test_chain_builder_build_evm_without_chain_id() {
    let chain = ChainBuilder::new()
        .with_rpc_urls(vec!["https://eth.da".into()])
        .build_evm();
    assert!(chain.is_ok());
    let evm = chain.unwrap();
    assert_eq!(evm.config().chain_id, 1);
}

#[test]
fn test_chain_builder_build_evm_without_rpc() {
    let chain = ChainBuilder::new().with_chain_id(5).build_evm();
    assert!(chain.is_ok());
    assert_eq!(chain.unwrap().config().target_tps, 10);
}

// ─── MemoryOptimizedLoggerConfig ─────────────────────────

#[test]
fn test_logger_config_defaults() {
    let config = MemoryOptimizedLoggerConfig::default();
    assert_eq!(config.max_file_size, 10 * 1024 * 1024);
    assert_eq!(config.max_files, 5);
    assert_eq!(config.flush_interval_ms, 1000);
    assert_eq!(config.buffer_size, 8 * 1024);
}

#[test]
fn test_logger_config_custom() {
    let config = MemoryOptimizedLoggerConfig {
        max_file_size: 1024,
        max_files: 2,
        flush_interval_ms: 500,
        buffer_size: 4096,
    };
    assert_eq!(config.max_file_size, 1024);
    assert_eq!(config.max_files, 2);
    assert_eq!(config.flush_interval_ms, 500);
    assert_eq!(config.buffer_size, 4096);
}

#[test]
fn test_logger_config_debug() {
    let config = MemoryOptimizedLoggerConfig::default();
    let debug = format!("{:?}", config);
    assert!(debug.contains("max_file_size"));
    assert!(debug.contains("max_files"));
}

// ─── Database extended ───────────────────────────────────

#[tokio::test]
async fn test_database_batch_logging() {
    let dir = std::env::temp_dir().join(format!("db_batch_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    let items = vec![
        TaskMetricBatchItem {
            worker_id: "w1".into(),
            wallet: "0xaaa".into(),
            task: "mint".into(),
            success: true,
            message: "ok".into(),
            duration_ms: 100,
        },
        TaskMetricBatchItem {
            worker_id: "w2".into(),
            wallet: "0xbbb".into(),
            task: "redeem".into(),
            success: false,
            message: "fail".into(),
            duration_ms: 200,
        },
    ];

    let count = db.batch_log_task_results(&items).await;
    assert!(count.is_ok());
    assert_eq!(count.unwrap(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_empty_batch() {
    let dir = std::env::temp_dir().join(format!("db_batch_empty_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    let count = db.batch_log_task_results(&[]).await;
    assert!(count.is_ok());
    assert_eq!(count.unwrap(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_transaction_count() {
    let dir = std::env::temp_dir().join(format!("db_tx_count_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    db.log_task_result("w1", "0xaaa", "mint", true, "ok", 100)
        .await
        .unwrap();
    db.log_task_result("w1", "0xaaa", "mint", true, "ok", 100)
        .await
        .unwrap();

    let count = db.get_transaction_count("0xaaa").await;
    assert!(count.is_ok());
    assert_eq!(count.unwrap(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_success_count() {
    let dir = std::env::temp_dir().join(format!("db_success_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    db.log_task_result("w1", "0xaaa", "mint", true, "ok", 100)
        .await
        .unwrap();
    db.log_task_result("w1", "0xaaa", "mint", false, "fail", 200)
        .await
        .unwrap();
    db.log_task_result("w1", "0xaaa", "mint", true, "ok", 100)
        .await
        .unwrap();

    let success = db.get_success_count("0xaaa").await;
    assert!(success.is_ok());
    assert_eq!(success.unwrap(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_has_task_succeeded() {
    let dir = std::env::temp_dir().join(format!("db_has_success_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    db.log_task_result("w1", "0xaaa", "mint", true, "ok", 100)
        .await
        .unwrap();

    let result = db.has_task_succeeded("0xaaa", "mint").await;
    assert!(result.is_ok());
    assert!(result.unwrap());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_asset_creation_and_query() {
    let dir = std::env::temp_dir().join(format!("db_asset_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    db.log_asset_creation("w1", "0xaaa", "Counter", "0x1234", "CounterV1")
        .await
        .unwrap();

    let assets = db.get_assets_by_type("w1", "Counter").await;
    assert!(assets.is_ok());
    let assets = assets.unwrap();
    assert_eq!(assets.len(), 1);
    assert!(assets[0].contains("0xaaa"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_counter_deployment() {
    let dir = std::env::temp_dir().join(format!("db_counter_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    db.log_counter_contract_creation("w1", "0xdead", 21894)
        .await
        .unwrap();

    let contracts = db.get_deployed_counter_contracts("w1", 21894).await;
    assert!(contracts.is_ok());
    let contracts = contracts.unwrap();
    assert_eq!(contracts.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_proxy_stats() {
    let dir = std::env::temp_dir().join(format!("db_proxy_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    db.update_proxy_stats("http://proxy1:8080", true)
        .await
        .unwrap();
    db.update_proxy_stats("http://proxy1:8080", false)
        .await
        .unwrap();

    let _ = std::fs::remove_dir_all(&dir);
}