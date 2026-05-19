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
    pub const MAX_FEE_GWEI_DEFAULT: f64 = 1.0;
    pub const PRIORITY_FEE_GWEI_DEFAULT: f64 = 0.001;
    pub const LIMIT_DEPLOY: U256 = U256([1_200_000, 0, 0, 0]);
    pub const LIMIT_TRANSFER: U256 = U256([21_000, 0, 0, 0]);
    pub const LIMIT_COUNTER_INTERACT: U256 = U256([50_000, 0, 0, 0]);
    pub const LIMIT_SEND_MEME: U256 = U256([100_000, 0, 0, 0]);

    pub fn new(provider: Arc<Provider<Http>>, min_fee_gwei: f64) -> Self {
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

        let base_fee_floor = base_fee.saturating_mul(U256::from(3u64)) + config_prio.saturating_mul(U256::from(3u64));
        let mut est_max = base_fee_floor.max(gas_price).max(min_fee_floor);
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
