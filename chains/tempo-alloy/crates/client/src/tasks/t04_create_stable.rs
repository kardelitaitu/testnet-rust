//! Create Stablecoin Task
//!
//! Creates a new stablecoin using the Tempo TIP-20 factory.

use crate::tasks::{prelude::*, GasManager};
use alloy::primitives::{Address, U256};
use alloy::sol;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use rand::Rng;
use std::str::FromStr;

/// TIP-20 Factory Interface
sol!(
    ITIP20Factory,
    r#"[
        function createToken(string name, string symbol, string currency, address quoteToken, address admin, bytes32 salt) returns (address)
        event TokenCreated(address indexed token, string name, string symbol, string currency, address quoteToken, address admin, bytes32 salt)
    ]"#
);

/// TIP-20 Mintable Interface
sol!(
    ITIP20Mintable,
    r#"[
        function mint(address to, uint256 amount)
        function grantRole(bytes32 role, address account)
    ]"#
);

/// Create a new stablecoin
#[derive(Debug, Clone, Default)]
pub struct CreateStableTask;

impl CreateStableTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for CreateStableTask {
    fn name(&self) -> &'static str {
        "04_create_stable"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let factory_address = Address::from_str("0x20FC000000000000000000000000000000000000")?;
        let quote_token = Address::from_str("0x20C0000000000000000000000000000000000000")?;

        let client = &ctx.client;
        let gas_manager = GasManager::default();

        let gas_price = gas_manager.estimate_gas(client).await?;
        let bumped_gas_price = gas_manager.bump_fees(gas_price, 20);

        let address = ctx.address();

        // 1. Generate random name and symbol
        let name = generate_random_name();
        let symbol = generate_random_symbol();
        let currency = "USD".to_string();

        let salt = {
            let mut rng = rand::thread_rng();
            let mut salt = [0u8; 32];
            rng.fill(&mut salt);
            salt
        };

        println!("Creating {} ({})...", name, symbol);

        // 2. Create factory contract
        let factory = ITIP20Factory::new(factory_address, client.provider.clone());

        // 3. Create token
        let tx = factory.create_token(
            name.clone(),
            symbol.clone(),
            currency,
            quote_token,
            address,
            salt,
        );
        let pending = tx
            .max_fee_per_gas(bumped_gas_price)
            .send()
            .await
            .context("Failed to send create token transaction")?;

        let receipt = pending
            .get_receipt()
            .await
            .context("Failed to get receipt")?;

        // 4. Parse token address from logs
        let mut token_address = Address::ZERO;
        for log in &receipt.inner.logs {
            if log.address() == factory_address && log.topics().len() > 1 {
                // Parse TokenCreated event
                if let Ok(event) = factory.decode_event::<TokenCreated>(
                    "TokenCreated",
                    log.topics().to_vec(),
                    log.data().clone(),
                ) {
                    token_address = event.token;
                    break;
                }
            }
        }

        if token_address.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Token created but address not found in logs. Tx: {:?}",
                    receipt.inner.transaction_hash
                ),
                tx_hash: Some(format!("{:?}", receipt.inner.transaction_hash)),
            });
        }

        println!("Token deployed at: {:?}", token_address);

        // 5. Grant issuer role and mint initial supply
        let issuer_role = alloy_primitives::keccak256("ISSUER_ROLE".as_bytes());
        let token_contract = ITIP20Mintable::new(token_address, client.provider.clone());

        // Grant role
        token_contract
            .grant_role(issuer_role, address)
            .max_fee_per_gas(bumped_gas_price)
            .send()
            .await?
            .get_receipt()
            .await?;

        // Mint
        let mint_amount = U256::from(100_000u64) * U256::exp10(18);
        let mint_receipt = token_contract
            .mint(address, mint_amount)
            .max_fee_per_gas(bumped_gas_price)
            .send()
            .await?
            .get_receipt()
            .await?;

        // 6. Log to database
        if let Some(db) = &ctx.db {
            db.log_asset_creation(
                &address.to_string(),
                &token_address.to_string(),
                "stablecoin",
                &name,
                &symbol,
            )
            .await?;
        }

        Ok(TaskResult {
            success: true,
            message: format!("Created {} ({}) at {:?}", name, symbol, token_address),
            tx_hash: Some(format!("{:?}", mint_receipt.inner.transaction_hash)),
        })
    }
}

fn generate_random_name() -> String {
    let prefixes = [
        "Alpha", "Beta", "Gamma", "Delta", "Omega", "Nova", "Stellar", "Crypto", "Digital", "Meta",
    ];
    let suffixes = [
        "Dollar", "Coin", "Cash", "Pay", "Money", "Finance", "Capital", "Fund",
    ];
    let mut rng = rand::thread_rng();
    format!(
        "{} {}",
        prefixes[rng.gen_range(0..prefixes.len())],
        suffixes[rng.gen_range(0..suffixes.len())]
    )
}

fn generate_random_symbol() -> String {
    let letters = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut rng = rand::thread_rng();
    let mut s = String::new();
    for _ in 0..3 {
        s.push(letters[rng.gen_range(0..letters.len())] as char);
    }
    s + "USD"
}
