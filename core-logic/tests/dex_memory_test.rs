use core_logic::database::{DatabaseManager, DexOrder};
use core_logic::{
    get_memory_status, init_memory_optimization, perform_memory_cleanup, register_memory_cleanup_hook,
    MemoryOptimizer, MemoryOptimizerConfig,
};

// ─── Database DEX operations ─────────────────────────────

#[tokio::test]
async fn test_database_dex_order_log_and_query() {
    let dir = std::env::temp_dir().join(format!("db_dex_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    db.log_dex_order("w1", "order_001", "USDC", "USDT", "100", true, 100i16, "0xtx1")
        .await
        .unwrap();
    db.log_dex_order("w1", "order_002", "USDT", "USDC", "50", false, -50i16, "0xtx2")
        .await
        .unwrap();

    let orders = db.get_active_orders("w1").await;
    assert!(orders.is_ok());
    let orders = orders.unwrap();
    assert_eq!(orders.len(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_dex_order_update_status() {
    let dir = std::env::temp_dir().join(format!("db_dex_upd_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    db.log_dex_order("w1", "order_001", "USDC", "USDT", "100", true, 100i16, "0xtx1")
        .await
        .unwrap();

    let orders = db.get_active_orders("w1").await.unwrap();
    assert_eq!(orders.len(), 1);

    db.update_order_status("order_001", "FILLED").await.unwrap();

    let orders_after = db.get_active_orders("w1").await.unwrap();
    assert_eq!(orders_after.len(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_dex_order_debug() {
    let order = DexOrder {
        id: 1,
        wallet_address: "w1".into(),
        order_id: "ord1".into(),
        base_token: "USDC".into(),
        quote_token: "USDT".into(),
        amount: "100".into(),
        is_bid: 1,
        tick: 100,
        tx_hash: "0xtx".into(),
        status: "ACTIVE".into(),
        timestamp: 1234567890,
    };
    let debug = format!("{:?}", order);
    assert!(debug.contains("USDC"));
    assert!(debug.contains("ACTIVE"));
}

// ─── Database getAll queries ─────────────────────────────

#[tokio::test]
async fn test_database_get_all_assets_by_type() {
    let dir = std::env::temp_dir().join(format!("db_all_asset_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    db.log_asset_creation("w1", "0xaaa", "Counter", "CounterV1", "CTR1")
        .await
        .unwrap();
    db.log_asset_creation("w2", "0xbbb", "Counter", "CounterV1", "CTR2")
        .await
        .unwrap();

    let all = db.get_all_assets_by_type("Counter").await;
    assert!(all.is_ok());
    let all = all.unwrap();
    assert_eq!(all.len(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_get_all_deployed_counter_contracts() {
    let dir = std::env::temp_dir().join(format!("db_all_ctr_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    db.log_counter_contract_creation("w1", "0x1111", 21894)
        .await
        .unwrap();
    db.log_counter_contract_creation("w2", "0x2222", 21894)
        .await
        .unwrap();
    db.log_counter_contract_creation("w3", "0x3333", 137)
        .await
        .unwrap();

    let on_da = db.get_all_deployed_counter_contracts(21894).await;
    assert!(on_da.is_ok());
    assert_eq!(on_da.unwrap().len(), 2);

    let on_poly = db.get_all_deployed_counter_contracts(137).await;
    assert!(on_poly.is_ok());
    assert_eq!(on_poly.unwrap().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_get_all_deployed_counter_with_wallets() {
    let dir = std::env::temp_dir().join(format!("db_all_ctr_w_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    db.log_counter_contract_creation("w1", "0x1111", 21894)
        .await
        .unwrap();
    db.log_counter_contract_creation("w2", "0x2222", 21894)
        .await
        .unwrap();

    let result = db.get_all_deployed_counter_contracts_with_wallets(21894).await;
    assert!(result.is_ok());
    let entries = result.unwrap();
    assert_eq!(entries.len(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_get_latest_asset_by_type() {
    let dir = std::env::temp_dir().join(format!("db_latest_asset_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    db.log_asset_creation("w1", "0xaaa", "MEME", "MemeCoin", "MEME")
        .await
        .unwrap();
    db.log_asset_creation("w1", "0xbbb", "MEME", "NewMeme", "NMME")
        .await
        .unwrap();

    let latest = db.get_latest_asset_by_type("w1", "MEME").await;
    assert!(latest.is_ok());
    let latest = latest.unwrap();
    assert!(latest.is_some());
    assert_eq!(latest.unwrap(), "0xbbb");

    let _ = std::fs::remove_dir_all(&dir);
}

// ─── MemoryOptimizer operations ──────────────────────────

#[tokio::test]
async fn test_memory_optimizer_init() {
    let result = init_memory_optimization().await;
    let _ = result;
}

#[tokio::test]
async fn test_memory_optimizer_get_memory_status() {
    let status = get_memory_status().await;
    assert!(!status.is_empty());
}

#[tokio::test]
async fn test_memory_optimizer_perform_cleanup() {
    let _ = perform_memory_cleanup().await;
}

#[tokio::test]
async fn test_memory_optimizer_register_hook() {
    register_memory_cleanup_hook(|_is_emergency| {
        Box::pin(async move {})
    });
}

#[test]
fn test_memory_optimizer_custom_config() {
    let config = MemoryOptimizerConfig {
        enable_memory_monitoring: false,
        enable_log_optimization: false,
        enable_gc_tuning: false,
        memory_cleanup_interval_ms: 10000,
        max_memory_usage_mb: 500,
    };
    let opt = MemoryOptimizer::new(config);
    let report = opt.get_status_report();
    assert!(!report.is_empty());
}
