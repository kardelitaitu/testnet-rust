use chrono::Utc;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub timestamp: String,
    pub tasks: TaskMetrics,
    pub performance: PerformanceMetrics,
    pub rpc: RpcMetrics,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskMetrics {
    pub total: u64,
    pub success: u64,
    pub failed: u64,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerformanceMetrics {
    pub total_duration_ms: u64,
    pub avg_task_duration_ms: f64,
    pub min_task_duration_ms: u64,
    pub max_task_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcMetrics {
    pub total_calls: u64,
    pub avg_latency_ms: f64,
    pub min_latency_ms: u64,
    pub max_latency_ms: u64,
}

#[derive(Debug)]
pub struct MetricsCollector {
    tasks_total: AtomicU64,
    tasks_success: AtomicU64,
    tasks_failed: AtomicU64,
    task_duration_sum_ms: AtomicU64,
    task_min_duration_ms: AtomicU64,
    task_max_duration_ms: AtomicU64,
    rpc_calls: AtomicU64,
    rpc_latency_sum_ms: AtomicU64,
    rpc_min_latency_ms: AtomicU64,
    rpc_max_latency_ms: AtomicU64,
    start_time: Instant,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self {
            tasks_total: AtomicU64::new(0),
            tasks_success: AtomicU64::new(0),
            tasks_failed: AtomicU64::new(0),
            task_duration_sum_ms: AtomicU64::new(0),
            task_min_duration_ms: AtomicU64::new(u64::MAX),
            task_max_duration_ms: AtomicU64::new(0),
            rpc_calls: AtomicU64::new(0),
            rpc_latency_sum_ms: AtomicU64::new(0),
            rpc_min_latency_ms: AtomicU64::new(u64::MAX),
            rpc_max_latency_ms: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }
}

impl MetricsCollector {
    pub fn global() -> &'static Self {
        static INSTANCE: std::sync::OnceLock<MetricsCollector> = std::sync::OnceLock::new();
        INSTANCE.get_or_init(|| MetricsCollector::default())
    }

    pub fn record_task(&self, _name: &str, duration: Duration, success: bool) {
        self.tasks_total.fetch_add(1, Ordering::SeqCst);
        self.task_duration_sum_ms
            .fetch_add(duration.as_millis() as u64, Ordering::SeqCst);

        let duration_ms = duration.as_millis() as u64;

        self.task_min_duration_ms
            .fetch_min(duration_ms, Ordering::SeqCst);
        self.task_max_duration_ms
            .fetch_max(duration_ms, Ordering::SeqCst);

        if success {
            self.tasks_success.fetch_add(1, Ordering::SeqCst);
        } else {
            self.tasks_failed.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub fn record_rpc_latency(&self, latency: Duration) {
        self.rpc_calls.fetch_add(1, Ordering::SeqCst);
        self.rpc_latency_sum_ms
            .fetch_add(latency.as_millis() as u64, Ordering::SeqCst);

        let latency_ms = latency.as_millis() as u64;
        self.rpc_min_latency_ms
            .fetch_min(latency_ms, Ordering::SeqCst);
        self.rpc_max_latency_ms
            .fetch_max(latency_ms, Ordering::SeqCst);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let total_tasks = self.tasks_total.load(Ordering::SeqCst);
        let total_duration = self.task_duration_sum_ms.load(Ordering::SeqCst);
        let min_duration = self.task_min_duration_ms.load(Ordering::SeqCst);
        let max_duration = self.task_max_duration_ms.load(Ordering::SeqCst);

        let rpc_calls = self.rpc_calls.load(Ordering::SeqCst);
        let rpc_latency = self.rpc_latency_sum_ms.load(Ordering::SeqCst);
        let min_rpc = self.rpc_min_latency_ms.load(Ordering::SeqCst);
        let max_rpc = self.rpc_max_latency_ms.load(Ordering::SeqCst);

        let total_success = self.tasks_success.load(Ordering::SeqCst);

        MetricsSnapshot {
            timestamp: Utc::now().to_rfc3339(),
            tasks: TaskMetrics {
                total: total_tasks,
                success: total_success,
                failed: self.tasks_failed.load(Ordering::SeqCst),
                success_rate: if total_tasks > 0 {
                    total_success as f64 / total_tasks as f64 * 100.0
                } else {
                    0.0
                },
            },
            performance: PerformanceMetrics {
                total_duration_ms: total_duration,
                avg_task_duration_ms: if total_tasks > 0 {
                    total_duration as f64 / total_tasks as f64
                } else {
                    0.0
                },
                min_task_duration_ms: if min_duration == u64::MAX {
                    0
                } else {
                    min_duration
                },
                max_task_duration_ms: max_duration,
            },
            rpc: RpcMetrics {
                total_calls: rpc_calls,
                avg_latency_ms: if rpc_calls > 0 {
                    rpc_latency as f64 / rpc_calls as f64
                } else {
                    0.0
                },
                min_latency_ms: if min_rpc == u64::MAX { 0 } else { min_rpc },
                max_latency_ms: max_rpc,
            },
        }
    }

    pub fn to_json(&self) -> String {
        let snapshot = self.snapshot();
        serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn to_compact_json(&self) -> String {
        let snapshot = self.snapshot();
        serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".to_string())
    }

    pub async fn export_to_file(&self, path: &str) -> std::io::Result<()> {
        let json = self.to_json();
        tokio::fs::write(path, json).await
    }

    pub fn tasks_total(&self) -> u64 {
        self.tasks_total.load(Ordering::SeqCst)
    }

    pub fn tasks_success(&self) -> u64 {
        self.tasks_success.load(Ordering::SeqCst)
    }

    pub fn tasks_failed(&self) -> u64 {
        self.tasks_failed.load(Ordering::SeqCst)
    }

    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector() {
        let metrics = MetricsCollector::default();

        metrics.record_task("test_task", Duration::from_millis(100), true);
        metrics.record_task("test_task", Duration::from_millis(200), true);
        metrics.record_task("test_task", Duration::from_millis(150), false);

        assert_eq!(metrics.tasks_total(), 3);
        assert_eq!(metrics.tasks_success(), 2);
        assert_eq!(metrics.tasks_failed(), 1);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.tasks.total, 3);
        assert!((snapshot.tasks.success_rate - 66.67).abs() < 0.1);
    }

    #[tokio::test]
    async fn test_json_export() {
        let metrics = MetricsCollector::default();
        metrics.record_task("test", Duration::from_millis(100), true);

        let json = metrics.to_json();
        assert!(json.contains("tasks"));
        assert!(json.contains("performance"));
    }
}
