//! Tempo Alloy Client - Off-chain client for Tempo blockchain using Alloy v1.0
//!
//! This crate provides type-safe interaction with the Tempo blockchain using
//! the modern Alloy library with significant performance improvements over ethers-rs.

#![warn(unused_crate_dependencies)]

pub mod provider;
pub mod tasks;
pub mod utils;

pub use provider::{TempoClient, TempoProvider};
pub use tasks::{TaskContext, TempoTask};
