use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpamConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    pub target_tps: u32,
    pub duration_seconds: Option<u64>,
    pub wallet_source: WalletSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalletSource {
    File { path: String, encrypted: bool },
    Env { key: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub name: String,
    pub rpc_endpoint: String,
    pub chain_id: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spam_config_fields() {
        let cfg = SpamConfig {
            rpc_url: "https://rpc.example.com".into(),
            chain_id: 1,
            target_tps: 10,
            duration_seconds: Some(3600),
            wallet_source: WalletSource::File {
                path: "wallets.json".into(),
                encrypted: true,
            },
        };
        assert_eq!(cfg.rpc_url, "https://rpc.example.com");
        assert_eq!(cfg.chain_id, 1);
        assert_eq!(cfg.target_tps, 10);
        assert_eq!(cfg.duration_seconds, Some(3600));
    }

    #[test]
    fn test_spam_config_no_duration() {
        let cfg = SpamConfig {
            rpc_url: "https://rpc.test.com".into(),
            chain_id: 137,
            target_tps: 5,
            duration_seconds: None,
            wallet_source: WalletSource::Env {
                key: "PRIVATE_KEY".into(),
            },
        };
        assert!(cfg.duration_seconds.is_none());
    }

    #[test]
    fn test_wallet_source_file_variant() {
        let src = WalletSource::File {
            path: "/tmp/keys.json".into(),
            encrypted: false,
        };
        match src {
            WalletSource::File { path, encrypted } => {
                assert_eq!(path, "/tmp/keys.json");
                assert!(!encrypted);
            },
            _ => panic!("Expected File variant"),
        }
    }

    #[test]
    fn test_wallet_source_env_variant() {
        let src = WalletSource::Env { key: "MY_KEY".into() };
        match src {
            WalletSource::Env { key } => assert_eq!(key, "MY_KEY"),
            _ => panic!("Expected Env variant"),
        }
    }

    #[test]
    fn test_proxy_config_no_auth() {
        let proxy = ProxyConfig {
            url: "http://10.0.0.1:3128".into(),
            username: None,
            password: None,
        };
        assert_eq!(proxy.url, "http://10.0.0.1:3128");
        assert!(proxy.username.is_none());
        assert!(proxy.password.is_none());
    }

    #[test]
    fn test_proxy_config_with_auth() {
        let proxy = ProxyConfig {
            url: "http://10.0.0.1:3128".into(),
            username: Some("admin".into()),
            password: Some("secret".into()),
        };
        assert_eq!(proxy.username.as_deref(), Some("admin"));
        assert_eq!(proxy.password.as_deref(), Some("secret"));
    }

    #[test]
    fn test_chain_config_fields() {
        let chain = ChainConfig {
            name: "Ethereum".into(),
            rpc_endpoint: "https://eth.llamarpc.com".into(),
            chain_id: 1,
        };
        assert_eq!(chain.name, "Ethereum");
        assert_eq!(chain.rpc_endpoint, "https://eth.llamarpc.com");
        assert_eq!(chain.chain_id, 1);
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let cfg = SpamConfig {
            rpc_url: "https://rpc.test.com".into(),
            chain_id: 42,
            target_tps: 25,
            duration_seconds: Some(1800),
            wallet_source: WalletSource::File {
                path: "keys.json".into(),
                encrypted: true,
            },
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: SpamConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.rpc_url, cfg.rpc_url);
        assert_eq!(deserialized.chain_id, cfg.chain_id);
        assert_eq!(deserialized.target_tps, cfg.target_tps);
        assert_eq!(deserialized.duration_seconds, cfg.duration_seconds);
    }

    #[test]
    fn test_wallet_source_file_toml() {
        let toml = r#"
rpc_url = "https://rpc.test.com"
chain_id = 42
target_tps = 10
wallet_source = { File = { path = "wallets.json", encrypted = true } }
"#;
        let cfg: SpamConfig = toml::from_str(toml).unwrap();
        match cfg.wallet_source {
            WalletSource::File { path, encrypted } => {
                assert_eq!(path, "wallets.json");
                assert!(encrypted);
            },
            _ => panic!("Expected File variant"),
        }
    }

    #[test]
    fn test_wallet_source_env_toml() {
        let toml = r#"
rpc_url = "https://rpc.test.com"
chain_id = 42
target_tps = 10
wallet_source = { Env = { key = "PRIVATE_KEY" } }
"#;
        let cfg: SpamConfig = toml::from_str(toml).unwrap();
        match cfg.wallet_source {
            WalletSource::Env { key } => assert_eq!(key, "PRIVATE_KEY"),
            _ => panic!("Expected Env variant"),
        }
    }

    #[test]
    fn test_wallet_source_json_roundtrip() {
        let src = WalletSource::File {
            path: "/tmp/wallet.json".into(),
            encrypted: false,
        };
        let json = serde_json::to_string(&src).unwrap();
        let deserialized: WalletSource = serde_json::from_str(&json).unwrap();
        match deserialized {
            WalletSource::File { path, encrypted } => {
                assert_eq!(path, "/tmp/wallet.json");
                assert!(!encrypted);
            },
            _ => panic!("Expected File variant"),
        }
    }

    #[test]
    fn test_proxy_config_serde_roundtrip() {
        let proxy = ProxyConfig {
            url: "http://proxy:8080".into(),
            username: Some("user".into()),
            password: Some("pass".into()),
        };
        let json = serde_json::to_string(&proxy).unwrap();
        let deserialized: ProxyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.url, proxy.url);
        assert_eq!(deserialized.username, proxy.username);
        assert_eq!(deserialized.password, proxy.password);
    }

    #[test]
    fn test_chain_config_serde_roundtrip() {
        let chain = ChainConfig {
            name: "TestNet".into(),
            rpc_endpoint: "https://test.rpc.com".into(),
            chain_id: 999,
        };
        let json = serde_json::to_string(&chain).unwrap();
        let deserialized: ChainConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, chain.name);
        assert_eq!(deserialized.chain_id, chain.chain_id);
    }
}
