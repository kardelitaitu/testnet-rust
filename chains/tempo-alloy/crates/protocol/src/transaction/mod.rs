//! Tempo Transaction Types
//!
//! Defines the Tempo-specific transaction types and related utilities.

use alloy_primitives::{uint, Address, Bytes, B256, U256};
use alloy_rlp::{Decodable, Encodable};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Tempo transaction type ID
pub const TEMPO_TX_TYPE_ID: u8 = 0x76;

/// Tempo transaction type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TempoTxType {
    /// Legacy transaction type
    Legacy = 0,
    /// EIP-2930 access list transaction
    Eip2930 = 1,
    /// EIP-1559 dynamic fee transaction
    Eip1559 = 2,
    /// Tempo system transaction
    System = 0x76,
}

/// A single call within a Tempo transaction
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// Target address
    pub to: Address,
    /// Value to transfer
    pub value: U256,
    /// Calldata
    pub input: Bytes,
}

impl Call {
    /// Create a new call
    pub fn new(to: Address, input: Bytes) -> Self {
        Self {
            to,
            value: U256::ZERO,
            input,
        }
    }

    /// Set the value
    pub fn with_value(mut self, value: U256) -> Self {
        self.value = value;
        self
    }
}

/// Tempo transaction
///
/// A Tempo transaction consists of multiple calls that are executed atomically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempoTransaction {
    /// Chain ID
    pub chain_id: u64,
    /// Maximum priority fee per gas
    pub max_priority_fee_per_gas: u128,
    /// Maximum fee per gas
    pub max_fee_per_gas: u128,
    /// Gas limit
    pub gas_limit: u64,
    /// Calls to execute
    pub calls: Vec<Call>,
    /// Access list
    pub access_list: Vec<(Address, Vec<Address>)>,
    /// Nonce key (for 2D nonce)
    pub nonce_key: U256,
    /// Nonce
    pub nonce: u64,
    /// Valid before timestamp
    pub valid_before: Option<u64>,
    /// Valid after timestamp
    pub valid_after: Option<u64>,
    /// Fee token address
    pub fee_token: Option<Address>,
    /// Tempo authorization list
    pub tempo_authorization_list: Vec<Bytes>,
    /// Key authorization
    pub key_authorization: Option<Bytes>,
}

impl Default for TempoTransaction {
    fn default() -> Self {
        Self {
            chain_id: 42431,
            max_priority_fee_per_gas: 1_500_000_000,
            max_fee_per_gas: 150_000_000_000,
            gas_limit: 500_000,
            calls: Vec::new(),
            access_list: Vec::new(),
            nonce_key: U256::ZERO,
            nonce: 0,
            valid_before: None,
            valid_after: None,
            fee_token: Some(
                Address::from_str("0x20C0000000000000000000000000000000000000").unwrap(),
            ),
            tempo_authorization_list: Vec::new(),
            key_authorization: None,
        }
    }
}

impl TempoTransaction {
    /// Create a new empty transaction
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a call
    pub fn with_calls(mut self, calls: Vec<Call>) -> Self {
        self.calls = calls;
        self
    }

    /// Set nonce
    pub fn with_nonce(mut self, nonce_key: U256, nonce: u64) -> Self {
        self.nonce_key = nonce_key;
        self.nonce = nonce;
        self
    }

    /// Check if transaction is empty
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    /// Get number of calls
    pub fn len(&self) -> usize {
        self.calls.len()
    }
}

/// Signature type for Tempo transactions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempoSignature {
    /// Recovery id (v)
    pub v: u64,
    /// R component
    pub r: B256,
    /// S component
    pub s: B256,
}

/// Signature length constants
pub const SECP256K1_SIGNATURE_LENGTH: usize = 64;
pub const P256_SIGNATURE_LENGTH: usize = 64;
pub const MAX_WEBAUTHN_SIGNATURE_LENGTH: usize = 73;

/// Signature type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignatureType {
    /// SECP256K1 (standard Ethereum)
    Secp256k1 = 0,
    /// P-256 (WebAuthn)
    P256 = 1,
    /// WebAuthn with custom length
    WebAuthn = 2,
}

/// Call validation helper
pub fn validate_calls(calls: &[Call]) -> Result<(), &'static str> {
    if calls.is_empty() {
        return Err("No calls provided");
    }
    Ok(())
}
