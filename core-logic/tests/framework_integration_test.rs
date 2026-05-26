use core_logic::{
    AsyncDbConfig, DatabaseManager, FallbackStrategy, MetricsCollector, ProxyRateLimiter,
    QueuedTaskResult, SpammerStats, WorkerRunner, WalletManager, ChainType
};
use core_logic::traits::Spammer;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use std::time::Duration;

struct IntegrationSpammer {
    db: Arc<DatabaseManager>,
    wallet_mgr: Arc<WalletManager>,
    rate_limiter: Arc<ProxyRateLimiter>,
    id: String,
}

#[async_trait]
impl Spammer for IntegrationSpammer {
    async fn new(_config: core_logic::config::SpamConfig) -> Result<Self> {
        unimplemented!("Not used in integration test")
    }

    async fn start(&self, token: CancellationToken) -> Result<SpammerStats> {
        let mut stats = SpammerStats::default();
        
        for i in 0..5 {
            if token.is_cancelled() { break; }
            
            // 1. Simulate Rate Limiting
            let proxy = "http://proxy:8080";
            self.rate_limiter.wait_until_available(proxy).await;
            
            // 2. Simulate Wallet Loading (index i % count)
            let wallet_idx = i % self.wallet_mgr.count();
            let wallet = self.wallet_mgr.get_wallet_for_chain(wallet_idx, Some("pwd"), ChainType::Evm).await;
            let address = wallet.map(|w| w.evm_address.clone()).unwrap_or_else(|_| "unknown".to_string());
            
            // 3. Record Task
            let start = std::time::Instant::now();
            tokio::time::sleep(Duration::from_millis(10)).await;
            let duration = start.elapsed();
            
            MetricsCollector::global().record_task("integration_task", duration, true);
            stats.success += 1;
            
            // 4. Log to DB
            self.db.queue_task_result(QueuedTaskResult {
                worker_id: self.id.clone(),
                wallet_address: address,
                task_name: "integration_task".into(),
                success: true,
                message: "integrated".into(),
                duration_ms: duration.as_millis() as u64,
                timestamp: chrono::Utc::now().timestamp(),
            }).ok();
        }
        
        Ok(stats)
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn test_full_framework_lifecycle_integration() {
    let dir = std::env::temp_dir().join(format!("integration_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    
    // Setup DB
    let db_path = dir.join("integration.db");
    let db = Arc::new(DatabaseManager::new_with_async(
        db_path.to_str().unwrap(),
        AsyncDbConfig::default(),
        FallbackStrategy::Drop
    ).await.unwrap());
    
    // Setup Wallets (dummy files)
    let wallet_dir = dir.join("wallets");
    std::fs::create_dir_all(&wallet_dir).unwrap();
    for i in 0..2 {
        std::fs::write(wallet_dir.join(format!("w{}.json", i)), "{}").unwrap();
    }
    let wallet_mgr = Arc::new(WalletManager::with_wallet_dir(&wallet_dir).unwrap());
    
    // Setup Rate Limiter
    let rate_limiter = Arc::new(ProxyRateLimiter::new(100));
    
    // Build Spammers
    let s1 = IntegrationSpammer {
        db: db.clone(),
        wallet_mgr: wallet_mgr.clone(),
        rate_limiter: rate_limiter.clone(),
        id: "WK1".into(),
    };
    let s2 = IntegrationSpammer {
        db: db.clone(),
        wallet_mgr: wallet_mgr.clone(),
        rate_limiter: rate_limiter.clone(),
        id: "WK2".into(),
    };
    
    // Run via Runner
    let result = WorkerRunner::run_spammers(vec![Box::new(s1), Box::new(s2)]).await;
    assert!(result.is_ok());
    
    // Verify Metrics
    let snap = MetricsCollector::global().snapshot();
    assert!(snap.tasks.total >= 10);
    
    // Shutdown DB to flush
    // We must drop all clones of Arc<DatabaseManager> except one to use try_unwrap or take ownership
    let db_owned = Arc::try_unwrap(db).map_err(|_| "Failed to unwrap DB Arc").unwrap();
    db_owned.shutdown().await.unwrap();
    
    // Reopen DB to verify persistence (use standard new to check rows)
    let db_check = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();
    
    // Use get_transaction_count for specific wallets
    let count1 = db_check.get_transaction_count("unknown").await.unwrap();
    assert_eq!(count1, 10);
    
    let _ = std::fs::remove_dir_all(&dir);
}
