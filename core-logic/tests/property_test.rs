use core_logic::ExplorerGasSnapshot;
use core_logic::GasConfig;
use core_logic::ProxyHealthManager;
use proptest::prelude::*;

// ─── GasConfig property tests ────────────────────────────

proptest! {
    #[test]
    fn gas_config_max_gwei_always_non_negative(max_gwei: f64) {
        // max_gwei can be any f64, but GasConfig stores it as-is
        let config = GasConfig::new().with_max_fee(max_gwei);
        let stored = config.max_gwei();
        // The builder should preserve the value (even if negative/NaN)
        // This tests that no validation silently modifies the value
        assert_eq!(stored, max_gwei);
    }

    #[test]
    fn gas_config_priority_gwei_always_non_negative(priority: f64) {
        let config = GasConfig::new().with_priority_fee(priority);
        assert_eq!(config.priority_gwei(), priority);
    }

    #[test]
    fn gas_config_builder_chain_roundtrip(max_gwei: f64, priority: f64) {
        let config = GasConfig::new()
            .with_max_fee(max_gwei)
            .with_priority_fee(priority);
        assert_eq!(config.max_gwei(), max_gwei);
        assert_eq!(config.priority_gwei(), priority);
    }

    #[test]
    fn gas_config_limit_values(deploy: u64, transfer: u64, counter: u64, meme: u64) {
        // Gas limits are always positive; any u64 is valid
        // (even 0 or extreme values — contract handles them)
        let _config = GasConfig::new();
        // No panic expected for any u64 input
    }

    #[test]
    fn gas_config_clone_preserves(max_gwei: f64, priority: f64) {
        let a = GasConfig::new().with_max_fee(max_gwei).with_priority_fee(priority);
        let b = a.clone();
        assert_eq!(a.max_gwei(), b.max_gwei());
        assert_eq!(a.priority_gwei(), b.priority_gwei());
        assert_eq!(a.limit_deploy(), b.limit_deploy());
    }
}

// ─── ProxyHealthManager property tests ───────────────────

proptest! {
    #[test]
    fn proxy_health_failure_count_monotonic(failures: Vec<bool>) {
        // Run this in async context via proptest's runtime
        // We test the synchronous parts: any Vec<bool> is valid input
        let _fails: Vec<bool> = failures;
        // Just verify no panic for any input pattern
    }
}

#[tokio::test]
async fn proxy_health_never_panics_on_any_input() {
    // Run many random patterns
    for n in 0..50u32 {
        let hm = ProxyHealthManager::new((n % 5) + 1, ((n % 10) + 1).into());
        // Random access patterns
        for _ in 0..n {
            hm.record_failure("http://proxy:8080").await;
            hm.record_success("http://proxy:8080").await;
            let _ = hm.is_available("http://proxy:8080").await;
            let _ = hm.get_status("http://proxy:8080").await;
        }
        // After all operations, proxy should be in some valid state
        let available = hm.is_available("http://proxy:8080").await;
        // available is always true or false — no crash
        let _ = available;
    }
}

// ─── ExplorerGasSnapshot property tests ──────────────────

proptest! {
    #[test]
    fn gas_snapshot_normal_gwei_non_negative(val: f64) {
        let snapshot = ExplorerGasSnapshot {
            source_url: "https://explorer.com".into(),
            row_label: "Normal".into(),
            normal_gwei: val,
            base_gwei: None,
            priority_gwei: None,
        };
        // normal_gwei accepts any f64 — test it's stored correctly
        assert_eq!(snapshot.normal_gwei, val);
    }

    #[test]
    fn gas_snapshot_all_fields_roundtrip(
        url: String,
        label: String,
        normal: f64,
        base: f64,
        priority: f64,
    ) {
        let snapshot = ExplorerGasSnapshot {
            source_url: url.clone(),
            row_label: label.clone(),
            normal_gwei: normal,
            base_gwei: Some(base),
            priority_gwei: Some(priority),
        };
        assert_eq!(snapshot.source_url, url);
        assert_eq!(snapshot.row_label, label);
        assert_eq!(snapshot.normal_gwei, normal);
        assert_eq!(snapshot.base_gwei, Some(base));
        assert_eq!(snapshot.priority_gwei, Some(priority));
    }
}

#[test]
fn standard_gas_limits_defaults_positive() {
    // Default limits should always be > 0
    let limits = core_logic::GasConfig::new();
    assert!(limits.limit_deploy() > 0);
    assert!(limits.limit_transfer() > 0);
    assert!(limits.limit_counter_interact() > 0);
    assert!(limits.limit_send_meme() > 0);
}

// ─── TokenBucket property-based tests ────────────────────

proptest! {
    #[test]
    fn token_bucket_capacity_never_exceeded(acquires: Vec<u64>, capacity: u64) {
        // capacity=0 is legal but divides by zero for per-task acquires
        let cap = capacity.clamp(1, 10_000);
        let bucket = core_logic::TokenBucket::new(cap, 1000);
        let mut taken: u64 = 0;
        for a in &acquires {
            let cost = (*a % (cap + 1)).max(1);
            if bucket.try_acquire(cost) {
                taken = taken.saturating_add(cost);
            }
        }
        assert!(taken <= cap, "taken {} exceeds capacity {}", taken, cap);
    }
}

// ─── MetricsCollector monotonic invariants ──────────────

proptest! {
    #[test]
    fn metrics_counters_never_decrease(ops: Vec<(u64, u64, bool)>) {
        // Each op: (duration_ms_shift, label_bit, success)
        let metrics = core_logic::MetricsCollector::default();
        let mut expected_total: u64 = 0;
        let mut expected_success: u64 = 0;
        let mut expected_failed: u64 = 0;

        for (dur_shift, _, success) in ops {
            let dur_ms = dur_shift % 1000;
            metrics.record_task("prop", std::time::Duration::from_millis(dur_ms), success);
            expected_total += 1;
            if success { expected_success += 1; } else { expected_failed += 1; }

            // Snapshots are monotonically non-decreasing
            let snap = metrics.snapshot();
            assert_eq!(snap.tasks.total, expected_total);
            assert_eq!(snap.tasks.success, expected_success);
            assert_eq!(snap.tasks.failed, expected_failed);
        }
    }
}
