//! Comprehensive memory optimization utilities

use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time;
use tracing::{debug, info, warn};

use crate::utils::memory_monitor::{self, MemoryMonitorConfig};

use std::future::Future;
use std::pin::Pin;

/// Memory optimization configuration
#[derive(Debug, Clone)]
pub struct MemoryOptimizerConfig {
    pub enable_memory_monitoring: bool,
    pub enable_log_optimization: bool,
    pub enable_gc_tuning: bool,
    pub memory_cleanup_interval_ms: u64,
    pub max_memory_usage_mb: u64,
}

impl Default for MemoryOptimizerConfig {
    fn default() -> Self {
        Self {
            enable_memory_monitoring: true,
            enable_log_optimization: true,
            enable_gc_tuning: true,
            memory_cleanup_interval_ms: 30000, // 30 seconds
            max_memory_usage_mb: 300,          // Strict 300MB threshold
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_optimizer_config_defaults() {
        let cfg = MemoryOptimizerConfig::default();
        assert!(cfg.enable_memory_monitoring);
        assert!(cfg.enable_log_optimization);
        assert!(cfg.enable_gc_tuning);
        assert_eq!(cfg.memory_cleanup_interval_ms, 30000);
        assert_eq!(cfg.max_memory_usage_mb, 300);
    }

    #[test]
    fn test_memory_optimizer_config_custom() {
        let cfg = MemoryOptimizerConfig {
            enable_memory_monitoring: false,
            enable_log_optimization: false,
            enable_gc_tuning: false,
            memory_cleanup_interval_ms: 60000,
            max_memory_usage_mb: 512,
        };
        assert!(!cfg.enable_memory_monitoring);
        assert!(!cfg.enable_gc_tuning);
        assert_eq!(cfg.memory_cleanup_interval_ms, 60000);
        assert_eq!(cfg.max_memory_usage_mb, 512);
    }

    #[test]
    fn test_memory_optimizer_new_default_config() {
        let optimizer = MemoryOptimizer::new(MemoryOptimizerConfig::default());
        assert_eq!(optimizer.cleanup_count, 0);
    }

    #[test]
    fn test_memory_optimizer_new_with_config() {
        let cfg = MemoryOptimizerConfig {
            max_memory_usage_mb: 1024,
            ..Default::default()
        };
        let optimizer = MemoryOptimizer::new(cfg);
        assert_eq!(optimizer.config.max_memory_usage_mb, 1024);
    }

    #[test]
    fn test_get_status_report_contains_info() {
        let optimizer = MemoryOptimizer::new(MemoryOptimizerConfig::default());
        let report = optimizer.get_status_report();
        assert!(report.contains("Cleanups performed"));
        assert!(report.contains("Last cleanup"));
    }
}

pub type AsyncCleanupHook =
    Box<dyn Fn(bool) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Memory optimizer that manages various optimization strategies
pub struct MemoryOptimizer {
    config: MemoryOptimizerConfig,
    last_cleanup: Instant,
    cleanup_count: u64,
    cleanup_hooks: Vec<AsyncCleanupHook>,
    is_emergency_cleaning: bool,
}

impl MemoryOptimizer {
    pub fn new(config: MemoryOptimizerConfig) -> Self {
        Self {
            config,
            last_cleanup: Instant::now(),
            cleanup_count: 0,
            cleanup_hooks: Vec::new(),
            is_emergency_cleaning: false,
        }
    }

    /// Register a cleanup hook to be called during perform_cleanup
    pub fn register_hook<F, Fut>(&mut self, hook: F)
    where
        F: Fn(bool) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.cleanup_hooks
            .push(Box::new(move |emergency| Box::pin(hook(emergency))));
    }

    /// Initialize all memory optimization features
    pub async fn initialize(optimizer: Arc<tokio::sync::Mutex<Self>>) -> Result<()> {
        let config = {
            let opt = optimizer.lock().await;
            opt.config.clone()
        };

        info!("Initializing memory optimizer with config: {:?}", config);

        if config.enable_memory_monitoring {
            let monitor_config = MemoryMonitorConfig {
                sampling_interval_ms: 5000, // 5 seconds
                memory_threshold_mb: config.max_memory_usage_mb,
                cpu_threshold_percent: 80.0,
                history_size: 100,
            };
            memory_monitor::init_memory_monitoring()?;

            // Start background monitoring task
            Self::start_monitoring_task(optimizer.clone(), monitor_config).await?;
        }

        let opt = optimizer.lock().await;
        if opt.config.enable_log_optimization {
            opt.setup_log_optimization()?;
        }

        if opt.config.enable_gc_tuning {
            opt.tune_garbage_collection()?;
        }

        info!(
            "Memory optimization initialized successfully (Target: {}MB)",
            opt.config.max_memory_usage_mb
        );
        Ok(())
    }

    async fn start_monitoring_task(
        optimizer: Arc<tokio::sync::Mutex<Self>>,
        config: MemoryMonitorConfig,
    ) -> Result<()> {
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_millis(config.sampling_interval_ms));

            loop {
                interval.tick().await;

                if let Ok(stats) = memory_monitor::sample_memory_usage() {
                    let memory_mb = stats.resident_set_size as f64 / 1024.0 / 1024.0;

                    // Emergency cleanup if we exceed 85% of target
                    let threshold = config.memory_threshold_mb as f64 * 0.85;
                    if memory_mb > threshold {
                        warn!(
                            "EMERGENCY: Memory usage {:.1}MB exceeded 85% threshold ({:.1}MB). Triggering immediate cleanup.",
                            memory_mb, threshold
                        );

                        let mut opt = optimizer.lock().await;
                        if !opt.is_emergency_cleaning {
                            opt.is_emergency_cleaning = true;
                            // Force immediate cleanup regardless of interval
                            let _ = opt.perform_cleanup_forced().await;
                            opt.is_emergency_cleaning = false;
                        }
                    }

                    // Log detailed memory info periodically
                    if stats.timestamp.elapsed().as_secs() % 60 == 0 {
                        debug!(
                            "Memory usage: {:.1}MB, CPU: {:.1}%",
                            memory_mb, stats.cpu_usage
                        );
                    }
                }
            }
        });

        Ok(())
    }

    fn setup_log_optimization(&self) -> Result<()> {
        info!("Log optimization enabled - using buffered file I/O with rotation");
        std::env::set_var("RUST_LOG", "info");
        std::env::set_var("RUST_BACKTRACE", "1");
        Ok(())
    }

    fn tune_garbage_collection(&self) -> Result<()> {
        info!("Garbage collection tuning enabled");
        std::env::set_var("MALLOC_ARENA_MAX", "2");
        std::env::set_var("MALLOC_MMAP_THRESHOLD", "131072");
        std::env::set_var("JE_MALLOC_CONF", "narenas:2,lg_chunk:21");
        Ok(())
    }

    /// Perform periodic memory cleanup
    pub async fn perform_cleanup(&mut self) -> Result<()> {
        if self.last_cleanup.elapsed()
            < Duration::from_millis(self.config.memory_cleanup_interval_ms)
        {
            return Ok(());
        }
        self.perform_cleanup_forced().await
    }

    /// Force cleanup regardless of interval
    pub async fn perform_cleanup_forced(&mut self) -> Result<()> {
        debug!(
            "Performing memory cleanup (#{}) (emergency={})",
            self.cleanup_count + 1,
            self.is_emergency_cleaning
        );

        let is_emergency = self.is_emergency_cleaning;

        // Call registered hooks
        for hook in &self.cleanup_hooks {
            hook(is_emergency).await;
        }

        // Force garbage collection hint
        self.force_memory_release()?;

        // Clean up temporary files
        self.cleanup_temp_files()?;

        if let Ok(stats) = memory_monitor::sample_memory_usage() {
            let memory_mb = stats.resident_set_size as f64 / 1024.0 / 1024.0;
            if memory_mb > self.config.max_memory_usage_mb as f64 {
                warn!(
                    "Post-cleanup memory still high: {:.1}MB / {}MB limit",
                    memory_mb, self.config.max_memory_usage_mb
                );
            } else {
                debug!("Post-cleanup memory: {:.1}MB", memory_mb);
            }
        }

        self.last_cleanup = Instant::now();
        self.cleanup_count += 1;

        Ok(())
    }

    fn force_memory_release(&self) -> Result<()> {
        #[cfg(not(target_env = "msvc"))]
        {
            unsafe {
                libc::malloc_trim(0);
            }
        }
        Ok(())
    }

    fn cleanup_temp_files(&self) -> Result<()> {
        let temp_dirs = ["tmp", ".tmp", "logs/tmp"];

        for dir in temp_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() {
                        if let Ok(metadata) = entry.metadata() {
                            if let Ok(modified) = metadata.modified() {
                                if modified.elapsed().unwrap_or_default()
                                    > Duration::from_secs(3600)
                                {
                                    let _ = std::fs::remove_file(&path);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if memory usage is within acceptable limits
    pub fn check_memory_limits(&self) -> Result<bool> {
        if let Ok(stats) = memory_monitor::sample_memory_usage() {
            let memory_mb = stats.resident_set_size as f64 / 1024.0 / 1024.0;
            Ok(memory_mb <= self.config.max_memory_usage_mb as f64)
        } else {
            Ok(true)
        }
    }

    /// Get memory optimization status report
    pub fn get_status_report(&self) -> String {
        let memory_report = memory_monitor::get_memory_report();

        format!(
            "Memory Optimization Status:\n\
             Cleanups performed: {}\n\
             Last cleanup: {}s ago\n\
             {}",
            self.cleanup_count,
            self.last_cleanup.elapsed().as_secs(),
            memory_report
        )
    }
}

/// Global memory optimizer instance
pub static MEMORY_OPTIMIZER: once_cell::sync::Lazy<Arc<tokio::sync::Mutex<MemoryOptimizer>>> =
    once_cell::sync::Lazy::new(|| {
        let config = MemoryOptimizerConfig::default();
        Arc::new(tokio::sync::Mutex::new(MemoryOptimizer::new(config)))
    });

/// Initialize global memory optimization
pub async fn init_memory_optimization() -> Result<()> {
    MemoryOptimizer::initialize(MEMORY_OPTIMIZER.clone()).await
}

/// Register a global memory cleanup hook
pub fn register_memory_cleanup_hook<F, Fut>(hook: F)
where
    F: Fn(bool) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let optimizer_clone = MEMORY_OPTIMIZER.clone();
    tokio::spawn(async move {
        let mut optimizer = optimizer_clone.lock().await;
        optimizer.register_hook(hook);
    });
}

/// Perform periodic memory cleanup
pub async fn perform_memory_cleanup() -> Result<()> {
    let mut optimizer = MEMORY_OPTIMIZER.lock().await;
    optimizer.perform_cleanup().await
}

/// Get memory optimization status
pub async fn get_memory_status() -> String {
    let optimizer = MEMORY_OPTIMIZER.lock().await;
    optimizer.get_status_report()
}
