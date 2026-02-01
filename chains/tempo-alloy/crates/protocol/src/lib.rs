//! Tempo Protocol Types
//!
//! Core types for the Tempo blockchain protocol including transactions,
//! headers, and block types.

pub use alloy_consensus::Header;

pub mod transaction;
pub use transaction::{
    Call, MAX_WEBAUTHN_SIGNATURE_LENGTH, P256_SIGNATURE_LENGTH, SECP256K1_SIGNATURE_LENGTH,
    SignatureType, TempoSignature, TempoTransaction, TempoTxEnvelope, TempoTxType,
};

mod header;
pub use header::TempoHeader;

mod subblock;
pub use subblock::{RecoveredSubBlock, SignedSubBlock, SubBlock, SubBlockMetadata, SubBlockVersion};

/// Tempo block type
pub type Block = alloy_consensus::Block<TempoTxEnvelope, TempoHeader>;

/// Tempo block body
pub type BlockBody = alloy_consensus::BlockBody<TempoTxEnvelope, TempoHeader>;
