//! # Core Logic - Gas Configuration
//!
//! Generic gas configuration utilities that can be used across different
//! blockchain implementations. This module provides configuration only;
//! chain-specific implementations handle the actual gas estimation.

#![allow(dead_code)]

use serde::Deserialize;

/// Standard gas limits for common operations
#[derive(Debug, Clone, Copy)]
pub struct StandardGasLimits {
    pub deploy: u64,
    pub transfer: u64,
    pub counter_interact: u64,
    pub send_meme: u64,
}

impl Default for StandardGasLimits {
    fn default() -> Self {
        Self {
            deploy: 1_200_000,
            transfer: 21_000,
            counter_interact: 50_000,
            send_meme: 100_000,
        }
    }
}

/// Configuration for gas management
#[derive(Debug, Clone)]
pub struct GasConfig {
    pub max_gwei: f64,
    pub priority_gwei: f64,
    pub limits: StandardGasLimits,
}

impl Default for GasConfig {
    fn default() -> Self {
        Self {
            max_gwei: 2.5,      // 2.5 Gwei
            priority_gwei: 1.5, // 1.5 Gwei
            limits: StandardGasLimits::default(),
        }
    }
}

impl GasConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_fee(mut self, max_gwei: f64) -> Self {
        self.max_gwei = max_gwei;
        self
    }

    pub fn with_priority_fee(mut self, priority_gwei: f64) -> Self {
        self.priority_gwei = priority_gwei;
        self
    }

    pub fn max_gwei(&self) -> f64 {
        self.max_gwei
    }

    pub fn priority_gwei(&self) -> f64 {
        self.priority_gwei
    }

    pub fn limit_deploy(&self) -> u64 {
        self.limits.deploy
    }

    pub fn limit_transfer(&self) -> u64 {
        self.limits.transfer
    }

    pub fn limit_counter_interact(&self) -> u64 {
        self.limits.counter_interact
    }

    pub fn limit_send_meme(&self) -> u64 {
        self.limits.send_meme
    }
}

/// Convert gwei to wei as u64
pub fn gwei_to_wei(gwei: f64) -> u64 {
    (gwei * 1e9) as u64
}

/// Deserialize helper for GasConfig from TOML
#[derive(Deserialize)]
pub struct GasConfigToml {
    pub max_gwei: Option<f64>,
    pub priority_gwei: Option<f64>,
    pub limit_deploy: Option<u64>,
    pub limit_transfer: Option<u64>,
    pub limit_counter_interact: Option<u64>,
    pub limit_send_meme: Option<u64>,
}

