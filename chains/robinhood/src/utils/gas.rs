use anyhow::Result;
use core_logic::GasConfig;
use ethers::prelude::*;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct GasManager {
    config: GasConfig,
    provider: Arc<Provider<Http>>,
}

impl GasManager {
    pub const MAX_FEE_GWEI_DEFAULT: f64 = 100.0;
    pub const PRIORITY_FEE_GWEI_DEFAULT: f64 = 1.0;
    pub const LIMIT_DEPLOY: U256 = U256([1_200_000, 0, 0, 0]);
    pub const LIMIT_TRANSFER: U256 = U256([21_000, 0, 0, 0]);
    pub const LIMIT_COUNTER_INTERACT: U256 = U256([50_000, 0, 0, 0]);
    pub const LIMIT_SEND_MEME: U256 = U256([100_000, 0, 0, 0]);

    pub fn new(provider: Arc<Provider<Http>>) -> Self {
        Self {
            config: GasConfig::new()
                .with_max_fee(100.0) 
                .with_priority_fee(1.0),
            provider,
        }
    }

    pub fn with_config(mut self, config: GasConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn get_fees(&self) -> Result<(U256, U256)> {
        let block = self
            .provider
            .get_block(BlockNumber::Latest)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to get latest block"))?;

        let base_fee = block
            .base_fee_per_gas
            .unwrap_or_else(|| U256::from(1000000000u64)); // Default 1 Gwei

        let (mut est_max, mut est_prio) = match self.provider.estimate_eip1559_fees(None).await {
            Ok(fees) => fees,
            Err(_) => {
                let prio = parse_units(self.config.priority_gwei(), "gwei")?.into();
                (base_fee + prio, prio)
            }
        };

        let config_max: U256 = parse_units(self.config.max_gwei(), "gwei")?.into();

        if est_max > config_max {
            est_max = config_max;
        }

        if est_prio > est_max {
            est_prio = est_max;
        }

        Ok((est_max, est_prio))
    }

    pub fn limit_deploy(&self) -> U256 {
        U256([self.config.limit_deploy(), 0, 0, 0])
    }

    pub fn limit_transfer(&self) -> U256 {
        U256([self.config.limit_transfer(), 0, 0, 0])
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
        assert_eq!(GasManager::MAX_FEE_GWEI_DEFAULT, 100.0);
        assert_eq!(GasManager::PRIORITY_FEE_GWEI_DEFAULT, 1.0);
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
}
