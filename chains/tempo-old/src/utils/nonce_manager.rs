use anyhow::Result;
use ethers::prelude::*;
// use std::str::FromStr;

pub struct NonceManager;

abigen!(
    NonceContract,
    r#"[
        function nonce(address owner, uint256 key) external view returns (uint64)
    ]"#
);

impl NonceManager {
    // Address of the Nonce Precompile on Tempo
    pub const NONCE_PRECOMPILE: &'static str = "0x4E4F4E4345000000000000000000000000000000";

    /// Get the protocol nonce (key 0) from the account state
    pub async fn get_protocol_nonce<P: Middleware + 'static>(
        provider: &P,
        address: Address,
    ) -> Result<U256> {
        let nonce = provider.get_transaction_count(address, None).await?;
        Ok(nonce)
    }

    /// Get a user nonce (key > 0) from the Nonce precompile
    pub async fn get_user_nonce<P: Middleware + 'static>(
        provider: std::sync::Arc<P>,
        address: Address,
        key: U256,
    ) -> Result<u64> {
        let contract_addr: Address = Self::NONCE_PRECOMPILE.parse()?;
        let contract = NonceContract::new(contract_addr, provider);

        let nonce = contract.nonce(address, key).call().await?;
        Ok(nonce)
    }
}
