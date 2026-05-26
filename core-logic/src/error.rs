//! # Core Error Types
//!
//! Centralized error definitions for the core-logic crate.
//! All errors implement `std::error::Error` and `std::fmt::Display`.

use thiserror::Error;

/// Unified error type for core-logic operations.
///
/// This enum wraps all specific error types and provides a unified
/// error interface for the application layer.
///
/// ```
/// use core_logic::error::{CoreError, NetworkError, ConfigError};
///
/// // Errors are constructed via their specific type and auto-converted
/// let net_err: CoreError = NetworkError::Timeout {
///     timeout_ms: 5000,
///     endpoint: "http://rpc.example.com".into(),
/// }.into();
/// assert!(net_err.to_string().contains("5000"));
///
/// // The `Unknown` variant handles unclassified errors
/// let unknown = CoreError::Unknown {
///     message: "service unavailable".into(),
/// };
/// assert!(unknown.to_string().contains("service unavailable"));
/// ```
#[derive(Error, Debug)]
pub enum CoreError {
    #[error(transparent)]
    Config(ConfigError),

    #[error("Database error: {0}")]
    Database(DatabaseError),

    #[error(transparent)]
    Wallet(WalletError),

    #[error(transparent)]
    Network(NetworkError),

    #[error(transparent)]
    Security(SecurityError),

    #[error("Unknown error: {message}")]
    Unknown { message: String },
}

impl From<ConfigError> for CoreError {
    fn from(e: ConfigError) -> Self {
        CoreError::Config(e)
    }
}

impl From<WalletError> for CoreError {
    fn from(e: WalletError) -> Self {
        CoreError::Wallet(e)
    }
}

impl From<NetworkError> for CoreError {
    fn from(e: NetworkError) -> Self {
        CoreError::Network(e)
    }
}

impl From<SecurityError> for CoreError {
    fn from(e: SecurityError) -> Self {
        CoreError::Security(e)
    }
}

/// Configuration-related errors
#[derive(Error, Debug, Clone)]
pub enum ConfigError {
    #[error("Invalid RPC URL format: '{url}'")]
    InvalidRpcUrl { url: String },

    #[error("Missing required configuration field: '{field}'")]
    MissingField { field: String },

    #[error("Invalid value for '{field}': {reason}")]
    InvalidValue { field: String, reason: String },

    #[error("Parse error for '{field}': {source}")]
    ParseError {
        field: String,
        #[source]
        source: std::num::ParseIntError,
    },

    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("I/O error reading {path}: {msg}")]
    IoError { path: String, msg: String },
}

/// Wallet and cryptographic operation errors
#[derive(Error, Debug, Clone)]
pub enum WalletError {
    #[error("Decryption failed for wallet at '{path}': {reason}")]
    DecryptionFailed { path: String, reason: String },

    #[error("Wallet not found at index {index} (total wallets: {total})")]
    NotFound { index: usize, total: usize },

    #[error("Invalid private key format: expected hex string")]
    InvalidKeyFormat,

    #[error("Private key too short: expected 64 hex chars, got {length}")]
    InvalidKeyLength { length: usize },

    #[error("Wallet address mismatch: expected {expected}, got {actual}")]
    AddressMismatch { expected: String, actual: String },
}

/// Database operation errors
#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection pool exhausted (max: {max_size})")]
    PoolExhausted { max_size: u32 },

    #[error("Database lock timeout")]
    LockTimeout,

    #[error("Transaction failed: {msg}")]
    TransactionFailed { msg: String },

    #[error("Migration failed: {msg}")]
    MigrationFailed { msg: String },

    #[error("Query returned no rows for key: {key}")]
    NotFound { key: String },

    #[error("Constraint violation: {constraint}")]
    ConstraintViolation { constraint: String },
}

/// Network and RPC-related errors
#[derive(Error, Debug, Clone)]
pub enum NetworkError {
    #[error("RPC request timeout after {timeout_ms}ms to {endpoint}")]
    Timeout { timeout_ms: u64, endpoint: String },

    #[error("Rate limited by {endpoint}: retry after {retry_after}s")]
    RateLimited { endpoint: String, retry_after: u64 },

    #[error("Connection refused to {endpoint}: {reason}")]
    ConnectionRefused { endpoint: String, reason: String },

    #[error("HTTP error {status_code} from {endpoint}")]
    HttpError { status_code: u16, endpoint: String },

    #[error("Invalid response from {endpoint}: {reason}")]
    InvalidResponse { endpoint: String, reason: String },
}

/// Security-related errors
#[derive(Error, Debug, Clone)]
pub enum SecurityError {
    #[error("Password required but not provided")]
    PasswordRequired,

