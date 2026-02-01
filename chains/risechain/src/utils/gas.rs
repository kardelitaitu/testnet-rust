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
    pub const MAX_FEE_GWEI_DEFAULT: f64 = 0.000000009;
    pub const PRIORITY_FEE_GWEI_DEFAULT: f64 = 0.000000001;
    pub const LIMIT_DEPLOY: U256 = U256([1_200_000, 0, 0, 0]);
    pub const LIMIT_TRANSFER: U256 = U256([21_000, 0, 0, 0]);
    pub const LIMIT_COUNTER_INTERACT: U256 = U256([50_000, 0, 0, 0]);
    pub const LIMIT_SEND_MEME: U256 = U256([100_000, 0, 0, 0]);

    pub fn new(provider: Arc<Provider<Http>>) -> Self {
        Self {
            config: GasConfig::new()
                .with_max_fee(0.000000009) // 9 Wei
                .with_priority_fee(0.000000001), // 1 Wei
            provider,
        }
    }

    pub fn with_config(mut self, config: GasConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn get_fees(&self) -> Result<(U256, U256)> {
        // 1. Get Base Fee from latest block for calculation
        let block = self
            .provider
            .get_block(BlockNumber::Latest)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to get latest block"))?;

        let base_fee = block
            .base_fee_per_gas
            .ok_or_else(|| anyhow::anyhow!("Base fee missing in block"))?;

        // 2. Try to estimate fees from oracle (checks history aka "last block")
        let (mut est_max, mut est_prio) = match self.provider.estimate_eip1559_fees(None).await {
            Ok(fees) => fees,
            Err(_) => {
                // Fallback to config if estimation fails
                let prio = parse_units(self.config.priority_gwei(), "gwei")?.into();
                (base_fee + prio, prio)
            }
        };

        // 3. Clamp values to User Config
        let config_max: U256 = parse_units(self.config.max_gwei(), "gwei")?.into();
        let _config_prio: U256 = parse_units(self.config.priority_gwei(), "gwei")?.into();

        // Enforce Max Cap
        if est_max > config_max {
            est_max = config_max;
        }

        // For priority, we generally trust the oracle but ensure it's not insane?
        // Actually, user wants "very low" fees, so we should arguably clamp priority too if it exceeds our config
        // But usually priority < max.
        // Let's ensure priority isn't > max_fee (logic error)
        if est_prio > est_max {
            est_prio = est_max;
        }

        // Ensure we at least pay the configured priority if the oracle is too low?
        // No, user said "smart", implying dynamic. If network is cheap, pay cheap.

        Ok((est_max, est_prio))
    }

    pub async fn get_priority_fee_adjusted(&self, base_fee: U256) -> Result<U256> {
        let block = self.provider.get_block(BlockNumber::Latest).await?;
        let Some(block) = block else {
            return Ok(parse_units(self.config.priority_gwei(), "gwei")?.into());
        };

        let Some(parent_base_fee) = block.base_fee_per_gas else {
            return Ok(parse_units(self.config.priority_gwei(), "gwei")?.into());
        };

        let base_fee_change = if parent_base_fee > U256::zero() {
            (base_fee - parent_base_fee) * 100 / parent_base_fee
        } else {
            U256::zero()
        };

        let priority_fee = if base_fee_change > U256::from(10) {
            parse_units(self.config.priority_gwei() * 2.0, "gwei")?.into()
        } else if base_fee_change > U256::from(5) {
            parse_units(self.config.priority_gwei() * 1.5, "gwei")?.into()
        } else {
            parse_units(self.config.priority_gwei(), "gwei")?.into()
        };

        Ok(priority_fee)
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
