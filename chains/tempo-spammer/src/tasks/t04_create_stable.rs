//! Create Stable Task
//!
//! Deploys a new TIP-20 stablecoin token using the Tempo factory.
//! Factory: 0x20FC000000000000000000000000000000000000
//! Quote Token: 0x20C0000000000000000000000000000000000000 (PathUSD)

use crate::tasks::prelude::*;
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy::sol_types::SolCall;
use alloy_sol_types::sol;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use std::str::FromStr;

sol!(
    interface ITIP20Factory {
        function createToken(
            string name,
            string symbol,
            string currency,
            address quoteToken,
            address admin,
            bytes32 salt
        ) returns (address);
    }
);

sol!(
    interface ITIP20Mintable {
        function mint(address to, uint256 amount);
        function grantRole(bytes32 role, address account);
    }
);

const TIP20_FACTORY_ADDRESS: &str = "0x20FC000000000000000000000000000000000000";
const QUOTE_TOKEN_ADDRESS: &str = "0x20C0000000000000000000000000000000000000";

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
        let client = &ctx.client;
        let address = ctx.address();

        let factory_address =
            Address::from_str(TIP20_FACTORY_ADDRESS).context("Invalid factory address")?;
        let quote_token =
            Address::from_str(QUOTE_TOKEN_ADDRESS).context("Invalid quote token address")?;

        // Generate random name and symbol
        let name = generate_random_name();
        let symbol = generate_random_symbol();
        let currency = "USD".to_string();

        // Generate random salt (32 bytes) using thread_rng
        let mut salt = [0u8; 32];
        rand::thread_rng().fill(&mut salt);

        tracing::debug!("Creating {} ({})...", name, symbol);

        // Create the function call
        let call = ITIP20Factory::createTokenCall {
            name: name.clone(),
            symbol: symbol.clone(),
            currency: currency.clone(),
            quoteToken: quote_token,
            admin: address,
            salt: alloy_primitives::B256::from(salt),
        };

        // Build calldata using ABI encoding
        let calldata = call.abi_encode();

        // Get nonce from manager to prevent race conditions
        let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;

        // Send transaction with retry logic for nonce errors
        let tx = TransactionRequest::default()
            .to(factory_address)
            .input(TransactionInput::from(calldata))
            .from(address)
            .nonce(nonce);

        let pending = match client.provider.send_transaction(tx.clone()).await {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on createToken, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client
                        .provider
                        .send_transaction(tx)
                        .await
                        .context("Failed to send createToken transaction")?
                } else {
                    return Err(e).context("Failed to send createToken transaction");
                }
            }
        };

        let tx_hash = *pending.tx_hash();
        tracing::debug!("CreateToken tx sent: {:?}", tx_hash);

        let receipt = pending
            .get_receipt()
            .await
            .context("Failed to get receipt")?;

        tracing::debug!("CreateToken tx confirmed: {:?}", receipt.transaction_hash);

        // Allow token initialization to propagate (1.5s)
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        // Parse token address from logs
        let mut token_address = Address::ZERO;
        let event_sig = alloy_primitives::keccak256(
            b"TokenCreated(address,string,string,string,address,address,bytes32)",
        );

        for log in receipt.inner.logs() {
            if log.address() == factory_address {
                let topics = log.topics();
                if topics.first() == Some(&event_sig.into()) && topics.len() > 1 {
                    let token_bytes: [u8; 32] = topics[1].as_slice().try_into().unwrap();
                    token_address = Address::from_slice(&token_bytes[12..32]);
                    break;
                }
            }
        }

        if token_address.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Token created but address not found in logs. Tx: {:?}",
                    receipt.transaction_hash
                ),
                tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
            });
        }

        tracing::debug!("Token deployed at: {:?}", token_address);

        // Grant ISSUER_ROLE
        let issuer_role = alloy_primitives::keccak256(b"ISSUER_ROLE");

        let grant_call = ITIP20Mintable::grantRoleCall {
            role: alloy_primitives::B256::from(issuer_role),
            account: address,
        };
        let grant_calldata = grant_call.abi_encode();

        let grant_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let grant_tx = TransactionRequest::default()
            .to(token_address)
            .input(TransactionInput::from(grant_calldata.clone()))
            .from(address)
            .nonce(grant_nonce);

        tracing::debug!("Granting ISSUER_ROLE...");

        // Send grant with retry logic
        let grant_pending = match client.provider.send_transaction(grant_tx.clone()).await {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on grantRole, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client
                        .provider
                        .send_transaction(grant_tx)
                        .await
                        .context("Failed to grant role")?
                } else {
                    return Err(e).context("Failed to grant role");
                }
            }
        };
        grant_pending
            .get_receipt()
            .await
            .context("Failed to get grant receipt")?;

        tracing::debug!("ISSUER_ROLE granted");

        // Small delay to ensure nonce is updated
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Mint tokens (TIP-20 has 6 decimals, mint random amount between 100K and 10M)
        let hundred_k_units = rand::thread_rng().gen_range(1..=100);
        let mint_amount =
            U256::from(hundred_k_units * 100_000) * U256::from(10u64).pow(U256::from(6));
        tracing::debug!("Minting {} tokens...", hundred_k_units * 100_000);

        let mint_call = ITIP20Mintable::mintCall {
            to: address,
            amount: mint_amount,
        };
        let mint_calldata = mint_call.abi_encode();

        let mint_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let mint_tx = TransactionRequest::default()
            .to(token_address)
            .input(TransactionInput::from(mint_calldata.clone()))
            .from(address)
            .nonce(mint_nonce);

        // Send mint with retry logic
        let mint_result = match client.provider.send_transaction(mint_tx.clone()).await {
            Ok(p) => Ok(p),
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on mint, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client
                        .provider
                        .send_transaction(mint_tx)
                        .await
                        .context("Failed to send mint tx")
                } else {
                    Err(e).context("Failed to send mint tx")
                }
            }
        };

        let mint_receipt = match mint_result {
            Ok(pending) => match pending.get_receipt().await {
                Ok(receipt) => receipt,
                Err(e) => {
                    // Mint receipt failed (likely Unauthorized)
                    return Ok(TaskResult {
                        success: false,
                        message: format!(
                            "Token created at {:?} but mint failed: {}",
                            token_address, e
                        ),
                        tx_hash: Some(format!("{:?}", tx_hash)),
                    });
                }
            },
            Err(e) => {
                // Mint send failed
                return Ok(TaskResult {
                    success: false,
                    message: format!(
                        "Token created at {:?} but mint tx failed: {}",
                        token_address, e
                    ),
                    tx_hash: Some(format!("{:?}", tx_hash)),
                });
            }
        };

        // Log to database
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
            message: format!(
                "Created {} ({}) at {:?}. Mint tx: {:?}",
                name, symbol, token_address, mint_receipt.transaction_hash
            ),
            tx_hash: Some(format!("{:?}", mint_receipt.transaction_hash)),
        })
    }
}

fn generate_random_name() -> String {
    let prefixes = [
        "Alpha", "Beta", "Gamma", "Delta", "Omega", "Nova", "Stellar", "Crypto", "Digital", "Meta",
        "Prime", "Ultra", "Hyper", "Mega", "Super", "Quantum", "Titan", "Phoenix", "Dragon",
        "Imperial", "Royal", "Crown", "Noble", "Grand", "Major", "Premier", "Elite", "Classic",
        "Pure", "Solid",
    ];
    let suffixes = [
        "Dollar", "Coin", "Cash", "Pay", "Money", "Finance", "Capital", "Fund", "Bank", "Trust",
        "Vault", "Reserve", "Exchange", "Wallet", "Card", "Token", "Note", "Bills", "Shares",
        "Bonds", "Assets", "Wealth", "Prosper", "Growth", "Equity", "Stocks", "Funds", "Credits",
        "Points",
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
