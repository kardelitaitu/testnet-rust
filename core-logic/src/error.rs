//! # Core Error Types
//!
//! Centralized error definitions for the core-logic crate.
//! All errors implement `std::error::Error` and `std::fmt::Display`.

use thiserror::Error;

/// Unified error type for core-logic operations.
///
/// This enum wraps all specific error types and provides a unified
/// error interface for the application layer.
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
