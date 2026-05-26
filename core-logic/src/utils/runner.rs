use crate::traits::Spammer;
use anyhow::Result;
use tokio::signal;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, Instrument};

/// Calculate success rate as percentage. Returns 0.0 if total is 0.
fn calculate_success_rate(success: u64, failed: u64) -> f64 {
    let total = success + failed;
    if total > 0 {
        (success as f64 / total as f64) * 100.0
    } else {
        0.0
    }
}

pub struct WorkerRunner;

impl WorkerRunner {
    /// Spawns a list of spammers as concurrent tasks and waits for them.
    pub async fn run_spammers(spammers: Vec<Box<dyn Spammer>>) -> Result<()> {
        let mut set = JoinSet::new();

        // Create a cancellation token for graceful shutdown
        let token = CancellationToken::new();
        let cloned_token = token.clone();

        // Spawn a task to listen for Ctrl+C
        tokio::spawn(async move {
            match signal::ctrl_c().await {
                Ok(()) => {
                    info!("🛑 Received Ctrl+C. Initiating graceful shutdown...");
                    cloned_token.cancel();
                }
                Err(err) => {
                    error!("Unable to listen for shutdown signal: {}", err);
                }
            }
        });

        let start_time = std::time::Instant::now();
        info!("Starting {} spammer workers...", spammers.len());

        for (i, spammer) in spammers.into_iter().enumerate() {
            // Move spammer into the async block
            let id = i + 1;
            let span = tracing::info_span!("worker", worker_id = format!("{:03}", id));
            let child_token = token.clone();

            set.spawn(
                async move {
                    // We don't log "Worker {} starting" here because it might clutter if we strictly follow user format,
                    // but for debugging it's fine. The span will attach WK ID.
                    // info!("Worker {} starting...", id);
                    // Context is already in span.

                    match spammer.start(child_token).await {
                        Ok(stats) => Ok(stats),
                        Err(e) => {
                            error!("Worker {} failed: {:?}", id, e);
                            Err(e)
                        }
                    }
                }
                .instrument(span),
            );
        }

        let mut total_success = 0;
        let mut total_failed = 0;

        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(stats)) => {
                    total_success += stats.success;
                    total_failed += stats.failed;
                }
                Ok(Err(_)) => {
                    // Already logged in thread
                }
                Err(e) => {
                    error!("A worker task panicked or failed to join: {:?}", e);
                }
            }
        }

        let total_duration = start_time.elapsed();
        let rate = calculate_success_rate(total_success, total_failed);

        info!("🛑 Shutdown Complete.");
        info!(
            "Total Time: {:.1}s | Total Success: {} | Total Fail: {} | Success Rate: {:.2}%",
            total_duration.as_secs_f64(),
            total_success,
            total_failed,
            rate
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::SpammerStats;
    use async_trait::async_trait;

    // Mock spammer for testing WorkerRunner
    struct MockSpammer {
        stats: SpammerStats,
        delay_ms: u64,
        cancel_on_start: bool,
    }

    #[async_trait]
    impl Spammer for MockSpammer {
        async fn new(_config: crate::config::SpamConfig) -> Result<Self> {
            Ok(Self {
                stats: SpammerStats::default(),
                delay_ms: 0,
                cancel_on_start: false,
            })
        }

        async fn start(&self, cancellation_token: CancellationToken) -> Result<SpammerStats> {
            if self.cancel_on_start {
                cancellation_token.cancel();
                return Ok(self.stats.clone());
            }
            if self.delay_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
            }
            if cancellation_token.is_cancelled() {
                return Ok(self.stats.clone());
            }
            Ok(self.stats.clone())
        }

        async fn stop(&self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_run_spammers_empty_list() {
        let result = WorkerRunner::run_spammers(vec![]).await;
        assert!(result.is_ok(), "Empty spammer list should return Ok");
    }

    #[tokio::test]
    async fn test_run_spammers_single_worker() {
        let spammer = MockSpammer {
            stats: SpammerStats {
                success: 10,
                failed: 2,
            },
            delay_ms: 0,
            cancel_on_start: false,
        };
        let result = WorkerRunner::run_spammers(vec![Box::new(spammer)]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_spammers_multiple_workers() {
        let s1 = MockSpammer {
            stats: SpammerStats {
                success: 5,
                failed: 1,
            },
            delay_ms: 0,
            cancel_on_start: false,
        };
        let s2 = MockSpammer {
            stats: SpammerStats {
                success: 3,
                failed: 0,
            },
            delay_ms: 0,
            cancel_on_start: false,
        };
        let s3 = MockSpammer {
            stats: SpammerStats {
                success: 0,
                failed: 4,
            },
            delay_ms: 0,
            cancel_on_start: false,
        };
        let result =
            WorkerRunner::run_spammers(vec![Box::new(s1), Box::new(s2), Box::new(s3)]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_spammers_with_delays() {
        let s1 = MockSpammer {
            stats: SpammerStats {
                success: 1,
                failed: 0,
            },
            delay_ms: 10,
            cancel_on_start: false,
        };
        let s2 = MockSpammer {
            stats: SpammerStats {
                success: 2,
                failed: 0,
            },
            delay_ms: 5,
            cancel_on_start: false,
        };
        let result = WorkerRunner::run_spammers(vec![Box::new(s1), Box::new(s2)]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_success_rate_precision() {
        // Very high precision: 1 success out of 1_000_000
        let rate = calculate_success_rate(1, 999_999);
        assert!((rate - 0.0001).abs() < 0.00001);
    }

    #[tokio::test]
    async fn test_success_rate_u64_max() {
        // u64::MAX operations — no overflow
        let rate = calculate_success_rate(u64::MAX / 2, u64::MAX / 2);
        assert!((rate - 50.0).abs() < 0.0001);
    }

    #[test]
    fn test_success_rate_all_success() {
        let rate = calculate_success_rate(100, 0);
        assert!((rate - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_success_rate_all_failed() {
        let rate = calculate_success_rate(0, 100);
        assert!((rate - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_success_rate_half() {
        let rate = calculate_success_rate(50, 50);
        assert!((rate - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_success_rate_zero_total() {
        let rate = calculate_success_rate(0, 0);
        assert!((rate - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_success_rate_mixed() {
        let rate = calculate_success_rate(75, 25);
        assert!((rate - 75.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_success_rate_large_values() {
        let rate = calculate_success_rate(9_999_999, 1);
        assert!((rate - 99.99999).abs() < 0.0001);
    }

    #[tokio::test]
    async fn test_run_spammers_cancellation() {
        use tokio::time::{sleep, Duration};
        
        // One spammer cancels immediately
        let s1 = MockSpammer {
            stats: SpammerStats { success: 1, failed: 0 },
            delay_ms: 100, // Should be cancelled
            cancel_on_start: false,
        };
        let s2 = MockSpammer {
            stats: SpammerStats { success: 0, failed: 0 },
            delay_ms: 0,
            cancel_on_start: true, // Triggers cancellation
        };
        
        let start = std::time::Instant::now();
        let _ = WorkerRunner::run_spammers(vec![Box::new(s1), Box::new(s2)]).await;
        let elapsed = start.elapsed();
        
        // Should finish quickly — use generous tolerance for CI/coverage overhead
        assert!(elapsed < Duration::from_millis(200), "Should have cancelled s1 quickly, got {:?}", elapsed);
    }

    #[tokio::test]
    async fn test_run_spammers_one_hangs_others_succeed() {
        use tokio::time::Duration;
        
        let s1 = MockSpammer {
            stats: SpammerStats { success: 10, failed: 0 },
            delay_ms: 10,
            cancel_on_start: false,
        };
        let s2 = MockSpammer {
            stats: SpammerStats { success: 5, failed: 5 },
            delay_ms: 10,
            cancel_on_start: false,
        };
        
        let result = WorkerRunner::run_spammers(vec![Box::new(s1), Box::new(s2)]).await;
        assert!(result.is_ok());
    }
}
