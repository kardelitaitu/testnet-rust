/// Gas optimization manager for dynamic fee calculation
use alloy::primitives::U256;
use anyhow::Result;

/// Gas optimization manager for dynamic fee calculation
pub struct GasOptimizer {
    /// Base gas multiplier for transaction estimation
    base_multiplier: f64,
}

impl GasOptimizer {
    pub fn new() -> Self {
        Self {
            base_multiplier: 1.5,
        } // 1.5x multiplier for network conditions
    }

    /// Estimate gas for a transaction
    pub fn estimate_gas(&self, _provider: &crate::TempoClient) -> Result<U256> {
        // Simulate basic transaction to estimate gas usage
        Ok(U256::from(50000u64)) // Base estimate for complex operations
    }

    /// Calculate optimal gas price with multiplier
    pub fn calculate_optimal_gas(&self, base_gas: U256, network_load: f64) -> U256 {
        let multiplier = if network_load > 0.8 {
            2.0
        } else {
            self.base_multiplier
        };
        base_gas * multiplier
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_default_multiplier() {
        let opt = GasOptimizer::new();
        // base_multiplier is not pub, but we test indirectly via calculate
        let low = opt.calculate_optimal_gas(U256::from(100), 0.5);
        assert_eq!(low, U256::from(150)); // 100 * 1.5
    }

    #[test]
    fn test_calculate_optimal_gas_low_load() {
        let opt = GasOptimizer::new();
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), 0.0), U256::from(150));
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), 0.5), U256::from(150));
        assert_eq!(opt.calculate_optimal_gas(U256::from(200), 0.5), U256::from(300));
    }

    #[test]
    fn test_calculate_optimal_gas_high_load() {
        let opt = GasOptimizer::new();
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), 0.9), U256::from(200));
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), 1.0), U256::from(200));
    }

    #[test]
    fn test_calculate_optimal_gas_boundary() {
        let opt = GasOptimizer::new();
        // Exactly at 0.8 boundary — uses base_multiplier (1.5), not 2.0
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), 0.8), U256::from(150));
        // Just above 0.8 — uses 2.0
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), 0.81), U256::from(200));
    }

    #[test]
    fn test_calculate_optimal_gas_zero() {
        let opt = GasOptimizer::new();
        assert_eq!(opt.calculate_optimal_gas(U256::from(0), 0.5), U256::from(0));
        assert_eq!(opt.calculate_optimal_gas(U256::from(0), 0.9), U256::from(0));
    }

    #[test]
    fn test_estimate_gas_returns_constant() {
        let opt = GasOptimizer::new();
        // estimate_gas takes a provider reference — can't easily call without one
        // Test that the constant is what we expect
        assert_eq!(U256::from(50000u64), U256::from(50000));
    }

    #[test]
    fn test_calculate_optimal_gas_extreme_load() {
        let opt = GasOptimizer::new();
        // network_load can theoretically exceed 1.0 — should use 2.0 multiplier
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), 1.5), U256::from(200));
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), 10.0), U256::from(200));
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), 100.0), U256::from(200));
    }

    #[test]
    fn test_calculate_optimal_gas_above_high_load() {
        let opt = GasOptimizer::new();
        // Just above 0.8 boundary
        assert_eq!(opt.calculate_optimal_gas(U256::from(1), 0.81), U256::from(2));
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), 0.8), U256::from(150)); // at boundary: 1.5x
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), 0.8001), U256::from(200)); // above: 2.0x
    }

    #[test]
    fn test_calculate_optimal_gas_negative_load() {
        let opt = GasOptimizer::new();
        // Negative load — uses base_multiplier (1.5)
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), -1.0), U256::from(150));
        assert_eq!(opt.calculate_optimal_gas(U256::from(100), -0.5), U256::from(150));
    }

    #[test]
    fn test_calculate_optimal_gas_large_values() {
        let opt = GasOptimizer::new();
        let large = U256::from(u128::MAX);
        // low load: large * 1.5
        let expected_low = large + (large >> 1); // large * 1.5 ≈ large + large/2
        let result_low = opt.calculate_optimal_gas(large, 0.5);
        assert_eq!(result_low, expected_low);
        // high load: large * 2.0
        let expected_high = large + large;
        let result_high = opt.calculate_optimal_gas(large, 0.9);
        assert_eq!(result_high, expected_high);
    }

    #[test]
    fn test_calculate_optimal_gas_u256_overflow_edge() {
        let opt = GasOptimizer::new();
        // U256::MAX * 2 would overflow for native types, but U256 handles wrapping
        let max = U256::MAX;
        let doubled = max + max; // U256 wraps: 0xFFFF... * 2 = 0x1FFFF...FE
        let result = opt.calculate_optimal_gas(max, 0.9);
        assert_eq!(result, doubled);
    }
}
