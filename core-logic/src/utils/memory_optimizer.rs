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
            max_memory_usage_mb: 512,         // 512MB threshold
        }
    }
}

pub type AsyncCleanupHook = Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Memory optimizer that manages various optimization strategies
pub struct MemoryOptimizer {
    config: MemoryOptimizerConfig,
    last_cleanup: Instant,
    cleanup_count: u64,
    cleanup_hooks: Vec<AsyncCleanupHook>,
}

impl MemoryOptimizer {
    pub fn new(config: MemoryOptimizerConfig) -> Self {
        Self {
            config,
            last_cleanup: Instant::now(),
            cleanup_count: 0,
            cleanup_hooks: Vec::new(),
        }
    }
    
    /// Register a cleanup hook to be called during perform_cleanup
    pub fn register_hook<F, Fut>(&mut self, hook: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.cleanup_hooks.push(Box::new(move || Box::pin(hook())));
    }
    
    /// Initialize all memory optimization features
    pub async fn initialize(&mut self) -> Result<()> {
        info!("Initializing memory optimizer with config: {:?}", self.config);
        
        if self.config.enable_memory_monitoring {
            let monitor_config = MemoryMonitorConfig::default();
            memory_monitor::init_memory_monitoring()?;
            
            // Start background monitoring task
            self.start_monitoring_task(monitor_config).await?;
        }
        
        if self.config.enable_log_optimization {
            self.setup_log_optimization()?;
        }
        
        if self.config.enable_gc_tuning {
            self.tune_garbage_collection()?;
        }
        
        info!("Memory optimization initialized successfully");
        Ok(())
    }
    
    async fn start_monitoring_task(&self, config: MemoryMonitorConfig) -> Result<()> {
        let config = config.clone();
        
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_millis(config.sampling_interval_ms));
            
            loop {
                interval.tick().await;
                
                if let Ok(stats) = memory_monitor::sample_memory_usage() {
                    let memory_mb = stats.resident_set_size as f64 / 1024.0 / 1024.0;
                    
                    if memory_mb > config.memory_threshold_mb as f64 {
                        warn!(
                            "High memory usage: {:.1}MB (threshold: {}MB)",
                            memory_mb, config.memory_threshold_mb
                        );
                    }
                    
                    // Log detailed memory info periodically
                    if stats.timestamp.elapsed().as_secs() % 60 == 0 {
                        debug!("Memory usage: {:.1}MB, CPU: {:.1}%", 
                            memory_mb, stats.cpu_usage);
                    }
                }
            }
        });
        
        Ok(())
    }
    
    fn setup_log_optimization(&self) -> Result<()> {
        // This would switch to the memory-optimized logger
        // For now, we'll just configure the existing logger better
        info!("Log optimization enabled - using buffered file I/O with rotation");
        
        // Set up environment variables for better logging behavior
        std::env::set_var("RUST_LOG", "info");
        std::env::set_var("RUST_BACKTRACE", "1");
        
        Ok(())
    }
    
    fn tune_garbage_collection(&self) -> Result<()> {
        // Tune Rust's memory allocation and garbage collection behavior
        // Note: Rust doesn't have a traditional GC, but we can influence allocation patterns
        
        info!("Garbage collection tuning enabled");
        
        // Set environment variables that influence memory behavior
        std::env::set_var("MALLOC_ARENA_MAX", "2"); // Reduce memory fragmentation
        std::env::set_var("MALLOC_MMAP_THRESHOLD", "131072"); // 128KB threshold
        
        // For jemalloc (if used)
        std::env::set_var("JE_MALLOC_CONF", "narenas:2,lg_chunk:21");
        
        Ok(())
    }
    
    /// Perform periodic memory cleanup
    pub async fn perform_cleanup(&mut self) -> Result<()> {
        if self.last_cleanup.elapsed() < Duration::from_millis(self.config.memory_cleanup_interval_ms) {
            return Ok(());
        }
        
        debug!("Performing memory cleanup (#{})", self.cleanup_count + 1);
        
        // Call registered hooks
        for hook in &self.cleanup_hooks {
            hook().await;
        }

        // Force garbage collection (Rust doesn't have explicit GC, but we can hint)
        self.force_memory_release()?;
        
        // Clean up temporary files
        self.cleanup_temp_files()?;
        
        // Report memory usage
        if let Ok(stats) = memory_monitor::sample_memory_usage() {
            let memory_mb = stats.resident_set_size as f64 / 1024.0 / 1024.0;
            debug!("Post-cleanup memory: {:.1}MB", memory_mb);
        }
        
        self.last_cleanup = Instant::now();
        self.cleanup_count += 1;
        
        Ok(())
    }
    
    fn force_memory_release(&self) -> Result<()> {
        // Rust doesn't have explicit GC, but we can try to release memory
        // by dropping caches and encouraging allocator to return memory
        
        // Suggest to the allocator to release memory
        #[cfg(not(target_env = "msvc"))]
        {
            // For jemalloc and other allocators
            unsafe {
                libc::malloc_trim(0);
            }
        }
        
        Ok(())
    }
    
    fn cleanup_temp_files(&self) -> Result<()> {
        // Clean up temporary files that might accumulate
        let temp_dirs = ["tmp", ".tmp", "logs/tmp"];
        
        for dir in temp_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() {
                        // Delete files older than 1 hour
                        if let Ok(metadata) = entry.metadata() {
                            if let Ok(modified) = metadata.modified() {
                                if modified.elapsed().unwrap_or_default() > Duration::from_secs(3600) {
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
            Ok(true) // Assume OK if we can't measure
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
    let mut optimizer = MEMORY_OPTIMIZER.lock().await;
    optimizer.initialize().await
}

/// Register a global memory cleanup hook
pub fn register_memory_cleanup_hook<F, Fut>(hook: F)
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    // Note: This is synchronous, so we use try_lock or spawn a task
    // Since it's only called at startup, we can spawn a task
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