    #[error("Encryption/decryption failed: {reason}")]
    CryptographyFailed { reason: String },

    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    #[error("Invalid nonce state: {state}")]
    InvalidNonceState { state: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_error_invalid_rpc_url() {
        let err = ConfigError::InvalidRpcUrl {
            url: "not-a-url".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Invalid RPC URL format"));
        assert!(msg.contains("not-a-url"));
    }

    #[test]
    fn test_config_error_missing_field() {
        let err = ConfigError::MissingField {
            field: "rpc_url".into(),
        };
        assert_eq!(
            err.to_string(),
            "Missing required configuration field: 'rpc_url'"
        );
    }

    #[test]
    fn test_config_error_invalid_value() {
        let err = ConfigError::InvalidValue {
            field: "tps".into(),
            reason: "must be positive".into(),
        };
        assert!(err.to_string().contains("tps"));
        assert!(err.to_string().contains("must be positive"));
    }

    #[test]
    fn test_config_error_file_not_found() {
        let err = ConfigError::FileNotFound {
            path: "/tmp/config.toml".into(),
        };
        assert!(err.to_string().contains("/tmp/config.toml"));
    }

    #[test]
    fn test_wallet_error_decryption_failed() {
        let err = WalletError::DecryptionFailed {
            path: "wallet.json".into(),
            reason: "bad password".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("wallet.json"));
        assert!(msg.contains("bad password"));
    }

    #[test]
    fn test_wallet_error_not_found() {
        let err = WalletError::NotFound { index: 5, total: 3 };
        assert_eq!(
            err.to_string(),
            "Wallet not found at index 5 (total wallets: 3)"
        );
    }

    #[test]
    fn test_wallet_error_invalid_key_format() {
        let err = WalletError::InvalidKeyFormat;
        assert_eq!(
            err.to_string(),
            "Invalid private key format: expected hex string"
        );
    }

    #[test]
    fn test_wallet_error_invalid_key_length() {
        let err = WalletError::InvalidKeyLength { length: 10 };
        assert!(err.to_string().contains("10"));
    }

    #[test]
    fn test_database_error_pool_exhausted() {
        let err = DatabaseError::PoolExhausted { max_size: 20 };
        assert!(err.to_string().contains("20"));
    }

    #[test]
    fn test_database_error_lock_timeout() {
        let err = DatabaseError::LockTimeout;
        assert_eq!(err.to_string(), "Database lock timeout");
    }

    #[test]
    fn test_database_error_transaction_failed() {
        let err = DatabaseError::TransactionFailed {
            msg: "disk full".into(),
        };
        assert!(err.to_string().contains("disk full"));
    }

    #[test]
    fn test_database_error_not_found() {
        let err = DatabaseError::NotFound {
            key: "wallet_5".into(),
        };
        assert!(err.to_string().contains("wallet_5"));
    }

    #[test]
    fn test_database_error_migration_failed() {
        let err = DatabaseError::MigrationFailed {
            msg: "schema v2→v3 failed".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Migration failed"));
        assert!(msg.contains("schema v2→v3 failed"));
    }

    #[test]
    fn test_database_error_constraint_violation() {
        let err = DatabaseError::ConstraintViolation {
            constraint: "UNIQUE constraint failed: wallet.address".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Constraint violation"));
        assert!(msg.contains("wallet.address"));
    }

    #[test]
    fn test_network_error_timeout() {
        let err = NetworkError::Timeout {
            timeout_ms: 5000,
            endpoint: "http://rpc.com".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("5000"));
        assert!(msg.contains("rpc.com"));
    }

    #[test]
    fn test_network_error_rate_limited() {
        let err = NetworkError::RateLimited {
            endpoint: "http://api.com".into(),
            retry_after: 30,
        };
        assert!(err.to_string().contains("30"));
    }

    #[test]
    fn test_network_error_http_error() {
        let err = NetworkError::HttpError {
            status_code: 429,
            endpoint: "http://rpc.com".into(),
        };
        assert!(err.to_string().contains("429"));
    }

    #[test]
    fn test_security_error_password_required() {
        let err = SecurityError::PasswordRequired;
        assert_eq!(err.to_string(), "Password required but not provided");
    }

    #[test]
    fn test_security_error_cryptography_failed() {
        let err = SecurityError::CryptographyFailed {
            reason: "invalid key".into(),
        };
        assert!(err.to_string().contains("invalid key"));
    }

    #[test]
    fn test_security_error_signature_verification() {
        let err = SecurityError::SignatureVerificationFailed;
        assert_eq!(err.to_string(), "Signature verification failed");
    }

    #[test]
    fn test_security_error_invalid_nonce_state() {
        let err = SecurityError::InvalidNonceState {
            state: "too low".into(),
        };
        assert!(err.to_string().contains("too low"));
    }

    #[test]
    fn test_core_error_from_config() {
        let config_err = ConfigError::FileNotFound {
            path: "cfg.toml".into(),
        };
        let core: CoreError = config_err.into();
        match core {
            CoreError::Config(_) => {}
            _ => panic!("Expected CoreError::Config variant"),
        }
        assert!(core.to_string().contains("cfg.toml"));
    }

    #[test]
    fn test_core_error_from_wallet() {
        let wallet_err = WalletError::InvalidKeyFormat;
        let core: CoreError = wallet_err.into();
        match core {
            CoreError::Wallet(_) => {}
            _ => panic!("Expected CoreError::Wallet variant"),
        }
    }

    #[test]
    fn test_core_error_from_network() {
        let net_err = NetworkError::Timeout {
            timeout_ms: 1000,
            endpoint: "x".into(),
        };
        let core: CoreError = net_err.into();
        match core {
            CoreError::Network(_) => {}
            _ => panic!("Expected CoreError::Network variant"),
        }
    }

    #[test]
    fn test_core_error_from_security() {
        let sec_err = SecurityError::PasswordRequired;
        let core: CoreError = sec_err.into();
        match core {
            CoreError::Security(_) => {}
            _ => panic!("Expected CoreError::Security variant"),
        }
    }

    #[test]
    fn test_core_error_unknown_variant() {
        let err = CoreError::Unknown {
            message: "something broke".into(),
        };
        assert_eq!(err.to_string(), "Unknown error: something broke");
    }

    #[test]
    fn test_config_error_clone() {
        let err = ConfigError::IoError {
            path: "file.txt".into(),
            msg: "permission denied".into(),
        };
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }

    #[test]
    fn test_wallet_error_clone() {
        let err = WalletError::AddressMismatch {
            expected: "0xabc".into(),
            actual: "0xdef".into(),
        };
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }

    #[test]
    fn test_network_error_clone() {
        let err = NetworkError::ConnectionRefused {
            endpoint: "x".into(),
            reason: "timeout".into(),
        };
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }
}
