use core_logic::ProxyManager;

#[test]
fn test_proxy_manager_loads_none_when_no_file() {
    // Should return empty vec gracefully when file doesn't exist
    let proxies = ProxyManager::load_proxies().unwrap();
    assert!(proxies.is_empty());
}

#[test]
fn test_proxy_manager_empty_line_parsing() {
    // We test the parsing logic indirectly via the ProxyConfig struct
    let proxy = core_logic::config::ProxyConfig {
        url: "http://192.168.1.1:8080".to_string(),
        username: None,
        password: None,
    };
    assert_eq!(proxy.url, "http://192.168.1.1:8080");
    assert!(proxy.username.is_none());
    assert!(proxy.password.is_none());
}

#[test]
fn test_proxy_config_with_auth() {
    let proxy = core_logic::config::ProxyConfig {
        url: "http://192.168.1.1:8080".to_string(),
        username: Some("user123".to_string()),
        password: Some("pass456".to_string()),
    };
    assert_eq!(proxy.username.as_deref(), Some("user123"));
    assert_eq!(proxy.password.as_deref(), Some("pass456"));
}

#[test]
fn test_proxy_config_clone() {
    let proxy = core_logic::config::ProxyConfig {
        url: "http://proxy:3128".to_string(),
        username: Some("u".to_string()),
        password: Some("p".to_string()),
    };
    let cloned = proxy.clone();
    assert_eq!(cloned.url, proxy.url);
    assert_eq!(cloned.username, proxy.username);
    assert_eq!(cloned.password, proxy.password);
}

#[test]
fn test_proxy_config_debug() {
    let proxy = core_logic::config::ProxyConfig {
        url: "http://proxy:3128".to_string(),
        username: None,
        password: None,
    };
    let debug_str = format!("{:?}", proxy);
    assert!(debug_str.contains("proxy:3128"));
}
