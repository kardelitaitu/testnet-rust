use ethers::types::U256;

#[derive(Debug, Clone)]
pub struct GasManager {
    max_gwei: f64,
    priority_gwei: f64,
}

impl GasManager {
    pub const MAX_FEE_GWEI_DEFAULT: f64 = 0.000000009;
    pub const PRIORITY_FEE_GWEI_DEFAULT: f64 = 0.000000001;

    pub const LIMIT_DEPLOY: U256 = U256([1_200_000, 0, 0, 0]);
    pub const LIMIT_TRANSFER: U256 = U256([21_000, 0, 0, 0]);
    pub const LIMIT_COUNTER_INTERACT: U256 = U256([50_000, 0, 0, 0]);
    pub const LIMIT_SEND_MEME: U256 = U256([100_000, 0, 0, 0]);

    pub fn new() -> Self {
        Self {
            max_gwei: Self::MAX_FEE_GWEI_DEFAULT,
            priority_gwei: Self::PRIORITY_FEE_GWEI_DEFAULT,
        }
    }

    pub fn with_config(mut self, max_gwei: f64, priority_gwei: f64) -> Self {
        self.max_gwei = max_gwei;
        self.priority_gwei = priority_gwei;
        self
    }

    pub fn calculate_max_fee(&self, base_fee: U256) -> U256 {
        let priority_fee_wei = parse_units_precise(self.priority_gwei, "gwei");
        let max_fee_wei = base_fee + priority_fee_wei;

        let max_configured_wei = parse_units_precise(self.max_gwei, "gwei");

        if max_fee_wei > max_configured_wei {
            max_configured_wei
        } else {
            max_fee_wei
        }
    }

    pub fn calculate_priority_fee(&self) -> U256 {
        parse_units_precise(self.priority_gwei, "gwei")
    }

    pub fn calculate_deploy_cost(&self, base_fee: U256) -> U256 {
        Self::LIMIT_DEPLOY * self.calculate_max_fee(base_fee)
    }

    pub fn calculate_transfer_cost(&self, base_fee: U256) -> U256 {
        Self::LIMIT_TRANSFER * self.calculate_max_fee(base_fee)
    }
}

pub fn parse_units_precise(amount: f64, unit: &str) -> U256 {
    if unit == "gwei" {
        let wei_amount = (amount * 1_000_000_000.0) as u64;
        U256::from(wei_amount)
    } else if unit == "ether" {
        let wei_amount = (amount * 1_000_000_000_000_000_000.0) as u128;
        U256::from(wei_amount)
    } else {
        U256::from(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_limits_are_correct() {
        assert_eq!(GasManager::LIMIT_DEPLOY, U256([1_200_000, 0, 0, 0]));
        assert_eq!(GasManager::LIMIT_TRANSFER, U256([21_000, 0, 0, 0]));
        assert_eq!(GasManager::LIMIT_COUNTER_INTERACT, U256([50_000, 0, 0, 0]));
        assert_eq!(GasManager::LIMIT_SEND_MEME, U256([100_000, 0, 0, 0]));
    }

    #[test]
    fn test_calculate_max_fee_below_cap() {
        let gm = GasManager::new().with_config(100.0, 10.0);

        let base_fee = parse_units_precise(50.0, "gwei");
        let max_fee = gm.calculate_max_fee(base_fee);

        let expected = parse_units_precise(60.0, "gwei");
        assert_eq!(max_fee, expected);
    }

    #[test]
    fn test_calculate_max_fee_above_cap() {
        let gm = GasManager::new().with_config(50.0, 10.0);

        let base_fee = parse_units_precise(100.0, "gwei");
        let max_fee = gm.calculate_max_fee(base_fee);

        let expected = parse_units_precise(50.0, "gwei");
        assert_eq!(max_fee, expected);
    }

    #[test]
    fn test_calculate_priority_fee() {
        let gm = GasManager::new().with_config(100.0, 5.0);

        let priority_fee = gm.calculate_priority_fee();
        let expected = parse_units_precise(5.0, "gwei");

        assert_eq!(priority_fee, expected);
    }

    #[test]
    fn test_calculate_deploy_cost() {
        let gm = GasManager::new().with_config(100.0, 10.0);
        let base_fee = parse_units_precise(50.0, "gwei");

        let cost = gm.calculate_deploy_cost(base_fee);

        let max_fee = parse_units_precise(60.0, "gwei");
        let expected = GasManager::LIMIT_DEPLOY * max_fee;
        assert_eq!(cost, expected);
    }

    #[test]
    fn test_calculate_transfer_cost() {
        let gm = GasManager::new().with_config(100.0, 10.0);
        let base_fee = parse_units_precise(50.0, "gwei");

        let cost = gm.calculate_transfer_cost(base_fee);

        let max_fee = parse_units_precise(60.0, "gwei");
        let expected = GasManager::LIMIT_TRANSFER * max_fee;
        assert_eq!(cost, expected);
    }

    #[test]
    fn test_parse_units_gwei() {
        let one_gwei = parse_units_precise(1.0, "gwei");
        assert_eq!(one_gwei, U256::from(1_000_000_000u64));

        let fifty_gwei = parse_units_precise(50.0, "gwei");
        assert_eq!(fifty_gwei, U256::from(50_000_000_000u64));
    }

    #[test]
    fn test_parse_units_ether() {
        let one_ether = parse_units_precise(1.0, "ether");
        assert_eq!(one_ether, U256::from(1_000_000_000_000_000_000u128));
    }

    #[test]
    fn test_default_priority_fee() {
        let gm = GasManager::new();
        let priority_fee = gm.calculate_priority_fee();
        assert_eq!(priority_fee, U256::from(1u64));
    }

    #[test]
    fn test_calculate_max_fee_with_default_config() {
        let gm = GasManager::new();

        // Use a very small base_fee that's below the cap
        let base_fee = parse_units_precise(0.000000001, "gwei");
        let max_fee = gm.calculate_max_fee(base_fee);

        // base_fee = 1 wei, priority = 1 wei, max = 2 wei (below cap of 9 wei)
        assert_eq!(max_fee, U256::from(2u64));
    }

    #[test]
    fn test_calculate_max_fee_default_above_cap() {
        let gm = GasManager::new();

        let base_fee = parse_units_precise(50.0, "gwei");
        let max_fee = gm.calculate_max_fee(base_fee);

        // Should be capped at 0.000000009 gwei = 9 wei
        assert_eq!(max_fee, U256::from(9u64));
    }
}
