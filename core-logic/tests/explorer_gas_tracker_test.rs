use core_logic::ExplorerGasTrackerPayload;
use std::time::Duration;

#[test]
fn test_payload_builder_default() {
    let payload = ExplorerGasTrackerPayload::default();
    assert_eq!(payload.url, "");
    assert_eq!(payload.row_label, "Normal");
    assert!(payload.proxies.is_empty());
    assert_eq!(payload.request_timeout, Duration::from_secs(60));
}

#[test]
fn test_payload_builder_new() {
    let payload = ExplorerGasTrackerPayload::new("https://explorer.example.com");
    assert_eq!(payload.url, "https://explorer.example.com");
    assert_eq!(payload.row_label, "Normal");
}

#[test]
fn test_payload_builder_with_row_label() {
    let payload = ExplorerGasTrackerPayload::new("https://explorer.example.com").with_row_label("Fast");
    assert_eq!(payload.row_label, "Fast");
}

#[test]
fn test_payload_builder_with_timeout() {
    let payload = ExplorerGasTrackerPayload::new("https://explorer.example.com").with_timeout(Duration::from_secs(30));
    assert_eq!(payload.request_timeout, Duration::from_secs(30));
}

#[test]
fn test_payload_builder_with_proxies() {
    let proxies = vec![
        core_logic::config::ProxyConfig {
            url: "http://proxy1:8080".to_string(),
            username: Some("u1".to_string()),
            password: Some("p1".to_string()),
        },
        core_logic::config::ProxyConfig {
            url: "http://proxy2:8080".to_string(),
            username: None,
            password: None,
        },
    ];

    let payload = ExplorerGasTrackerPayload::new("https://explorer.example.com").with_proxies(proxies.clone());

    assert_eq!(payload.proxies.len(), 2);
    assert_eq!(payload.proxies[0].url, "http://proxy1:8080");
    assert_eq!(payload.proxies[0].username.as_deref(), Some("u1"));
    assert_eq!(payload.proxies[1].url, "http://proxy2:8080");
    assert!(payload.proxies[1].username.is_none());
}

#[test]
fn test_payload_builder_chained() {
    // All builders chained together
    let payload = ExplorerGasTrackerPayload::new("https://sepolia.example.com")
        .with_row_label("Fastest")
        .with_timeout(Duration::from_secs(120))
        .with_proxies(vec![core_logic::config::ProxyConfig {
            url: "http://proxy:3128".to_string(),
            username: None,
            password: None,
        }]);

    assert_eq!(payload.url, "https://sepolia.example.com");
    assert_eq!(payload.row_label, "Fastest");
    assert_eq!(payload.request_timeout, Duration::from_secs(120));
    assert_eq!(payload.proxies.len(), 1);
}

#[test]
fn test_payload_gas_snapshot_debug() {
    let snapshot = core_logic::ExplorerGasSnapshot {
        source_url: "https://explorer.example.com".to_string(),
        row_label: "Normal".to_string(),
        normal_gwei: 10.5,
        base_gwei: Some(8.0),
        priority_gwei: Some(2.5),
    };

    let debug = format!("{:?}", snapshot);
    assert!(debug.contains("10.5"));
    assert!(debug.contains("source_url"));
    assert!(debug.contains("Normal"));
}

#[test]
fn test_payload_gas_snapshot_partial() {
    let snapshot = core_logic::ExplorerGasSnapshot {
        source_url: "https://explorer.example.com".to_string(),
        row_label: "Normal".to_string(),
        normal_gwei: 5.0,
        base_gwei: None,
        priority_gwei: None,
    };

    assert!(snapshot.base_gwei.is_none());
    assert!(snapshot.priority_gwei.is_none());
    assert_eq!(snapshot.normal_gwei, 5.0);
}

#[test]
fn test_payload_gas_snapshot_eq() {
    let a = core_logic::ExplorerGasSnapshot {
        source_url: "https://a.com".to_string(),
        row_label: "Normal".to_string(),
        normal_gwei: 10.0,
        base_gwei: None,
        priority_gwei: None,
    };
    let b = core_logic::ExplorerGasSnapshot {
        source_url: "https://a.com".to_string(),
        row_label: "Normal".to_string(),
        normal_gwei: 10.0,
        base_gwei: None,
        priority_gwei: None,
    };
    assert_eq!(a, b);
}
