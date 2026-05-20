use core_logic::metrics::MetricsCollector;
use core_logic::metrics::{PerformanceMetrics, RpcMetrics, TaskMetrics};
use std::time::Duration;

#[test]
fn test_metrics_collector_default_state() {
    let mc = MetricsCollector::default();
    let snapshot = mc.snapshot();

    assert_eq!(snapshot.tasks.total, 0);
    assert_eq!(snapshot.tasks.success, 0);
    assert_eq!(snapshot.tasks.failed, 0);
    assert_eq!(snapshot.tasks.success_rate, 0.0);
    assert_eq!(snapshot.performance.total_duration_ms, 0);
    assert_eq!(snapshot.rpc.total_calls, 0);
}

#[test]
fn test_metrics_collector_record_success() {
    let mc = MetricsCollector::default();
    mc.record_task("check", Duration::from_millis(100), true);
    let snapshot = mc.snapshot();

    assert_eq!(snapshot.tasks.total, 1);
    assert_eq!(snapshot.tasks.success, 1);
    assert_eq!(snapshot.tasks.failed, 0);
}

#[test]
fn test_metrics_collector_record_failure() {
    let mc = MetricsCollector::default();
    mc.record_task("mint", Duration::from_millis(200), false);
    let snapshot = mc.snapshot();

    assert_eq!(snapshot.tasks.total, 1);
    assert_eq!(snapshot.tasks.failed, 1);
    assert_eq!(snapshot.tasks.success, 0);
}

#[test]
fn test_metrics_collector_success_rate() {
    let mc = MetricsCollector::default();
    mc.record_task("ok", Duration::from_millis(100), true);
    mc.record_task("ok", Duration::from_millis(100), true);
    mc.record_task("fail", Duration::from_millis(200), false);
    let snapshot = mc.snapshot();

    assert_eq!(snapshot.tasks.total, 3);
    assert!((snapshot.tasks.success_rate - 66.67).abs() < 1.0);
}

#[test]
fn test_metrics_collector_rpc_latency() {
    let mc = MetricsCollector::default();
    mc.record_rpc_latency(Duration::from_millis(50));
    mc.record_rpc_latency(Duration::from_millis(150));
    let snapshot = mc.snapshot();

    assert_eq!(snapshot.rpc.total_calls, 2);
    assert!((snapshot.rpc.avg_latency_ms - 100.0).abs() < 1.0);
    assert_eq!(snapshot.rpc.min_latency_ms, 50);
    assert_eq!(snapshot.rpc.max_latency_ms, 150);
}

#[test]
fn test_metrics_collector_min_max_task_duration() {
    let mc = MetricsCollector::default();
    mc.record_task("fast", Duration::from_millis(50), true);
    mc.record_task("slow", Duration::from_millis(500), true);
    let snapshot = mc.snapshot();

    assert_eq!(snapshot.performance.min_task_duration_ms, 50);
    assert_eq!(snapshot.performance.max_task_duration_ms, 500);
}

#[test]
fn test_metrics_collector_avg_task_duration() {
    let mc = MetricsCollector::default();
    mc.record_task("a", Duration::from_millis(100), true);
    mc.record_task("b", Duration::from_millis(300), true);
    let snapshot = mc.snapshot();

    assert!((snapshot.performance.avg_task_duration_ms - 200.0).abs() < 1.0);
    assert_eq!(snapshot.performance.total_duration_ms, 400);
}

#[test]
fn test_metrics_collector_mixed_workload() {
    let mc = MetricsCollector::default();
    for _ in 0..10 {
        mc.record_task("ok", Duration::from_millis(100), true);
    }
    for _ in 0..3 {
        mc.record_task("fail", Duration::from_millis(200), false);
    }
    for _ in 0..20 {
        mc.record_rpc_latency(Duration::from_millis(25));
    }

    let snapshot = mc.snapshot();
    assert_eq!(snapshot.tasks.total, 13);
    assert_eq!(snapshot.tasks.success, 10);
    assert_eq!(snapshot.tasks.failed, 3);
    assert_eq!(snapshot.rpc.total_calls, 20);
}

#[test]
fn test_metrics_snapshot_serializable() {
    let snapshot = core_logic::metrics::MetricsSnapshot {
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        tasks: TaskMetrics {
            total: 100,
            success: 95,
            failed: 5,
            success_rate: 95.0,
        },
        performance: PerformanceMetrics {
            total_duration_ms: 50000,
            avg_task_duration_ms: 500.0,
            min_task_duration_ms: 50,
            max_task_duration_ms: 2000,
        },
        rpc: RpcMetrics {
            total_calls: 1000,
            avg_latency_ms: 150.0,
            min_latency_ms: 10,
            max_latency_ms: 3000,
        },
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    assert!(json.contains("\"success\":95"));
    assert!(json.contains("\"failed\":5"));
    assert!(json.contains("\"total_calls\":1000"));
}

#[test]
fn test_metrics_collector_global_singleton() {
    let mc1 = MetricsCollector::global();
    let mc2 = MetricsCollector::global();
    mc1.record_task("test", Duration::from_millis(100), true);

    assert_eq!(mc2.tasks_total(), mc1.tasks_total());
    assert_eq!(mc2.tasks_total(), 1);
}

#[test]
fn test_metrics_collector_to_json() {
    let mc = MetricsCollector::default();
    mc.record_task("test", Duration::from_millis(100), true);

    let json = mc.to_json();
    assert!(json.contains("tasks"));
    assert!(json.contains("success"));
    assert!(json.contains("performance"));
}

#[test]
fn test_metrics_collector_to_compact_json() {
    let mc = MetricsCollector::default();
    mc.record_task("test", Duration::from_millis(100), true);

    let json = mc.to_compact_json();
    assert!(json.contains("\"tasks\""));
    assert!(!json.contains("  \"tasks\""));
}

#[test]
fn test_metrics_collector_empty_snapshot() {
    let mc = MetricsCollector::default();
    let snapshot = mc.snapshot();
    assert_eq!(snapshot.performance.min_task_duration_ms, 0);
    assert_eq!(snapshot.performance.max_task_duration_ms, 0);
    assert_eq!(snapshot.rpc.min_latency_ms, 0);
    assert_eq!(snapshot.rpc.max_latency_ms, 0);
}

#[test]
fn test_metrics_accessors() {
    let mc = MetricsCollector::default();
    assert_eq!(mc.tasks_total(), 0);
    assert_eq!(mc.tasks_success(), 0);
    assert_eq!(mc.tasks_failed(), 0);

    mc.record_task("x", Duration::from_millis(10), true);
    assert_eq!(mc.tasks_total(), 1);
    assert_eq!(mc.tasks_success(), 1);

    mc.record_task("x", Duration::from_millis(10), false);
    assert_eq!(mc.tasks_total(), 2);
    assert_eq!(mc.tasks_failed(), 1);
}
