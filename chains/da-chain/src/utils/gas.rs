use anyhow::Result;
use core_logic::GasConfig;
use ethers::prelude::*;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct GasManager {
    config: GasConfig,
    provider: Arc<Provider<Http>>,
    min_fee_gwei: f64,
}

impl GasManager {
    pub const MAX_FEE_GWEI_DEFAULT: f64 = 20000.0;
    pub const PRIORITY_FEE_GWEI_DEFAULT: f64 = 10.0;
    pub const LIMIT_DEPLOY: U256 = U256([1_200_000, 0, 0, 0]);
    pub const LIMIT_TRANSFER: U256 = U256([21_000, 0, 0, 0]);
    pub const LIMIT_COUNTER_INTERACT: U256 = U256([50_000, 0, 0, 0]);
    pub const LIMIT_SEND_MEME: U256 = U256([100_000, 0, 0, 0]);

    pub fn new(
        provider: Arc<Provider<Http>>,
        min_fee_gwei: f64,
    ) -> Self {
        Self {
            config: GasConfig::new()
                .with_max_fee(Self::MAX_FEE_GWEI_DEFAULT)
                .with_priority_fee(Self::PRIORITY_FEE_GWEI_DEFAULT),
            provider,
            min_fee_gwei,
        }
    }

    pub fn with_config(mut self, config: GasConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn get_gas_price(&self) -> Result<U256> {
        Ok(self
            .provider
            .get_gas_price()
            .await
            .unwrap_or_else(|_| U256::zero()))
    }

    pub async fn get_fees(&self) -> Result<(U256, U256)> {
        let block = self
            .provider
            .get_block(BlockNumber::Latest)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to get latest block"))?;

        let gas_price = self.get_gas_price().await?;

        let config_max: U256 = parse_units(self.config.max_gwei(), "gwei")?.into();
        let config_prio: U256 = parse_units(self.config.priority_gwei(), "gwei")?.into();
        let min_fee_floor: U256 = parse_units(self.min_fee_gwei, "gwei")?.into();

        let Some(base_fee) = block.base_fee_per_gas else {
            let fee = gas_price.max(min_fee_floor).min(config_max);
            let prio = config_prio.min(fee);
            return Ok((fee, prio));
        };

        let base_fee_floor = base_fee.saturating_mul(U256::from(2u64)) + config_prio;
        let mut est_max = base_fee_floor.max(gas_price).max(min_fee_floor);
        let mut est_prio = config_prio;

        if let Ok((oracle_max, oracle_prio)) = self.provider.estimate_eip1559_fees(None).await {
            est_max = est_max.max(oracle_max);
            est_prio = est_prio.max(oracle_prio);
        }

        if est_max > config_max {
            est_max = config_max;
        }

        if est_prio > est_max {
            est_prio = est_max;
        }

        Ok((est_max, est_prio))
    }

    pub fn get_max_fee(&self, base_fee: U256) -> U256 {
        let priority_fee_wei: U256 =
            parse_units(self.config.priority_gwei(), "gwei").unwrap_or(U256::zero());
        let max_fee_wei = base_fee + priority_fee_wei;
        let max_configured_wei: U256 =
            parse_units(self.config.max_gwei(), "gwei").unwrap_or(U256::zero());

        max_fee_wei.min(max_configured_wei)
    }

    pub fn limit_deploy(&self) -> U256 {
        U256([self.config.limit_deploy(), 0, 0, 0])
    }

    pub fn limit_transfer(&self) -> U256 {
        U256([self.config.limit_transfer(), 0, 0, 0])
    }

    pub fn limit_counter_interact(&self) -> U256 {
        U256([self.config.limit_counter_interact(), 0, 0, 0])
    }

    pub fn limit_send_meme(&self) -> U256 {
        U256([self.config.limit_send_meme(), 0, 0, 0])
    }
}

pub fn parse_units<K>(amount: K, unit: &str) -> Result<U256>
where
    K: Into<f64> + std::fmt::Display + Copy,
{
    let amount_str = format!("{}", amount);
    Ok(ethers::utils::parse_units(amount_str, unit)?.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_limit_constants() {
        assert_eq!(GasManager::LIMIT_DEPLOY, U256([1_200_000, 0, 0, 0]));
        assert_eq!(GasManager::LIMIT_TRANSFER, U256([21_000, 0, 0, 0]));
        assert_eq!(GasManager::LIMIT_COUNTER_INTERACT, U256([50_000, 0, 0, 0]));
        assert_eq!(GasManager::LIMIT_SEND_MEME, U256([100_000, 0, 0, 0]));
    }

    #[test]
    fn test_gas_default_constants() {
        assert_eq!(GasManager::MAX_FEE_GWEI_DEFAULT, 20000.0);
        assert_eq!(GasManager::PRIORITY_FEE_GWEI_DEFAULT, 10.0);
    }

    #[test]
    fn test_parse_units_gwei() {
        let result = parse_units(1.0, "gwei").unwrap();
        assert_eq!(result, U256::from(1_000_000_000u64));
    }

    #[test]
    fn test_parse_units_ether() {
        let result = parse_units(1.0, "ether").unwrap();
        assert_eq!(result, U256::from(10u128.pow(18)));
    }

    #[test]
    fn test_parse_units_zero() {
        let result = parse_units(0.0, "gwei").unwrap();
        assert_eq!(result, U256::zero());
    }

    #[test]
    fn test_parse_units_fractional_gwei() {
        let result = parse_units(0.5, "gwei").unwrap();
        assert_eq!(result, U256::from(500_000_000u64));
    }

    #[test]
    fn test_get_max_fee_below_config_cap() {
        // With dummy provider — get_max_fee is pure logic using config only
        let provider = Arc::new(Provider::<Http>::try_from("http://localhost:9999").unwrap());
        let mgr = GasManager::new(provider, 0.0);
        // Default: max_gwei=20000, priority_gwei=10
        // base_fee = 100 gwei → max_fee = 100 + 10 = 110 gwei → capped at 20000 → 110
        let base_fee = U256::from(100_000_000_000u128); // 100 gwei
        let result = mgr.get_max_fee(base_fee);
        assert_eq!(result, U256::from(110_000_000_000u128), "100 gwei base + 10 gwei priority = 110 gwei");
    }

    #[test]
    fn test_get_max_fee_above_config_cap() {
        let provider = Arc::new(Provider::<Http>::try_from("http://localhost:9999").unwrap());
        let mgr = GasManager::new(provider, 0.0);
        // base_fee = 50000 gwei → max_fee = 50000 + 10 = 50010 → capped at 20000 → 20000
        let base_fee = U256::from(50_000_000_000_000u128); // 50000 gwei
        let result = mgr.get_max_fee(base_fee);
        assert_eq!(result, U256::from(20_000_000_000_000u128), "Capped at 20000 gwei");
    }

    #[test]
    fn test_get_max_fee_zero_base() {
        let provider = Arc::new(Provider::<Http>::try_from("http://localhost:9999").unwrap());
        let mgr = GasManager::new(provider, 0.0);
        // base_fee = 0 → max_fee = 0 + 10 = 10 gwei → capped at 20000 → 10
        let base_fee = U256::zero();
        let result = mgr.get_max_fee(base_fee);
        assert_eq!(result, U256::from(10_000_000_000u128), "0 base + 10 gwei priority = 10 gwei");
    }

    #[test]
    fn test_get_max_fee_at_config_cap() {
        let provider = Arc::new(Provider::<Http>::try_from("http://localhost:9999").unwrap());
        let mgr = GasManager::new(provider, 0.0);
        // base_fee = 19990 gwei → max_fee = 19990 + 10 = 20000 → exactly at cap
        let base_fee = U256::from(19_990_000_000_000u128);
        let result = mgr.get_max_fee(base_fee);
        assert_eq!(result, U256::from(20_000_000_000_000u128), "Exactly at 20000 gwei cap");
    }

    #[test]
    fn test_get_max_fee_with_custom_config() {
        let provider = Arc::new(Provider::<Http>::try_from("http://localhost:9999").unwrap());
        let mut mgr = GasManager::new(provider, 0.0);
        // Override config: max=50 gwei, priority=2 gwei
        let custom = GasConfig::new()
            .with_max_fee(50.0)
            .with_priority_fee(2.0);
        mgr = mgr.with_config(custom);
        // base_fee = 40 gwei → max_fee = 40 + 2 = 42 gwei → capped at 50 → 42
        let base_fee = U256::from(40_000_000_000u128);
        let result = mgr.get_max_fee(base_fee);
        assert_eq!(result, U256::from(42_000_000_000u128), "40 base + 2 priority = 42 gwei");
    }

    #[test]
    fn test_limit_deploy_returns_config_value() {
        let provider = Arc::new(Provider::<Http>::try_from("http://localhost:9999").unwrap());
        let mgr = GasManager::new(provider, 0.0);
        assert_eq!(mgr.limit_deploy(), U256::from(1_200_000u64));
        assert_eq!(mgr.limit_transfer(), U256::from(21_000u64));
        assert_eq!(mgr.limit_counter_interact(), U256::from(50_000u64));
        assert_eq!(mgr.limit_send_meme(), U256::from(100_000u64));
    }
}
