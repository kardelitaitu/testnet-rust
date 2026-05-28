//! Memory monitoring and profiling utilities

use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};
use tracing::{info, warn};

/// Memory usage statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub timestamp: Instant,
    pub resident_set_size: u64,
    pub virtual_memory_size: u64,
    pub process_id: u32,
    pub cpu_usage: f32,
}

/// Memory monitoring configuration
#[derive(Debug, Clone)]
pub struct MemoryMonitorConfig {
    pub sampling_interval_ms: u64,
    pub history_size: usize,
    pub memory_threshold_mb: u64,
    pub cpu_threshold_percent: f32,
}

impl Default for MemoryMonitorConfig {
    fn default() -> Self {
        Self {
            sampling_interval_ms: 5000,  // 5 seconds
            history_size: 100,           // Keep last 100 samples
            memory_threshold_mb: 1024,   // 1GB threshold
            cpu_threshold_percent: 80.0, // 80% CPU threshold
        }
    }
}

/// Memory monitor that tracks process memory usage
pub struct MemoryMonitor {
    system: System,
    config: MemoryMonitorConfig,
    history: VecDeque<MemoryStats>,
    process_id: u32,
    last_alert: Option<Instant>,
}

impl MemoryMonitor {
    pub fn new(config: MemoryMonitorConfig) -> Result<Self> {
        let mut system = System::new_all();
        system.refresh_all();

        let process_id = std::process::id();

        Ok(Self {
            system,
            config: config.clone(),
            history: VecDeque::with_capacity(config.history_size),
            process_id,
            last_alert: None,
        })
    }

    pub fn sample(&mut self) -> Result<MemoryStats> {
        self.system.refresh_processes();

        let process = self
            .system
            .process(Pid::from(self.process_id as usize))
            .context("Failed to find current process")?;

        let stats = MemoryStats {
            timestamp: Instant::now(),
            resident_set_size: process.memory(),
            virtual_memory_size: process.virtual_memory(),
            process_id: self.process_id,
            cpu_usage: process.cpu_usage(),
        };

        // Add to history, maintaining size limit
        if self.history.len() >= self.config.history_size {
            self.history.pop_front();
        }
        self.history.push_back(stats.clone());

        // Check for memory leaks or high usage
        self.check_thresholds(&stats);

        Ok(stats)
    }

    fn check_thresholds(&mut self, stats: &MemoryStats) {
        let memory_mb = stats.resident_set_size / 1024 / 1024;

        if memory_mb > self.config.memory_threshold_mb {
            // Rate limit alerts to avoid spam
            if self
                .last_alert
                .is_none_or(|last| last.elapsed() > Duration::from_secs(60))
            {
                warn!(
                    "High memory usage detected: {}MB (threshold: {}MB)",
                    memory_mb, self.config.memory_threshold_mb
                );
                self.last_alert = Some(Instant::now());
            }
        }

        if stats.cpu_usage > self.config.cpu_threshold_percent {
            warn!(
                "High CPU usage detected: {:.1}% (threshold: {:.1}%)",
                stats.cpu_usage, self.config.cpu_threshold_percent
            );
        }
    }

    pub fn get_history(&self) -> &VecDeque<MemoryStats> {
        &self.history
    }

    pub fn get_trend(&self) -> MemoryTrend {
        if self.history.len() < 2 {
            return MemoryTrend::Stable;
        }

        let first = self.history.front().unwrap();
        let last = self.history.back().unwrap();
        let duration = last.timestamp.duration_since(first.timestamp).as_secs_f64();

        if duration <= 0.0 {
            return MemoryTrend::Stable;
        }

        let memory_growth = (last.resident_set_size as f64 - first.resident_set_size as f64) / duration;

        if memory_growth > 1024.0 * 1024.0 {
            // Growing > 1MB/s
            MemoryTrend::RapidGrowth
        } else if memory_growth > 1024.0 {
            // Growing > 1KB/s
            MemoryTrend::SlowGrowth
        } else if memory_growth < -1024.0 {
            // Shrinking
            MemoryTrend::Decreasing
        } else {
            MemoryTrend::Stable
        }
    }

    #[cfg(test)]
    pub fn test_add_stats(&mut self, stats: MemoryStats) {
        if self.history.len() >= self.config.history_size {
            self.history.pop_front();
        }
        self.history.push_back(stats);
    }

    pub fn generate_report(&self) -> String {
        if self.history.is_empty() {
            return "No memory data available".to_string();
        }

        let first = self.history.front().unwrap();
        let last = self.history.back().unwrap();
        let duration = last.timestamp.duration_since(first.timestamp);

        let min_memory = self.history.iter().map(|s| s.resident_set_size).min().unwrap();
        let max_memory = self.history.iter().map(|s| s.resident_set_size).max().unwrap();
        let avg_memory = self.history.iter().map(|s| s.resident_set_size).sum::<u64>() / self.history.len() as u64;

        format!(
            "Memory Usage Report:\n\
             Duration: {:.1}s\n\
             Current: {:.1}MB\n\
             Min: {:.1}MB\n\
             Max: {:.1}MB\n\
             Avg: {:.1}MB\n\
             Trend: {:?}\n\
             Samples: {}",
            duration.as_secs_f32(),
            last.resident_set_size as f64 / 1024.0 / 1024.0,
            min_memory as f64 / 1024.0 / 1024.0,
            max_memory as f64 / 1024.0 / 1024.0,
            avg_memory as f64 / 1024.0 / 1024.0,
            self.get_trend(),
            self.history.len()
        )
    }
}

