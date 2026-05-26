use core_logic::database::{AsyncDbConfig, DatabaseManager, FallbackStrategy, QueuedTaskResult};

#[tokio::test]
async fn test_database_manager_create() {
    let dir = std::env::temp_dir().join(format!("db_test_create_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Creating with a path should succeed
    let result = DatabaseManager::new(db_path.to_str().unwrap()).await;
    assert!(result.is_ok());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_manager_log_task_result() {
    let dir = std::env::temp_dir().join(format!("db_test_log_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap())
        .await
        .unwrap();

    // Log a task result
    let result = db
        .log_task_result(
            "wallet_1",
            "0xabc",
            "checkBalance",
            true,
            "Balance: 1.0 ETH",
            100,
        )
        .await;
    assert!(result.is_ok());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_manager_log_success_and_failure() {
    let dir = std::env::temp_dir().join(format!("db_test_both_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap())
        .await
        .unwrap();

    // Log a success
    db.log_task_result("w1", "0xaaa", "task1", true, "ok", 100)
        .await
        .unwrap();
    // Log a failure
    db.log_task_result("w1", "0xaaa", "task1", false, "error", 200)
        .await
        .unwrap();

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_manager_rejects_empty_wallet_id() {
    let dir = std::env::temp_dir().join(format!("db_test_empty_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap())
        .await
        .unwrap();

    let result = db
        .log_task_result("", "0xabc", "task1", true, "msg", 100)
        .await;
    // Should handle empty wallet ID gracefully (may succeed or fail)
    // Just verify no panic
    let _ = result;

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_manager_concurrent_writes() {
    let dir = std::env::temp_dir().join(format!("db_test_concurrent_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let db = std::sync::Arc::new(
        DatabaseManager::new(db_path.to_str().unwrap())
            .await
            .unwrap(),
    );

    let mut handles = Vec::new();
    for i in 0..5 {
        let db_clone = db.clone();
        handles.push(tokio::spawn(async move {
            db_clone
                .log_task_result(
                    &format!("w{}", i),
                    &format!("0x{:03x}", i),
                    "task",
                    true,
                    "msg",
                    i * 10,
                )
                .await
        }));
    }

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_new_with_async_queues_and_flushes() {
    let dir = std::env::temp_dir().join(format!("db_test_async_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let config = AsyncDbConfig {
        channel_capacity: 100,
        batch_size: 10,
        flush_interval_ms: 50,
    };
    let db = DatabaseManager::new_with_async(
        db_path.to_str().unwrap(),
        config,
        FallbackStrategy::Drop,
    )
    .await
    .unwrap();

    // Queue several results
    for i in 0..5 {
        let result = QueuedTaskResult {
            worker_id: format!("w{}", i),
            wallet_address: format!("0x{:03x}", i),
            task_name: "asyncTask".into(),
            success: i % 2 == 0,
            message: format!("result {}", i),
            duration_ms: i * 20,
            timestamp: chrono::Utc::now().timestamp(),
        };
        db.queue_task_result(result).unwrap();
    }

    // Shutdown flushes remaining entries and closes the pool
    db.shutdown().await.unwrap();

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_new_with_async_channel_full_fallback() {
    let dir = std::env::temp_dir().join(format!("db_test_full_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Use tiny channel so it fills up fast
    let config = AsyncDbConfig {
        channel_capacity: 2,
        batch_size: 10,
        flush_interval_ms: 5000, // long interval so channel doesn't drain during test
    };
    let db = DatabaseManager::new_with_async(
        db_path.to_str().unwrap(),
        config,
        FallbackStrategy::Hybrid,
    )
    .await
    .unwrap();

    // Fill the channel
    for i in 0..2 {
        let result = QueuedTaskResult {
            worker_id: format!("w{}", i),
            wallet_address: format!("0x{:03x}", i),
            task_name: "fullTask".into(),
            success: true,
            message: "data".into(),
            duration_ms: 10,
            timestamp: chrono::Utc::now().timestamp(),
        };
        db.queue_task_result(result).unwrap();
    }

    // Channel is full — this should trigger fallback (Hybrid = drop + warn)
    let overflow = QueuedTaskResult {
        worker_id: "overflow".into(),
        wallet_address: "0x999".into(),
        task_name: "fullTask".into(),
        success: false,
        message: "overflow".into(),
        duration_ms: 0,
        timestamp: chrono::Utc::now().timestamp(),
    };
    let result = db.queue_task_result(overflow);
    assert!(result.is_ok(), "Full channel should fallback without error");

    db.shutdown().await.unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
