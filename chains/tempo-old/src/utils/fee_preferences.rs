use anyhow::Result;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use rand::seq::SliceRandom;
use std::sync::Arc;

// Define the FeeManager contract interface
abigen!(
    FeeManager,
    r#"[
        function setUserToken(address token) external
    ]"#
);

pub struct FeePreferences;

impl FeePreferences {
    pub const PATH_USD_ADDRESS: &'static str = "0x20C0000000000000000000000000000000000000";
    pub const ALPHA_USD_ADDRESS: &'static str = "0x20C0000000000000000000000000000000000001";
    pub const BETA_USD_ADDRESS: &'static str = "0x20C0000000000000000000000000000000000002";
    pub const THETA_USD_ADDRESS: &'static str = "0x20C0000000000000000000000000000000000003";

    pub const FEE_MANAGER_ADDRESS: &'static str = "0xfeec000000000000000000000000000000000000";

    pub async fn set_random_fee_token(
        provider: Arc<Provider<Http>>,
        wallet: LocalWallet,
    ) -> Result<String> {
        let tokens = vec![
            (Self::PATH_USD_ADDRESS, "PATH_USD"),
            (Self::ALPHA_USD_ADDRESS, "ALPHA_USD"),
            (Self::BETA_USD_ADDRESS, "BETA_USD"),
            (Self::THETA_USD_ADDRESS, "THETA_USD"),
        ];

        let (chosen_address, chosen_symbol) = {
            let mut rng = rand::thread_rng();
            tokens.choose(&mut rng).unwrap().clone()
        };

        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);

        let fee_manager_address = Self::FEE_MANAGER_ADDRESS.parse::<Address>()?;
        let contract = FeeManager::new(fee_manager_address, client);

        let token_address = chosen_address.parse::<Address>()?;

        // Call setUserToken
        let tx = contract.set_user_token(token_address);
        let pending_tx = tx.send().await?;
        let _receipt = pending_tx.await?;

        Ok(chosen_symbol.to_string())
    }
}
