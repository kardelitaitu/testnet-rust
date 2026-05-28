use core_logic::config::{ChainConfig, ProxyConfig, SpamConfig, WalletSource};

const SPAM_CONFIG_TOML: &str = r#"
rpc_url = "https://sepolia.example.com"
chain_id = 11155111
target_tps = 10
wallet_source = { File = { path = "wallet-json", encrypted = true } }
"#;

const SPAM_CONFIG_ENV_TOML: &str = r#"
rpc_url = "https://rpc.example.com"
chain_id = 1
target_tps = 50
duration_seconds = 3600
wallet_source = { Env = { key = "PRIVATE_KEY" } }
"#;

#[test]
fn test_spam_config_deserialize_file_wallet() {
    let config: SpamConfig = toml::from_str(SPAM_CONFIG_TOML).unwrap();
    assert_eq!(config.rpc_url, "https://sepolia.example.com");
    assert_eq!(config.chain_id, 11155111);
    assert_eq!(config.target_tps, 10);
    assert!(config.duration_seconds.is_none());
    match &config.wallet_source {
        WalletSource::File { path, encrypted } => {
            assert_eq!(path, "wallet-json");
            assert!(*encrypted);
        },
        _ => panic!("Expected File wallet source"),
    }
}

#[test]
fn test_spam_config_deserialize_env_wallet() {
    let config: SpamConfig = toml::from_str(SPAM_CONFIG_ENV_TOML).unwrap();
    assert_eq!(config.rpc_url, "https://rpc.example.com");
    assert_eq!(config.chain_id, 1);
    assert_eq!(config.target_tps, 50);
    assert_eq!(config.duration_seconds, Some(3600));
    match &config.wallet_source {
        WalletSource::Env { key } => {
            assert_eq!(key, "PRIVATE_KEY");
        },
        _ => panic!("Expected Env wallet source"),
    }
}

#[test]
fn test_spam_config_roundtrip() {
    let config = SpamConfig {
        rpc_url: "https://eth.da".into(),
        chain_id: 21894,
        target_tps: 100,
        duration_seconds: None,
        wallet_source: WalletSource::File {
            path: "wallets.json".into(),
            encrypted: true,
        },
    };
    let toml_str = toml::to_string(&config).unwrap();
    let deserialized: SpamConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(deserialized.rpc_url, config.rpc_url);
    assert_eq!(deserialized.chain_id, config.chain_id);
    assert_eq!(deserialized.target_tps, config.target_tps);
}

#[test]
fn test_proxy_config_deserialize() {
    let toml_str = r#"
url = "http://proxy:8080"
username = "user123"
password = "pass456"
"#;
    let config: ProxyConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.url, "http://proxy:8080");
    assert_eq!(config.username.as_deref(), Some("user123"));
    assert_eq!(config.password.as_deref(), Some("pass456"));
}

#[test]
fn test_proxy_config_deserialize_no_auth() {
    let toml_str = r#"
url = "http://proxy:3128"
"#;
    let config: ProxyConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.url, "http://proxy:3128");
    assert!(config.username.is_none());
    assert!(config.password.is_none());
}

#[test]
fn test_proxy_config_roundtrip() {
    let config = ProxyConfig {
        url: "http://1.2.3.4:8080".into(),
        username: Some("u".into()),
        password: Some("p".into()),
    };
    let toml_str = toml::to_string(&config).unwrap();
    let deserialized: ProxyConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(deserialized.url, config.url);
    assert_eq!(deserialized.username, config.username);
    assert_eq!(deserialized.password, config.password);
}

#[test]
fn test_chain_config_deserialize() {
    let toml_str = r#"
name = "Sepolia Testnet"
rpc_endpoint = "https://sepolia.example.com"
chain_id = 11155111
"#;
    let config: ChainConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.name, "Sepolia Testnet");
    assert_eq!(config.rpc_endpoint, "https://sepolia.example.com");
    assert_eq!(config.chain_id, 11155111);
}

#[test]
fn test_chain_config_roundtrip() {
    let config = ChainConfig {
        name: "DA Chain".into(),
        rpc_endpoint: "https://da.chain".into(),
        chain_id: 21894,
    };
    let toml_str = toml::to_string(&config).unwrap();
    let deserialized: ChainConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(deserialized.name, config.name);
    assert_eq!(deserialized.chain_id, config.chain_id);
}

#[test]
fn test_wallet_source_file_serialize() {
    let source = WalletSource::File {
        path: "keys.json".into(),
        encrypted: false,
    };
    let toml_str = toml::to_string(&source).unwrap();
    // Should serialize in a way that can be deserialized back
    let deserialized: WalletSource = toml::from_str(&toml_str).unwrap();
    match deserialized {
        WalletSource::File { path, encrypted } => {
            assert_eq!(path, "keys.json");
            assert!(!encrypted);
        },
        _ => panic!("Expected File"),
    }
}

#[test]
fn test_wallet_source_env_serialize() {
    let source = WalletSource::Env { key: "MY_KEY".into() };
    let toml_str = toml::to_string(&source).unwrap();
    let deserialized: WalletSource = toml::from_str(&toml_str).unwrap();
    match deserialized {
        WalletSource::Env { key } => {
            assert_eq!(key, "MY_KEY");
        },
        _ => panic!("Expected Env"),
    }
}

#[test]
fn test_chain_config_missing_fields() {
    // Missing rpc_endpoint should fail
    let toml_str = r#"name = "Test"
chain_id = 1"#;
    let result: Result<ChainConfig, _> = toml::from_str(toml_str);
    assert!(result.is_err());
}

#[test]
fn test_spam_config_missing_wallet_source() {
    // Missing wallet_source should fail
    let toml_str = r#"rpc_url = "https://rpc.com"
chain_id = 1
target_tps = 10"#;
    let result: Result<SpamConfig, _> = toml::from_str(toml_str);
    assert!(result.is_err());
}
