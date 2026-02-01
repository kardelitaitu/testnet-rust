use anyhow::Result;
use ethers::prelude::*;

pub struct GasManager;

impl GasManager {
    pub async fn estimate_gas<P: Middleware + 'static>(provider: &P) -> Result<U256> {
        // Tempo uses a fixed base fee model in some contexts, but for now we can rely on provider
        let gas_price = provider
            .get_gas_price()
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(gas_price)
    }

    pub fn bump_fees(current_gas_price: U256) -> U256 {
        current_gas_price * U256::from(500) / U256::from(100)
    }
}
