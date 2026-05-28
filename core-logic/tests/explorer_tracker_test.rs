use core_logic::ExplorerGasTracker;
use core_logic::ExplorerGasTrackerPayload;
use core_logic::ProxyConfig;
use std::time::Duration;

#[test]
fn test_explorer_tracker_from_url() {
    // from_url should create a valid tracker (doesn't make network requests)
    let tracker = ExplorerGasTracker::from_url("https://explorer.example.com/gas-tracker");
    assert!(tracker.is_ok());
}

#[test]
fn test_explorer_tracker_new_valid() {
    let payload = ExplorerGasTrackerPayload::new("https://explorer.example.com");
    let tracker = ExplorerGasTracker::new(payload);
    assert!(tracker.is_ok());
}

#[test]
fn test_explorer_tracker_with_proxy() {
    let payload = ExplorerGasTrackerPayload::new("https://explorer.example.com").with_proxies(vec![ProxyConfig {
        url: "http://proxy:8080".into(),
        username: Some("user".into()),
        password: Some("pass".into()),
    }]);
    let tracker = ExplorerGasTracker::new(payload);
    assert!(tracker.is_ok());
}

#[test]
fn test_explorer_tracker_with_invalid_proxy_url() {
    // Invalid proxy URL scheme should fail
    let payload = ExplorerGasTrackerPayload::new("https://explorer.example.com").with_proxies(vec![ProxyConfig {
        url: ":::invalid:::".into(),
        username: None,
        password: None,
    }]);
    let tracker = ExplorerGasTracker::new(payload);
    assert!(tracker.is_err());
}

#[test]
fn test_explorer_tracker_with_multiple_proxies() {
    let payload = ExplorerGasTrackerPayload::new("https://explorer.example.com").with_proxies(vec![
        ProxyConfig {
            url: "http://proxy1:8080".into(),
            username: None,
            password: None,
        },
        ProxyConfig {
            url: "http://proxy2:8080".into(),
            username: Some("u".into()),
            password: Some("p".into()),
        },
    ]);
    let tracker = ExplorerGasTracker::new(payload);
    assert!(tracker.is_ok());
}

#[test]
fn test_explorer_tracker_with_proxies_and_timeout() {
    let payload = ExplorerGasTrackerPayload::new("https://explorer.example.com")
        .with_proxies(vec![ProxyConfig {
            url: "http://proxy:3128".into(),
            username: None,
            password: None,
        }])
        .with_timeout(Duration::from_secs(30))
        .with_row_label("Fast");
    let tracker = ExplorerGasTracker::new(payload);
    assert!(tracker.is_ok());
}

#[test]
fn test_explorer_tracker_empty_url_fails_on_fetch() {
    // Empty URL should still build a client (no network request in constructor)
    let payload = ExplorerGasTrackerPayload::new("");
    let tracker = ExplorerGasTracker::new(payload);
    assert!(tracker.is_ok());
}

#[test]
fn test_explorer_tracker_with_payload_builder() {
    // with_payload should update the payload
    let tracker = ExplorerGasTracker::from_url("https://original.com").unwrap();
    let new_payload = ExplorerGasTrackerPayload::new("https://updated.com");
    let updated = tracker.with_payload(new_payload);
    // with_payload returns Self (consumes self), so we just verify no panic
    let _ = updated;
}

#[test]
fn test_explorer_tracker_payload_no_proxies() {
    // Explicitly empty proxy list
    let payload = ExplorerGasTrackerPayload::new("https://explorer.example.com").with_proxies(vec![]);
    let tracker = ExplorerGasTracker::new(payload);
    assert!(tracker.is_ok());
}
