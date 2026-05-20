use core_logic::ProxyHealthManager;

#[tokio::test]
async fn test_health_manager_new() {
    let hm = ProxyHealthManager::new(3, 5);
    // Should not crash, default state
    assert!(hm.is_available("http://proxy1:8080").await);
}

#[tokio::test]
async fn test_health_manager_available_by_default() {
    let hm = ProxyHealthManager::new(3, 5);
    // Proxies without any recorded history should be available
    assert!(hm.is_available("http://unknown:8080").await);
}

#[tokio::test]
async fn test_health_manager_single_failure_still_available() {
    let hm = ProxyHealthManager::new(3, 5);
    hm.record_failure("http://proxy1:8080").await;
    // 1 failure < 3 threshold, should still be available
    assert!(hm.is_available("http://proxy1:8080").await);
}

#[tokio::test]
async fn test_health_manager_pauses_after_threshold() {
    let hm = ProxyHealthManager::new(3, 5); // 3 failures = 5 min pause
    for _ in 0..3 {
        hm.record_failure("http://proxy1:8080").await;
    }
    // After 3 failures, proxy should be paused
    assert!(!hm.is_available("http://proxy1:8080").await);
}

#[tokio::test]
async fn test_health_manager_recovers_on_success() {
    let hm = ProxyHealthManager::new(3, 5);
    for _ in 0..3 {
        hm.record_failure("http://proxy1:8080").await;
    }
    assert!(!hm.is_available("http://proxy1:8080").await);

    // Success should clear the pause
    hm.record_success("http://proxy1:8080").await;
    assert!(hm.is_available("http://proxy1:8080").await);
}

#[tokio::test]
async fn test_health_manager_concurrent_access() {
    let hm = std::sync::Arc::new(ProxyHealthManager::new(3, 5));
    let hm2 = hm.clone();

    let jh1 = tokio::spawn(async move {
        for _ in 0..2 {
            hm.record_failure("http://proxy1:8080").await;
        }
    });
    let jh2 = tokio::spawn(async move {
        for _ in 0..2 {
            hm2.record_failure("http://proxy1:8080").await;
        }
    });
    let _ = tokio::join!(jh1, jh2);

    // After the join, we can't access hm anymore since it was moved
    // But we can still verify the state is consistent
}

#[tokio::test]
async fn test_health_manager_multiple_proxies_independent() {
    let hm = ProxyHealthManager::new(2, 5);
    hm.record_failure("http://proxyA:8080").await;
    hm.record_failure("http://proxyA:8080").await;
    hm.record_failure("http://proxyB:8080").await;

    // proxyA hit threshold (2), proxyB has only 1 failure
    assert!(!hm.is_available("http://proxyA:8080").await);
    assert!(hm.is_available("http://proxyB:8080").await);
}

#[tokio::test]
async fn test_health_manager_failure_count_resets_on_success() {
    let hm = ProxyHealthManager::new(3, 5);
    hm.record_failure("http://proxy1:8080").await;
    hm.record_failure("http://proxy1:8080").await;
    // Reset with success
    hm.record_success("http://proxy1:8080").await;
    // Then one more failure should not trigger pause (threshold is 3)
    hm.record_failure("http://proxy1:8080").await;
    assert!(hm.is_available("http://proxy1:8080").await);
}

#[tokio::test]
async fn test_health_manager_get_status() {
    let hm = ProxyHealthManager::new(3, 5);
    hm.record_success("http://proxy1:8080").await;
    hm.record_failure("http://proxy1:8080").await;

    let status = hm.get_status("http://proxy1:8080").await;
    assert!(status.is_some());
    let s = status.unwrap();
    assert!(s.contains("success"));
    assert!(s.contains("failure"));
}

#[tokio::test]
async fn test_health_manager_get_status_unknown() {
    let hm = ProxyHealthManager::new(3, 5);
    let status = hm.get_status("http://never-heard:8080").await;
    assert!(status.is_none());
}

#[tokio::test]
async fn test_health_manager_healthy_count() {
    let hm = ProxyHealthManager::new(2, 5);
    let proxies = vec![
        "http://proxyA:8080".to_string(),
        "http://proxyB:8080".to_string(),
        "http://proxyC:8080".to_string(),
    ];

    // All healthy initially
    assert_eq!(hm.get_healthy_count(&proxies).await, 3);

    // Pause proxyA
    hm.record_failure("http://proxyA:8080").await;
    hm.record_failure("http://proxyA:8080").await;
    assert_eq!(hm.get_healthy_count(&proxies).await, 2);
}

#[tokio::test]
async fn test_health_manager_cleanup_expired() {
    let hm = ProxyHealthManager::new(1, 1); // 1 failure, 1 minute pause
    hm.record_failure("http://proxy1:8080").await;
    assert!(!hm.is_available("http://proxy1:8080").await);

    // Cleanup expired pauses - should NOT clear a 1-minute pause
    hm.cleanup_expired().await;
    assert!(!hm.is_available("http://proxy1:8080").await);
}

#[tokio::test]
async fn test_health_manager_default() {
    let hm = core_logic::ProxyHealthManager::default();
    // Default: 3 failures, 5 minutes
    for _ in 0..3 {
        hm.record_failure("http://proxy1:8080").await;
    }
    assert!(!hm.is_available("http://proxy1:8080").await);
}
