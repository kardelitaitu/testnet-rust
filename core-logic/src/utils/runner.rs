use crate::traits::Spammer;
use anyhow::Result;
use tokio::signal;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, Instrument};

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
        let total = total_success + total_failed;
        let rate = if total > 0 {
            (total_success as f64 / total as f64) * 100.0
        } else {
            0.0
        };

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
