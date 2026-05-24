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
    pub const MAX_FEE_GWEI_DEFAULT: f64 = 1.2;
    pub const PRIORITY_FEE_GWEI_DEFAULT: f64 = 0.0001;
    pub const LIMIT_DEPLOY: U256 = U256([1_200_000, 0, 0, 0]);
    pub const LIMIT_TRANSFER: U256 = U256([21_000, 0, 0, 0]);
    pub const LIMIT_COUNTER_INTERACT: U256 = U256([50_000, 0, 0, 0]);
    pub const LIMIT_SEND_MEME: U256 = U256([100_000, 0, 0, 0]);

    pub fn new(provider: Arc<Provider<Http>>, min_fee_gwei: f64) -> Self {
        Self::with_max(provider, min_fee_gwei, Self::MAX_FEE_GWEI_DEFAULT)
    }

    pub fn with_max(provider: Arc<Provider<Http>>, min_fee_gwei: f64, max_fee_gwei: f64) -> Self {
        Self {
            config: GasConfig::new()
                .with_max_fee(max_fee_gwei)
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

        let config_max: U256 = parse_units(self.config.max_gwei(), "gwei")?;
        let config_prio: U256 = parse_units(self.config.priority_gwei(), "gwei")?;
        let min_fee_floor: U256 = parse_units(self.min_fee_gwei, "gwei")?;

        let Some(base_fee) = block.base_fee_per_gas else {
            let fee = gas_price.max(min_fee_floor).min(config_max);
            let prio = config_prio.min(fee);
            return Ok((fee, prio));
        };

        let base_fee_floor = base_fee.saturating_mul(U256::from(11u64)) / U256::from(10u64)
            + config_prio.saturating_mul(U256::from(11u64)) / U256::from(10u64);
        let mut est_max = base_fee_floor.max(min_fee_floor);
        let mut est_prio = config_prio;

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
        assert_eq!(GasManager::MAX_FEE_GWEI_DEFAULT, 1.2);
        assert_eq!(GasManager::PRIORITY_FEE_GWEI_DEFAULT, 0.0001);
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
    fn test_parse_units_invalid_unit_errors() {
        let result = parse_units(1.0, "invalid_unit");
        assert!(result.is_err(), "Invalid unit string should return error");
    }

    #[test]
    fn test_parse_units_empty_unit_errors() {
        let result = parse_units(1.0, "");
        assert!(result.is_err(), "Empty unit string should return error");
    }

    #[test]
    fn test_parse_units_wei_unit() {
        let result = parse_units(1.0, "wei").unwrap();
        assert_eq!(result, U256::from(1));
    }

    #[test]
    fn test_parse_units_kwei() {
        let result = parse_units(1.0, "kwei").unwrap();
        assert_eq!(result, U256::from(1_000u64));
    }

    #[test]
    fn test_parse_units_mwei() {
        let result = parse_units(1.0, "mwei").unwrap();
        assert_eq!(result, U256::from(1_000_000u64));
    }

    #[test]
    fn test_get_max_fee_below_config() {
        let provider = Arc::new(Provider::<Http>::try_from("http://localhost:9999").unwrap());
        let mgr = GasManager::new(provider, 0.0);
        // Default: max_fee=1.2 gwei, priority=0.0001 gwei
        // base_fee=1 gwei → max=1+0.0001=1.0001 gwei → capped at 1.2 → 1.0001
        let base_fee = U256::from(1_000_000_000u128);
        let result = mgr.get_max_fee(base_fee);
        assert_eq!(result, U256::from(1_000_100_000u128));
    }

    #[test]
    fn test_get_max_fee_above_config() {
        let provider = Arc::new(Provider::<Http>::try_from("http://localhost:9999").unwrap());
        let mgr = GasManager::new(provider, 0.0);
        // base_fee=5 gwei → capped at 1.2 gwei
        let base_fee = U256::from(5_000_000_000u128);
        let result = mgr.get_max_fee(base_fee);
        assert_eq!(result, U256::from(1_200_000_000u128));
    }

    #[test]
    fn test_get_max_fee_zero_base() {
        let provider = Arc::new(Provider::<Http>::try_from("http://localhost:9999").unwrap());
        let mgr = GasManager::new(provider, 0.0);
        let base_fee = U256::zero();
        let result = mgr.get_max_fee(base_fee);
        assert_eq!(result, U256::from(100_000u128)); // 0.0001 gwei priority
    }

    #[test]
    fn test_get_max_fee_at_config_cap() {
        let provider = Arc::new(Provider::<Http>::try_from("http://localhost:9999").unwrap());
        let mgr = GasManager::new(provider, 0.0);
        // base_fee=1.1999 gwei → max=1.1999+0.0001=1.2 → exactly at cap
        let base_fee = U256::from(1_199_900_000u128);
        let result = mgr.get_max_fee(base_fee);
        assert_eq!(result, U256::from(1_200_000_000u128), "Exactly at cap");
    }

    #[test]
    fn test_limit_deploy_returns_config() {
        let provider = Arc::new(Provider::<Http>::try_from("http://localhost:9999").unwrap());
        let mgr = GasManager::new(provider, 0.0);
        assert_eq!(mgr.limit_deploy(), U256::from(1_200_000u64));
        assert_eq!(mgr.limit_transfer(), U256::from(21_000u64));
        assert_eq!(mgr.limit_counter_interact(), U256::from(50_000u64));
        assert_eq!(mgr.limit_send_meme(), U256::from(100_000u64));
    }

    #[test]
    fn test_get_max_fee_with_custom_config() {
        let provider = Arc::new(Provider::<Http>::try_from("http://localhost:9999").unwrap());
        let mut mgr = GasManager::new(provider, 0.0);
        let custom = GasConfig::new().with_max_fee(10.0).with_priority_fee(0.5);
        mgr = mgr.with_config(custom);
        // base_fee=5 gwei → max=5+0.5=5.5 → capped at 10 → 5.5
        let base_fee = U256::from(5_000_000_000u128);
        let result = mgr.get_max_fee(base_fee);
        assert_eq!(result, U256::from(5_500_000_000u128));
    }
}
