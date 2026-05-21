use core_logic::database::{DatabaseManager, DbMetricsSnapshot};
use core_logic::WalletManager;
use std::fs;
use std::path::Path;

/// Helper: create a minimal valid wallet JSON with properly-sized hex values
fn create_wallet_json(dir: &Path, name: &str) {
    let content = r#"{
        "encryption_type": "aes-256-gcm",
        "encrypted": {
            "ciphertext": "aa",
            "iv": "aaaaaaaaaaaaaaaaaaaaaaaa",
            "salt": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "tag": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }
    }"#;
    fs::write(dir.join(name), content).unwrap();
}

#[test]
fn test_wallet_manager_new_in_empty_dir() {
    let dir = std::env::temp_dir().join(format!("wallets_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mgr = WalletManager::with_wallet_dir(&dir).unwrap();
    assert_eq!(mgr.count(), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_wallet_manager_count_single_wallet() {
    let dir = std::env::temp_dir().join(format!("wallets_test_ct_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    create_wallet_json(&dir, "0001.json");
    let mgr = WalletManager::with_wallet_dir(&dir).unwrap();
    assert_eq!(mgr.count(), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_wallet_manager_count_multiple_wallets() {
    let dir = std::env::temp_dir().join(format!("wallets_test_multi_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for i in 1..=5 {
        create_wallet_json(&dir, &format!("{:04}.json", i));
    }
    fs::write(dir.join("readme.txt"), "not a wallet").unwrap();
    let mgr = WalletManager::with_wallet_dir(&dir).unwrap();
    assert_eq!(mgr.count(), 5);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_wallet_manager_list_wallets() {
    let dir = std::env::temp_dir().join(format!("wallets_test_list_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    create_wallet_json(&dir, "alice.json");
    create_wallet_json(&dir, "bob.json");
    create_wallet_json(&dir, "charlie.json");
    let mgr = WalletManager::with_wallet_dir(&dir).unwrap();
    let wallets = mgr.list_wallets();
    assert_eq!(wallets.len(), 3);
    let names: Vec<&str> = wallets.iter().map(|s| s.as_str()).collect();
    assert!(names.contains(&"alice.json"));
    assert!(names.contains(&"bob.json"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_wallet_manager_ignores_non_json_files() {
    let dir = std::env::temp_dir().join(format!("wallets_test_ignore_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    create_wallet_json(&dir, "0001.json");
    fs::write(dir.join("addresses.txt"), "0xabc").unwrap();
    fs::write(dir.join("notes.md"), "# wallet notes").unwrap();
    let mgr = WalletManager::with_wallet_dir(&dir).unwrap();
    assert_eq!(mgr.count(), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_wallet_manager_count_after_reload() {
    let dir = std::env::temp_dir().join(format!("wallets_test_reload_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    create_wallet_json(&dir, "0001.json");
    let mgr = WalletManager::with_wallet_dir(&dir).unwrap();
    assert_eq!(mgr.count(), 1);
    create_wallet_json(&dir, "0002.json");
    let count = mgr.count();
    assert!(count >= 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_wallet_manager_with_subdirectory() {
    let dir = std::env::temp_dir().join(format!("wallets_test_sub_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    create_wallet_json(&dir, "main.json");
    let mgr = WalletManager::with_wallet_dir(&dir).unwrap();
    assert_eq!(mgr.count(), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_wallet_manager_evm_key_extraction() {
    let dir = std::env::temp_dir().join(format!("wallets_test_evm_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    create_wallet_json(&dir, "0001.json");
    let mgr = WalletManager::with_wallet_dir(&dir).unwrap();
    assert_eq!(mgr.count(), 1);
    let result = mgr.get_wallet(0, Some("wrong")).await;
    assert!(result.is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_wallet_manager_get_wallet_out_of_range() {
    let dir = std::env::temp_dir().join(format!("wallets_test_oob_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let mgr = WalletManager::with_wallet_dir(&dir).unwrap();
    let result = mgr.get_wallet(0, Some("pass")).await;
    assert!(result.is_err());
    let _ = fs::remove_dir_all(&dir);
}

// ─── DatabaseManager metrics tests ───────────────────────

#[test]
fn test_db_metrics_snapshot_error_rate() {
    let snapshot = DbMetricsSnapshot {
        total_queries: 100,
        total_errors: 5,
        total_inserts: 80,
        total_selects: 20,
    };
    assert!((snapshot.error_rate() - 5.0).abs() < 0.001);
}

#[test]
fn test_db_metrics_snapshot_zero_queries() {
    let snapshot = DbMetricsSnapshot {
        total_queries: 0,
        total_errors: 0,
        total_inserts: 0,
        total_selects: 0,
    };
    assert_eq!(snapshot.error_rate(), 0.0);
}

#[tokio::test]
async fn test_database_get_metrics() {
    let dir = std::env::temp_dir().join(format!("db_metrics_test_{}", std::process::id()));
    let db_path = dir.join("test.db");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let db = DatabaseManager::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metrics = db.get_metrics();
    assert_eq!(metrics.total_queries, 0);
    assert_eq!(metrics.error_rate(), 0.0);

    let _ = fs::remove_dir_all(&dir);
}
