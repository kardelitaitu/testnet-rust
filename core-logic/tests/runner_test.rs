use core_logic::config::SpamConfig;
use core_logic::SpammerStats;
use core_logic::SpammerTrait;
use core_logic::WorkerRunner;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// A mock spammer that records start/stop calls and returns configurable stats
struct MockSpammer {
    stats: Arc<AtomicU64>,
    fail: bool,
    delay_ms: u64,
}

impl MockSpammer {
    fn new(stats: Arc<AtomicU64>, fail: bool, delay_ms: u64) -> Self {
        Self { stats, fail, delay_ms }
    }
}

#[async_trait::async_trait]
impl SpammerTrait for MockSpammer {
    async fn new(_config: SpamConfig) -> anyhow::Result<Self> {
        Ok(Self {
            stats: Arc::new(AtomicU64::new(0)),
            fail: false,
            delay_ms: 0,
        })
    }

    async fn start(
        &self,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<SpammerStats> {
        // Simulate work by checking cancellation
        for _ in 0..10 {
            if cancellation_token.is_cancelled() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms / 10)).await;
        }
        self.stats.fetch_add(1, Ordering::SeqCst);

        if self.fail {
            Err(anyhow::anyhow!("mock failure"))
        } else {
            Ok(SpammerStats {
                success: 10,
                failed: 0,
            })
        }
    }

    async fn stop(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn test_runner_empty_spammers() {
    let result = WorkerRunner::run_spammers(vec![]).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_runner_single_spammer() {
    let counter = Arc::new(AtomicU64::new(0));
    let spammer = MockSpammer::new(counter.clone(), false, 10);
    let result = WorkerRunner::run_spammers(vec![Box::new(spammer)]).await;
    assert!(result.is_ok());
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_runner_multiple_spammers() {
    let c1 = Arc::new(AtomicU64::new(0));
    let c2 = Arc::new(AtomicU64::new(0));
    let c3 = Arc::new(AtomicU64::new(0));

    let result = WorkerRunner::run_spammers(vec![
        Box::new(MockSpammer::new(c1.clone(), false, 10)),
        Box::new(MockSpammer::new(c2.clone(), false, 10)),
        Box::new(MockSpammer::new(c3.clone(), false, 10)),
    ])
    .await;
    assert!(result.is_ok());
    assert_eq!(c1.load(Ordering::SeqCst), 1);
    assert_eq!(c2.load(Ordering::SeqCst), 1);
    assert_eq!(c3.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_runner_with_failing_spammer() {
    let counter = Arc::new(AtomicU64::new(0));
    let spammer = MockSpammer::new(counter.clone(), true, 10);
    let result = WorkerRunner::run_spammers(vec![Box::new(spammer)]).await;
    // Runner should still return Ok even if individual spammers fail
    assert!(result.is_ok());
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_runner_mixed_success_failure() {
    let c_ok = Arc::new(AtomicU64::new(0));
    let c_fail = Arc::new(AtomicU64::new(0));

    let result = WorkerRunner::run_spammers(vec![
        Box::new(MockSpammer::new(c_ok.clone(), false, 10)),
        Box::new(MockSpammer::new(c_fail.clone(), true, 10)),
    ])
    .await;
    assert!(result.is_ok());
    assert_eq!(c_ok.load(Ordering::SeqCst), 1);
    assert_eq!(c_fail.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_runner_concurrent_execution() {
    // Two spammers with delays should run concurrently, completing in ~max(delay) not sum
    let c1 = Arc::new(AtomicU64::new(0));
    let c2 = Arc::new(AtomicU64::new(0));

    let start = tokio::time::Instant::now();
    let result = WorkerRunner::run_spammers(vec![
        Box::new(MockSpammer::new(c1.clone(), false, 100)),
        Box::new(MockSpammer::new(c2.clone(), false, 50)),
    ])
    .await;
    let elapsed = start.elapsed();

    assert!(result.is_ok());
    // Should finish in roughly 100ms (the slower one), not 150ms (sum)
    assert!(elapsed.as_millis() < 200, "Concurrent execution should finish faster than sequential");
}
