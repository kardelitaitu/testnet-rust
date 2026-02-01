use core_logic::config::{ChainConfig, ProxyConfig, SpamConfig, WalletSource};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
enum WalletSourceConfig {
    #[serde(rename = "file")]
    File { path: String, encrypted: bool },
    #[serde(rename = "env")]
    Env { key: String },
}

fn parse_wallet_source(source: &WalletSourceConfig) -> WalletSource {
    match source {
        WalletSourceConfig::File { path, encrypted } => WalletSource::File {
            path: path.clone(),
            encrypted: *encrypted,
        },
        WalletSourceConfig::Env { key } => WalletSource::Env { key: key.clone() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_source_file_parsing() {
        let source = WalletSourceConfig::File {
            path: "wallet.json".to_string(),
            encrypted: true,
        };

        let wallet_source = parse_wallet_source(&source);

        match wallet_source {
            WalletSource::File { path, encrypted } => {
                assert_eq!(path, "wallet.json");
                assert!(encrypted);
            }
            _ => panic!("Expected File variant"),
        }
    }

    #[test]
    fn test_wallet_source_env_parsing() {
        let source = WalletSourceConfig::Env {
            key: "PRIVATE_KEY".to_string(),
        };

        let wallet_source = parse_wallet_source(&source);

        match wallet_source {
            WalletSource::Env { key } => {
                assert_eq!(key, "PRIVATE_KEY");
            }
            _ => panic!("Expected Env variant"),
        }
    }

    #[test]
    fn test_spam_config_defaults() {
        let config = SpamConfig {
            rpc_url: "https://rpc.example.com".to_string(),
            chain_id: 1,
            target_tps: 100,
            duration_seconds: None,
            wallet_source: WalletSource::File {
                path: "wallets.json".to_string(),
                encrypted: true,
            },
        };

        assert_eq!(config.target_tps, 100);
        assert!(config.duration_seconds.is_none());
    }

    #[test]
    fn test_proxy_config_parsing() {
        let proxy = ProxyConfig {
            url: "http://proxy.example.com:8080".to_string(),
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
        };

        assert_eq!(proxy.url, "http://proxy.example.com:8080");
        assert_eq!(proxy.username, Some("user".to_string()));
        assert_eq!(proxy.password, Some("pass".to_string()));
    }

    #[test]
    fn test_proxy_config_no_auth() {
        let proxy = ProxyConfig {
            url: "http://proxy.example.com:8080".to_string(),
            username: None,
            password: None,
        };

        assert!(proxy.username.is_none());
        assert!(proxy.password.is_none());
    }

    #[test]
    fn test_chain_config() {
        let config = ChainConfig {
            name: "Ethereum Mainnet".to_string(),
            rpc_endpoint: "https://eth-mainnet.example.com".to_string(),
            chain_id: 1,
        };

        assert_eq!(config.name, "Ethereum Mainnet");
        assert_eq!(config.chain_id, 1);
    }

    #[test]
    fn test_wallet_source_clone() {
        let source = WalletSource::File {
            path: "test.json".to_string(),
            encrypted: true,
        };
        let cloned = source.clone();

        match cloned {
            WalletSource::File { path, encrypted } => {
                assert_eq!(path, "test.json");
                assert!(encrypted);
            }
            _ => panic!("Clone failed"),
        }
    }

    #[test]
    fn test_spam_config_clone() {
        let config = SpamConfig {
            rpc_url: "https://rpc.example.com".to_string(),
            chain_id: 1,
            target_tps: 50,
            duration_seconds: Some(3600),
            wallet_source: WalletSource::Env {
                key: "KEY".to_string(),
            },
        };
        let cloned = config.clone();

        assert_eq!(cloned.rpc_url, config.rpc_url);
        assert_eq!(cloned.chain_id, config.chain_id);
        assert_eq!(cloned.target_tps, config.target_tps);
        assert_eq!(cloned.duration_seconds, config.duration_seconds);
    }
}