/// Memory trend analysis
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemoryTrend {
    RapidGrowth,
    SlowGrowth,
    Stable,
    Decreasing,
}

/// Global memory monitor instance
pub static MEMORY_MONITOR: once_cell::sync::Lazy<Arc<Mutex<MemoryMonitor>>> = once_cell::sync::Lazy::new(|| {
    let config = MemoryMonitorConfig::default();
    Arc::new(Mutex::new(MemoryMonitor::new(config).unwrap()))
});

/// Initialize memory monitoring
pub fn init_memory_monitoring() -> Result<()> {
    let mut monitor = MEMORY_MONITOR.lock().unwrap();

    // Take initial sample
    let stats = monitor.sample()?;

    info!(
        "Memory monitoring initialized. Initial memory usage: {:.1}MB",
        stats.resident_set_size as f64 / 1024.0 / 1024.0
    );

    Ok(())
}

/// Sample current memory usage
pub fn sample_memory_usage() -> Result<MemoryStats> {
    let mut monitor = MEMORY_MONITOR.lock().unwrap();
    monitor.sample()
}

/// Get memory usage report
pub fn get_memory_report() -> String {
    let monitor = MEMORY_MONITOR.lock().unwrap();
    monitor.generate_report()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_values() {
        let cfg = MemoryMonitorConfig::default();
        assert_eq!(cfg.sampling_interval_ms, 5000);
        assert_eq!(cfg.history_size, 100);
        assert_eq!(cfg.memory_threshold_mb, 1024);
        assert!((cfg.cpu_threshold_percent - 80.0f32).abs() < f32::EPSILON);
    }

    #[test]
    fn test_memory_stats_clone() {
        let s = MemoryStats {
            timestamp: Instant::now(),
            resident_set_size: 1_000_000,
            virtual_memory_size: 2_000_000,
            process_id: 12345,
            cpu_usage: 45.5,
        };
        let c = s.clone();
        assert_eq!(c.resident_set_size, 1_000_000);
        assert_eq!(c.process_id, 12345);
    }

    #[test]
    fn test_memory_monitor_config_custom() {
        let cfg = MemoryMonitorConfig {
            sampling_interval_ms: 10000,
            history_size: 200,
            memory_threshold_mb: 2048,
            cpu_threshold_percent: 90.0,
        };
        assert_eq!(cfg.sampling_interval_ms, 10000);
        assert_eq!(cfg.history_size, 200);
        assert_eq!(cfg.memory_threshold_mb, 2048);
        assert!((cfg.cpu_threshold_percent - 90.0f32).abs() < f32::EPSILON);
    }

    #[test]
    fn test_memory_trend_variants() {
        assert_eq!(MemoryTrend::Stable, MemoryTrend::Stable);
    }

    #[test]
    fn test_memory_trend_partial_eq() {
        assert_eq!(MemoryTrend::Stable, MemoryTrend::Stable);
        assert_ne!(MemoryTrend::RapidGrowth, MemoryTrend::Decreasing);
    }

    #[test]
    fn test_memory_trend_clone() {
        let a = MemoryTrend::RapidGrowth;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn test_memory_monitor_trend_detection() {
        let mut monitor = MemoryMonitor::new(MemoryMonitorConfig::default()).unwrap();
        let now = Instant::now();

        // No stats -> Stable
        assert_eq!(monitor.get_trend(), MemoryTrend::Stable);

        // Rapid growth: +10MB over 1s
        monitor.test_add_stats(MemoryStats {
            timestamp: now,
            resident_set_size: 100 * 1024 * 1024,
            virtual_memory_size: 0,
            process_id: 0,
            cpu_usage: 0.0,
        });
        monitor.test_add_stats(MemoryStats {
            timestamp: now + Duration::from_secs(1),
            resident_set_size: 110 * 1024 * 1024,
            virtual_memory_size: 0,
            process_id: 0,
            cpu_usage: 0.0,
        });
        assert_eq!(monitor.get_trend(), MemoryTrend::RapidGrowth);

        // Reset and test Decreasing
        let mut monitor2 = MemoryMonitor::new(MemoryMonitorConfig::default()).unwrap();
        monitor2.test_add_stats(MemoryStats {
            timestamp: now,
            resident_set_size: 100 * 1024 * 1024,
            virtual_memory_size: 0,
            process_id: 0,
            cpu_usage: 0.0,
        });
        monitor2.test_add_stats(MemoryStats {
            timestamp: now + Duration::from_secs(1),
            resident_set_size: 90 * 1024 * 1024,
            virtual_memory_size: 0,
            process_id: 0,
            cpu_usage: 0.0,
        });
        assert_eq!(monitor2.get_trend(), MemoryTrend::Decreasing);
    }

    #[test]
    fn test_memory_monitor_generate_report() {
        let mut monitor = MemoryMonitor::new(MemoryMonitorConfig::default()).unwrap();
        let report = monitor.generate_report();
        assert_eq!(report, "No memory data available");

        monitor.test_add_stats(MemoryStats {
            timestamp: Instant::now(),
            resident_set_size: 1024 * 1024,
            virtual_memory_size: 0,
            process_id: 0,
            cpu_usage: 0.0,
        });
        let report2 = monitor.generate_report();
        assert!(report2.contains("Current: 1.0MB"));
    }

    #[test]
    fn test_init_memory_monitoring_no_panic() {
        let result = init_memory_monitoring();
        assert!(result.is_ok());
    }

    #[test]
    fn test_sample_memory_usage_no_panic() {
        let result = sample_memory_usage();
        assert!(result.is_ok());
    }
}
