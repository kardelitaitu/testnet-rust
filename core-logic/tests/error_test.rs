use core_logic::error::{ConfigError, CoreError, DatabaseError, NetworkError, SecurityError, WalletError};

// ─── CoreError ─────────────────────────────────────────

#[test]
fn test_core_error_display_config() {
    let err = CoreError::Config(ConfigError::MissingField {
        field: "rpc_url".into(),
    });
    let msg = format!("{}", err);
    assert!(msg.contains("rpc_url"));
}

#[test]
fn test_core_error_display_database() {
    let err = CoreError::Database(DatabaseError::PoolExhausted { max_size: 10 });
    let msg = format!("{}", err);
    assert!(msg.contains("10"));
}

#[test]
fn test_core_error_display_wallet() {
    let err = CoreError::Wallet(WalletError::InvalidKeyFormat);
    let msg = format!("{}", err);
    assert!(msg.contains("key"));
}

#[test]
fn test_core_error_display_network() {
    let err = CoreError::Network(NetworkError::Timeout {
        timeout_ms: 5000,
        endpoint: "https://rpc.example.com".into(),
    });
    let msg = format!("{}", err);
    assert!(msg.contains("5000"));
    assert!(msg.contains("rpc.example.com"));
}

#[test]
fn test_core_error_display_security() {
    let err = CoreError::Security(SecurityError::PasswordRequired);
    let msg = format!("{}", err);
    assert!(msg.contains("Password"));
}

#[test]
fn test_core_error_unknown() {
    let err = CoreError::Unknown {
        message: "something broke".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("something broke"));
}

// ─── From impls ────────────────────────────────────────

#[test]
fn test_core_error_from_config() {
    let config_err = ConfigError::FileNotFound {
        path: "config.toml".into(),
    };
    let core: CoreError = config_err.into();
    match core {
        CoreError::Config(_) => {}, // expected
        _ => panic!("Expected Config variant"),
    }
}

#[test]
fn test_core_error_from_wallet() {
    let wallet_err = WalletError::NotFound { index: 5, total: 10 };
    let core: CoreError = wallet_err.into();
    match core {
        CoreError::Wallet(_) => {}, // expected
        _ => panic!("Expected Wallet variant"),
    }
}

#[test]
fn test_core_error_from_network() {
    let net_err = NetworkError::ConnectionRefused {
        endpoint: "https://rpc.com".into(),
        reason: "timeout".into(),
    };
    let core: CoreError = net_err.into();
    match core {
        CoreError::Network(_) => {}, // expected
        _ => panic!("Expected Network variant"),
    }
}

#[test]
fn test_core_error_from_security() {
    let sec_err = SecurityError::CryptographyFailed {
        reason: "bad decrypt".into(),
    };
    let core: CoreError = sec_err.into();
    match core {
        CoreError::Security(_) => {}, // expected
        _ => panic!("Expected Security variant"),
    }
}

// ─── ConfigError ───────────────────────────────────────

#[test]
fn test_config_error_invalid_rpc_url() {
    let err = ConfigError::InvalidRpcUrl {
        url: "not_a_url".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("not_a_url"));
}

#[test]
fn test_config_error_missing_field() {
    let err = ConfigError::MissingField {
        field: "chain_id".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("chain_id"));
}

#[test]
fn test_config_error_invalid_value() {
    let err = ConfigError::InvalidValue {
        field: "tps".into(),
        reason: "must be positive".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("tps"));
    assert!(msg.contains("positive"));
}

#[test]
fn test_config_error_file_not_found() {
    let err = ConfigError::FileNotFound {
        path: "./configs/main.toml".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("configs/main.toml"));
}

// ─── WalletError ───────────────────────────────────────

#[test]
fn test_wallet_error_decryption_failed() {
    let err = WalletError::DecryptionFailed {
        path: "wallet-json/0001.json".into(),
        reason: "wrong password".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("0001.json"));
    assert!(msg.contains("wrong password"));
}

#[test]
fn test_wallet_error_not_found() {
    let err = WalletError::NotFound { index: 99, total: 5 };
    let msg = format!("{}", err);
    assert!(msg.contains("99"));
    assert!(msg.contains("5"));
}

#[test]
fn test_wallet_error_invalid_key_format() {
    let err = WalletError::InvalidKeyFormat;
    let msg = format!("{}", err);
    assert!(msg.contains("hex"));
}

#[test]
fn test_wallet_error_invalid_key_length() {
    let err = WalletError::InvalidKeyLength { length: 32 };
    let msg = format!("{}", err);
    assert!(msg.contains("32"));
}

#[test]
fn test_wallet_error_address_mismatch() {
    let err = WalletError::AddressMismatch {
        expected: "0xabc".into(),
        actual: "0xdef".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("0xabc"));
    assert!(msg.contains("0xdef"));
}

// ─── DatabaseError ─────────────────────────────────────

#[test]
fn test_database_error_pool_exhausted() {
    let err = DatabaseError::PoolExhausted { max_size: 50 };
    let msg = format!("{}", err);
    assert!(msg.contains("50"));
}

#[test]
fn test_database_error_lock_timeout() {
    let err = DatabaseError::LockTimeout;
    let msg = format!("{}", err);
    assert!(msg.to_lowercase().contains("lock"));
}

#[test]
fn test_database_error_transaction_failed() {
    let err = DatabaseError::TransactionFailed { msg: "deadlock".into() };
    let msg = format!("{}", err);
    assert!(msg.contains("deadlock"));
}

#[test]
fn test_database_error_not_found() {
    let err = DatabaseError::NotFound {
        key: "wallet_0001".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("wallet_0001"));
}

// ─── NetworkError ──────────────────────────────────────

#[test]
fn test_network_error_timeout() {
    let err = NetworkError::Timeout {
        timeout_ms: 10000,
        endpoint: "https://eth.com".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("10000"));
    assert!(msg.contains("eth.com"));
}

#[test]
fn test_network_error_rate_limited() {
    let err = NetworkError::RateLimited {
        endpoint: "https://infura.io".into(),
        retry_after: 30,
    };
    let msg = format!("{}", err);
    assert!(msg.contains("30"));
}

#[test]
fn test_network_error_connection_refused() {
    let err = NetworkError::ConnectionRefused {
        endpoint: "https://node.com".into(),
        reason: "connection reset".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("node.com"));
    assert!(msg.contains("connection reset"));
}

#[test]
fn test_network_error_http_error() {
    let err = NetworkError::HttpError {
        status_code: 429,
        endpoint: "https://rpc.com".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("429"));
}

#[test]
fn test_network_error_invalid_response() {
    let err = NetworkError::InvalidResponse {
        endpoint: "https://api.com".into(),
        reason: "unexpected format".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("api.com"));
    assert!(msg.contains("unexpected format"));
}

// ─── SecurityError ─────────────────────────────────────

#[test]
fn test_security_error_password_required() {
    let err = SecurityError::PasswordRequired;
    let msg = format!("{}", err);
    assert!(msg.contains("Password"));
}

#[test]
fn test_security_error_cryptography_failed() {
    let err = SecurityError::CryptographyFailed {
        reason: "AEAD error".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("AEAD"));
}

#[test]
fn test_security_error_signature_verification() {
    let err = SecurityError::SignatureVerificationFailed;
    let msg = format!("{}", err);
    assert!(msg.to_lowercase().contains("signature"));
}

#[test]
fn test_security_error_invalid_nonce() {
    let err = SecurityError::InvalidNonceState {
        state: "replay detected".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("replay"));
}