impl From<GasConfigToml> for GasConfig {
    fn from(toml: GasConfigToml) -> Self {
        Self {
            max_gwei: toml.max_gwei.unwrap_or(0.000000009),
            priority_gwei: toml.priority_gwei.unwrap_or(0.000000001),
            limits: StandardGasLimits {
                deploy: toml.limit_deploy.unwrap_or(1_200_000),
                transfer: toml.limit_transfer.unwrap_or(21_000),
                counter_interact: toml.limit_counter_interact.unwrap_or(50_000),
                send_meme: toml.limit_send_meme.unwrap_or(100_000),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gwei_to_wei() {
        assert_eq!(gwei_to_wei(1.0), 1_000_000_000);
        assert_eq!(gwei_to_wei(0.5), 500_000_000);
        assert_eq!(gwei_to_wei(0.000000001), 1);
    }

    #[test]
    fn test_gas_config_defaults() {
        let config = GasConfig::default();
        assert_eq!(config.max_gwei, 2.5);
        assert_eq!(config.priority_gwei, 1.5);
        assert_eq!(config.limit_transfer(), 21_000);
    }

    #[test]
    fn test_gas_config_from_toml_defaults() {
        // When no fields are set, GasConfigToml uses its own defaults
        let toml = GasConfigToml {
            max_gwei: None,
            priority_gwei: None,
            limit_deploy: None,
            limit_transfer: None,
            limit_counter_interact: None,
            limit_send_meme: None,
        };
        let config: GasConfig = toml.into();
        assert_eq!(config.max_gwei, 0.000000009);
        assert_eq!(config.priority_gwei, 0.000000001);
    }

    #[test]
    fn test_gas_config_from_toml_overrides() {
        let toml = GasConfigToml {
            max_gwei: Some(100.0),
            priority_gwei: Some(2.0),
            limit_deploy: Some(500_000),
            limit_transfer: None,
            limit_counter_interact: None,
            limit_send_meme: None,
        };
        let config: GasConfig = toml.into();
        assert_eq!(config.max_gwei, 100.0);
        assert_eq!(config.priority_gwei, 2.0);
        assert_eq!(config.limit_deploy(), 500_000);
        assert_eq!(config.limit_transfer(), 21_000); // default
    }

    #[test]
    fn test_gas_config_builder() {
        let config = GasConfig::new()
            .with_max_fee(0.000000050)
            .with_priority_fee(0.000000002);

        assert_eq!(config.max_gwei(), 0.000000050);
        assert_eq!(config.priority_gwei(), 0.000000002);
    }

    #[test]
    fn test_gwei_to_wei_zero() {
        assert_eq!(gwei_to_wei(0.0), 0);
    }

    #[test]
    fn test_standard_gas_limits_default() {
        let limits = StandardGasLimits::default();
        assert_eq!(limits.deploy, 1_200_000);
        assert_eq!(limits.transfer, 21_000);
        assert_eq!(limits.counter_interact, 50_000);
        assert_eq!(limits.send_meme, 100_000);
    }

    #[test]
    fn test_gas_config_new_equals_default() {
        assert_eq!(GasConfig::new().max_gwei, GasConfig::default().max_gwei);
        assert_eq!(GasConfig::new().priority_gwei, GasConfig::default().priority_gwei);
    }

    #[test]
    fn test_gas_config_limit_counter_interact() {
        let config = GasConfig::default();
        assert_eq!(config.limit_counter_interact(), 50_000);
        assert_eq!(config.limit_send_meme(), 100_000);
        assert_eq!(config.limit_deploy(), 1_200_000);
    }

    #[test]
    fn test_gas_config_builder_both_fees() {
        let config = GasConfig::new().with_max_fee(5.0).with_priority_fee(0.5);
        assert_eq!(config.max_gwei(), 5.0);
        assert_eq!(config.priority_gwei(), 0.5);
    }

    // ---- Property-based tests ----

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn proptest_gwei_to_wei_no_panic(val: f64) {
            // Should never panic, regardless of input
            let _ = gwei_to_wei(val);
        }

        #[test]
        fn proptest_gwei_to_wei_monotonic(a: f64, b: f64) {
            // Filter out NaN which breaks ordering
            prop_assume!(!a.is_nan() && !b.is_nan());
            if a > b {
                let wa = gwei_to_wei(a);
                let wb = gwei_to_wei(b);
                assert!(wa >= wb, "gwei_to_wei({})={} should be >= gwei_to_wei({})={}", a, wa, b, wb);
            }
        }

        #[test]
        fn proptest_gas_config_builder_roundtrip(max_gwei: f64, priority: f64) {
            // Filter out NaN and infinity which GasConfig doesn't handle
            prop_assume!(max_gwei.is_finite() && max_gwei >= 0.0);
            prop_assume!(priority.is_finite() && priority >= 0.0);
            let config = GasConfig::new()
                .with_max_fee(max_gwei)
                .with_priority_fee(priority);
            assert!((config.max_gwei() - max_gwei).abs() < f64::EPSILON,
                "max_gwei roundtrip: expected {}, got {}", max_gwei, config.max_gwei());
            assert!((config.priority_gwei() - priority).abs() < f64::EPSILON,
                "priority roundtrip: expected {}, got {}", priority, config.priority_gwei());
        }
    }
}
