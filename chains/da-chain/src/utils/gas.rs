use anyhow::Result;
use core_logic::GasConfig;
use ethers::prelude::*;
use serde_json::json;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct GasManager {
    config: GasConfig,
    provider: Arc<Provider<Http>>,
    rpc_url: String,
    rpc_client: reqwest::Client,
    min_fee_gwei: f64,
}

impl GasManager {
    pub const MAX_FEE_GWEI_DEFAULT: f64 = 20000.0;
    pub const PRIORITY_FEE_GWEI_DEFAULT: f64 = 10.0;
    pub const LIMIT_DEPLOY: U256 = U256([1_200_000, 0, 0, 0]);
    pub const LIMIT_TRANSFER: U256 = U256([21_000, 0, 0, 0]);
    pub const LIMIT_COUNTER_INTERACT: U256 = U256([50_000, 0, 0, 0]);
    pub const LIMIT_SEND_MEME: U256 = U256([100_000, 0, 0, 0]);

    pub fn new(rpc_url: String, provider: Arc<Provider<Http>>, min_fee_gwei: f64) -> Self {
        Self {
            config: GasConfig::new()
                .with_max_fee(Self::MAX_FEE_GWEI_DEFAULT)
                .with_priority_fee(Self::PRIORITY_FEE_GWEI_DEFAULT),
            provider,
            rpc_url,
            rpc_client: reqwest::Client::new(),
            min_fee_gwei,
        }
    }

    pub fn with_config(mut self, config: GasConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn get_gas_price(&self) -> Result<U256> {
        let response = self
            .rpc_client
            .post(&self.rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1u64,
                "method": "eth_gasPrice",
                "params": [],
            }))
            .send()
            .await;

        let response = match response {
            Ok(resp) => resp,
            Err(_) => return Ok(U256::zero()),
        };

        let payload: serde_json::Value = response.json().await?;
        let gas_price_hex = payload
            .get("result")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("eth_gasPrice returned no result"))?;

        let gas_price = U256::from_str_radix(gas_price_hex.trim_start_matches("0x"), 16)
            .unwrap_or_else(|_| U256::from(0u64));
        Ok(gas_price)
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

        let Some(base_fee) = block.base_fee_per_gas else {
            let fee = gas_price.min(config_max);
            return Ok((fee, fee.min(config_prio)));
        };

        let oracle_fees = self.provider.estimate_eip1559_fees(None).await.ok();
        let (mut est_max, mut est_prio) = oracle_fees.unwrap_or((base_fee + config_prio, config_prio));

        // Keep the fee suggestion grounded in the actual chain state.
        // Some RPCs return stale or overly conservative oracle values, so we
        // use both the RPC gas price and a stronger base-fee-derived floor
        // before clamping to config caps.
        let base_fee_floor = base_fee.saturating_mul(U256::from(2u64)) + config_prio;
        est_max = est_max.max(base_fee_floor).max(gas_price);
        let min_fee_floor: U256 = parse_units(self.min_fee_gwei, "gwei")?.into();
        est_max = est_max.max(min_fee_floor);
        if est_prio < config_prio {
            est_prio = config_prio;
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
