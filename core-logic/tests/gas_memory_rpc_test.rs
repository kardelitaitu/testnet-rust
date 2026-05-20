use core_logic::GasConfig;
use core_logic::MemoryMonitor;
use core_logic::MemoryMonitorConfig;
use core_logic::MemoryOptimizer;
use core_logic::MemoryOptimizerConfig;

// ─── GasConfig ────────────────────────────────────────────

#[test]
fn test_gas_config_defaults() {
    let config = GasConfig::default();
    assert_eq!(config.max_gwei(), 2.5);
    assert_eq!(config.priority_gwei(), 1.5);
}

#[test]
fn test_gas_config_new() {
    let config = GasConfig::new();
    assert_eq!(config.max_gwei(), 2.5);
    assert_eq!(config.priority_gwei(), 1.5);
}

#[test]
fn test_gas_config_with_builder() {
    let config = GasConfig::new().with_max_fee(50.0).with_priority_fee(2.0);
    assert_eq!(config.max_gwei(), 50.0);
    assert_eq!(config.priority_gwei(), 2.0);
}

#[test]
fn test_gas_config_limit_accessors() {
    let config = GasConfig::default();
    assert_eq!(config.limit_deploy(), 1_200_000);
    assert_eq!(config.limit_transfer(), 21_000);
    assert_eq!(config.limit_counter_interact(), 50_000);
    assert_eq!(config.limit_send_meme(), 100_000);
}

#[test]
fn test_gas_config_clone() {
    let a = GasConfig::new().with_max_fee(99.9);
    let b = a.clone();
    assert_eq!(b.max_gwei(), 99.9);
}

// ─── MemoryMonitorConfig ──────────────────────────────────

#[test]
fn test_memory_monitor_config_defaults() {
    let config = MemoryMonitorConfig::default();
    assert_eq!(config.sampling_interval_ms, 5000);
    assert_eq!(config.history_size, 100);
    assert_eq!(config.memory_threshold_mb, 1024);
    assert_eq!(config.cpu_threshold_percent, 80.0);
}

#[test]
fn test_memory_monitor_new() {
    let config = MemoryMonitorConfig::default();
    let monitor = MemoryMonitor::new(config);
    assert!(monitor.is_ok());
    let mut monitor = monitor.unwrap();

    // Initially no history
    assert_eq!(monitor.get_history().len(), 0);

    // sample() should succeed on this system
    let sample = monitor.sample();
    assert!(sample.is_ok());
    let stats = sample.unwrap();
    assert!(stats.process_id > 0);
    assert_eq!(monitor.get_history().len(), 1);

    // Generate report
    let report = monitor.generate_report();
    assert!(!report.is_empty());
}

#[test]
fn test_memory_monitor_history_capped() {
    let config = MemoryMonitorConfig {
        sampling_interval_ms: 0,
        history_size: 3,
        memory_threshold_mb: 99999,
        cpu_threshold_percent: 100.0,
    };
    let mut monitor = MemoryMonitor::new(config).unwrap();

    for _ in 0..5 {
        let _ = monitor.sample();
    }
    assert_eq!(monitor.get_history().len(), 3);
}

#[test]
fn test_memory_monitor_get_trend() {
    let config = MemoryMonitorConfig {
        sampling_interval_ms: 0,
        history_size: 10,
        memory_threshold_mb: 99999,
        cpu_threshold_percent: 100.0,
    };
    let mut monitor = MemoryMonitor::new(config).unwrap();
    let trend = monitor.get_trend();
    // Should return some valid trend variant
    let _display = format!("{:?}", trend);
}

#[test]
fn test_memory_monitor_sample_usage_free_function() {
    // The free function sample_memory_usage() should work on any system
    let result = core_logic::sample_memory_usage();
    assert!(result.is_ok());
    let stats = result.unwrap();
    assert!(stats.process_id > 0);
}

#[test]
fn test_memory_monitor_get_report_free_function() {
    let report = core_logic::get_memory_report();
    assert!(!report.is_empty());
}

// ─── MemoryOptimizer ─────────────────────────────────────

#[test]
fn test_memory_optimizer_config_defaults() {
    let config = MemoryOptimizerConfig::default();
    assert!(config.enable_memory_monitoring);
    assert!(config.enable_log_optimization);
    assert!(config.enable_gc_tuning);
    assert_eq!(config.memory_cleanup_interval_ms, 30000);
    assert_eq!(config.max_memory_usage_mb, 300);
}

#[test]
fn test_memory_optimizer_new() {
    let optimizer = MemoryOptimizer::new(MemoryOptimizerConfig::default());
    let report = optimizer.get_status_report();
    assert!(!report.is_empty());
}

#[test]
fn test_memory_optimizer_status_report() {
    let optimizer = MemoryOptimizer::new(MemoryOptimizerConfig::default());
    let report = optimizer.get_status_report();
    assert!(!report.is_empty());
    assert!(report.contains("memory") || report.contains("Memory") || report.contains("300"));
}

#[test]
fn test_memory_optimizer_config_custom() {
    let config = MemoryOptimizerConfig {
        enable_memory_monitoring: false,
        enable_log_optimization: false,
        enable_gc_tuning: false,
        memory_cleanup_interval_ms: 5000,
        max_memory_usage_mb: 512,
    };
    let optimizer = MemoryOptimizer::new(config);
    let report = optimizer.get_status_report();
    assert!(!report.is_empty());
    assert!(report.contains("512") || report.contains("cleanup") || report.contains("5000"));
}
