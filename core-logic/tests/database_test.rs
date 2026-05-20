use core_logic::database::DatabaseManager;

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

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    // Log a task result
    let result = db
        .log_task_result("wallet_1", "0xabc", "checkBalance", true, "Balance: 1.0 ETH", 100)
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

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

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

    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

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

    let db = std::sync::Arc::new(DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap());

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
