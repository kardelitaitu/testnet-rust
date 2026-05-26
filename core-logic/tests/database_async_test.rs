use core_logic::{AsyncDbConfig, DatabaseManager, FallbackStrategy, QueuedTaskResult};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_database_async_logging_flushes_on_batch_size() {
    let dir = std::env::temp_dir().join(format!("core_db_async_batch_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    
    // Config: small batch size, long interval
    let config = AsyncDbConfig {
        channel_capacity: 100,
        batch_size: 5,
        flush_interval_ms: 5000, 
    };
    
    let db = DatabaseManager::new_with_async(
        db_path.to_str().unwrap(),
        config,
        FallbackStrategy::Drop
    ).await.unwrap();

    // Queue 4 results (below batch size)
    for i in 0..4 {
        db.queue_task_result(QueuedTaskResult {
            worker_id: "WK".into(),
            wallet_address: "0xalice".into(),
            task_name: "test".into(),
            success: true,
            message: format!("msg {}", i),
            duration_ms: 10,
            timestamp: chrono::Utc::now().timestamp(),
        }).unwrap();
    }

    // Wait a bit - should NOT be flushed yet (interval is 5s)
    sleep(Duration::from_millis(200)).await;
    assert_eq!(db.get_transaction_count("0xalice").await.unwrap(), 0);

    // Queue 1 more (hits batch size 5)
    db.queue_task_result(QueuedTaskResult {
        worker_id: "WK".into(),
        wallet_address: "0xalice".into(),
        task_name: "test".into(),
        success: true,
        message: "msg 4".into(),
        duration_ms: 10,
        timestamp: chrono::Utc::now().timestamp(),
    }).unwrap();

    // Wait a bit - SHOULD be flushed now
    sleep(Duration::from_millis(300)).await;
    assert_eq!(db.get_transaction_count("0xalice").await.unwrap(), 5);

    db.shutdown().await.unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_async_logging_flushes_on_interval() {
    let dir = std::env::temp_dir().join(format!("core_db_async_interval_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    
    // Config: large batch size, short interval
    let config = AsyncDbConfig {
        channel_capacity: 100,
        batch_size: 100,
        flush_interval_ms: 200, 
    };
    
    let db = DatabaseManager::new_with_async(
        db_path.to_str().unwrap(),
        config,
        FallbackStrategy::Drop
    ).await.unwrap();

    // Queue 1 result
    db.queue_task_result(QueuedTaskResult {
        worker_id: "WK".into(),
        wallet_address: "0xalice".into(),
        task_name: "test".into(),
        success: true,
        message: "interval test".into(),
        duration_ms: 10,
        timestamp: chrono::Utc::now().timestamp(),
    }).unwrap();

    // Wait for interval
    sleep(Duration::from_millis(500)).await;
    assert_eq!(db.get_transaction_count("0xalice").await.unwrap(), 1);

    db.shutdown().await.unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_async_shutdown_flushes_pending() {
    let dir = std::env::temp_dir().join(format!("core_db_async_shutdown_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    
    // Config: large batch size, long interval
    let config = AsyncDbConfig {
        channel_capacity: 100,
        batch_size: 100,
        flush_interval_ms: 10000, 
    };
    
    let db = DatabaseManager::new_with_async(
        db_path.to_str().unwrap(),
        config,
        FallbackStrategy::Drop
    ).await.unwrap();

    // Queue 1 result
    db.queue_task_result(QueuedTaskResult {
        worker_id: "WK".into(),
        wallet_address: "0xalice".into(),
        task_name: "test".into(),
        success: true,
        message: "shutdown test".into(),
        duration_ms: 10,
        timestamp: chrono::Utc::now().timestamp(),
    }).unwrap();

    // Shutdown immediately
    db.shutdown().await.unwrap();
    
    // Reopen to verify
    let db2 = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();
    assert_eq!(db2.get_transaction_count("0xalice").await.unwrap(), 1);
    
    db2.shutdown().await.unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_async_fallback_drop() {
    let dir = std::env::temp_dir().join(format!("core_db_fallback_drop_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    
    // Config: tiny channel capacity
    let config = AsyncDbConfig {
        channel_capacity: 1,
        batch_size: 100,
        flush_interval_ms: 10000, 
    };
    
    let db = DatabaseManager::new_with_async(
        db_path.to_str().unwrap(),
        config,
        FallbackStrategy::Drop
    ).await.unwrap();

    // Fill channel (1 item)
    db.queue_task_result(QueuedTaskResult {
        worker_id: "WK".into(),
        wallet_address: "0xalice".into(),
        task_name: "test".into(),
        success: true,
        message: "msg 1".into(),
        duration_ms: 10,
        timestamp: 12345,
    }).unwrap();

    // Next one should be dropped but NOT return error
    let result = db.queue_task_result(QueuedTaskResult {
        worker_id: "WK".into(),
        wallet_address: "0xalice".into(),
        task_name: "test".into(),
        success: true,
        message: "msg 2".into(),
        duration_ms: 10,
        timestamp: 12346,
    });
    
    assert!(result.is_ok(), "Drop strategy should return Ok even if full");
    
    let (_, dropped) = db.get_async_metrics();
    assert_eq!(dropped, 1);

    db.shutdown().await.unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_async_fallback_hybrid() {
    let dir = std::env::temp_dir().join(format!("core_db_fallback_hybrid_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    
    let config = AsyncDbConfig {
        channel_capacity: 1,
        batch_size: 100,
        flush_interval_ms: 10000, 
    };
    
    let db = DatabaseManager::new_with_async(
        db_path.to_str().unwrap(),
        config,
        FallbackStrategy::Hybrid
    ).await.unwrap();

    // Fill channel
    db.queue_task_result(QueuedTaskResult {
        worker_id: "WK".into(),
        wallet_address: "0xalice".into(),
        task_name: "test".into(),
        success: true,
        message: "msg 1".into(),
        duration_ms: 10,
        timestamp: 12345,
    }).unwrap();

    // Next one should be dropped but NOT return error
    let result = db.queue_task_result(QueuedTaskResult {
        worker_id: "WK".into(),
        wallet_address: "0xalice".into(),
        task_name: "test".into(),
        success: true,
        message: "msg 2".into(),
        duration_ms: 10,
        timestamp: 12346,
    });
    
    assert!(result.is_ok(), "Hybrid strategy should return Ok even if full");
    
    let (_, dropped) = db.get_async_metrics();
    assert_eq!(dropped, 1);

    db.shutdown().await.unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_is_async_flag() {
    let dir = std::env::temp_dir().join(format!("core_db_async_flag_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    
    // Test standard new
    let db_sync = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();
    assert!(!db_sync.is_async());
    assert!(db_sync.get_async_config().is_none());
    db_sync.shutdown().await.unwrap();

    // Test async new
    let config = AsyncDbConfig::default();
    let db_async = DatabaseManager::new_with_async(
        db_path.to_str().unwrap(),
        config,
        FallbackStrategy::Drop
    ).await.unwrap();
    assert!(db_async.is_async());
    assert!(db_async.get_async_config().is_some());
    
    // Check initial async metrics
    let (queued, dropped) = db_async.get_async_metrics();
    assert_eq!(queued, 0);
    assert_eq!(dropped, 0);

    db_async.shutdown().await.unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
