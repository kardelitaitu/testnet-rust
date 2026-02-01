//! Create Meme Token Task
//!
//! Creates a new meme token via the TIP-20 factory.
//!
//! Workflow:
//! 1. Generate random name/symbol
//! 2. Create token via factory
//! 3. Grant ISSUER_ROLE and mint initial supply

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy_sol_types::SolCall;
use alloy_sol_types::sol;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use rand::seq::SliceRandom;
use std::path::Path;
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

const TIP20_FACTORY_ADDRESS: &str = "0x20fc000000000000000000000000000000000000";
const PATHUSD_ADDRESS: &str = "0x20c0000000000000000000000000000000000000";

#[derive(Debug, Clone, Default)]
pub struct CreateMemeTask;

impl CreateMemeTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for CreateMemeTask {
    fn name(&self) -> &'static str {
        "21_create_meme"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        let factory_addr = Address::from_str(TIP20_FACTORY_ADDRESS).context("Invalid factory")?;
        let pathusd_addr = Address::from_str(PATHUSD_ADDRESS).context("Invalid PathUSD")?;

        let mut rng = rand::rngs::OsRng;

        // Try to read mnemonic file
        let mnemonic_path = Path::new("core-logic/src/utils/mnemonic.txt");
        let (name, symbol) = if mnemonic_path.exists() {
            if let Ok(content) = std::fs::read_to_string(mnemonic_path) {
                let words: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
                if let Some(word) = words.choose(&mut rng) {
                    let capitalized = format!("{}{}", word[..1].to_uppercase(), &word[1..]);
                    let symbol_upper = word.to_uppercase();
                    (format!("Meme {}", capitalized), symbol_upper)
                } else {
                    // Fallback if empty file
                    (
                        format!("Meme_{}", rng.gen_range(100..999)),
                        format!("MEME{}", rng.gen_range(10..99)),
                    )
                }
            } else {
                (
                    format!("Meme_{}", rng.gen_range(100..999)),
                    format!("MEME{}", rng.gen_range(10..99)),
                )
            }
        } else {
            (
                format!("Meme_{}", rng.gen_range(100..999)),
                format!("MEME{}", rng.gen_range(10..99)),
            )
        };

        // println!("Creating Meme Token: {} ({})...", name, symbol);

        let decimals = TempoTokens::get_token_decimals(client, pathusd_addr).await?;
        let balance = TempoTokens::get_token_balance(client, pathusd_addr, address).await?;

        if balance < U256::from(100) * U256::from(10_u64.pow(decimals as u32)) {
            return Ok(TaskResult {
                success: false,
                message: "Insufficient PathUSD for meme creation".to_string(),
                tx_hash: None,
            });
        }

        // Generate random salt (32 bytes) using thread_rng
        let mut salt = [0u8; 32];
        rand::thread_rng().fill(&mut salt);

        // Create the function call using alloy's ABI encoding
        let call = ITIP20Factory::createTokenCall {
            name: name.clone(),
            symbol: symbol.clone(),
            currency: "USD".to_string(),
            quoteToken: pathusd_addr,
            admin: address,
            salt: alloy_primitives::B256::from(salt),
        };
        let create_calldata = call.abi_encode();

        let tx = TransactionRequest::default()
            .to(factory_addr)
            .input(TransactionInput::from(create_calldata.clone()))
            .from(address);

        // Send create with retry logic
        let pending = match client.provider.send_transaction(tx.clone()).await {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on meme create, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client
                        .provider
                        .send_transaction(tx)
                        .await
                        .context("Failed to create token")?
                } else {
                    return Err(e).context("Failed to create token");
                }
            }
        };
        let tx_hash = *pending.tx_hash();
        let receipt = pending
            .get_receipt()
            .await
            .context("Failed to get receipt")?;

        if !receipt.inner.status() {
            return Ok(TaskResult {
                success: false,
                message: "Token creation reverted".to_string(),
                tx_hash: Some(format!("{:?}", tx_hash)),
            });
        }

        // Parse token address from logs
        let mut token_address = Address::ZERO;
        let event_sig = alloy_primitives::keccak256(
            b"TokenCreated(address,string,string,string,address,address,bytes32)",
        );

        for log in receipt.inner.logs() {
            if log.address() == factory_addr {
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

        // println!("✅ Meme Token created at {:?}", token_address);

        // Grant ISSUER_ROLE and mint initial supply
        let issuer_role = alloy_primitives::keccak256(b"ISSUER_ROLE");

        let grant_call = ITIP20Mintable::grantRoleCall {
            role: alloy_primitives::B256::from(issuer_role),
            account: address,
        };
        let grant_calldata = grant_call.abi_encode();

        let grant_tx = TransactionRequest::default()
            .to(token_address)
            .input(TransactionInput::from(grant_calldata.clone()))
            .from(address);

        // println!("Granting ISSUER_ROLE...");

        // Send grant with retry logic
        let grant_result = match client.provider.send_transaction(grant_tx.clone()).await {
            Ok(p) => Ok(p),
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on meme grant, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client.provider.send_transaction(grant_tx).await
                } else {
                    Err(e)
                }
            }
        };

        if let Ok(pending) = grant_result {
            let _ = pending.get_receipt().await;
            // println!("ISSUER_ROLE granted");
        }

        // Small delay to ensure nonce is updated
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Mint initial supply (1M-10M tokens)
        let mint_amount =
            U256::from(rng.gen_range(1..=10) * 1_000_000) * U256::from(10u64).pow(U256::from(6));

        let mint_call = ITIP20Mintable::mintCall {
            to: address,
            amount: mint_amount,
        };
        let mint_calldata = mint_call.abi_encode();

        let mint_tx = TransactionRequest::default()
            .to(token_address)
            .input(TransactionInput::from(mint_calldata.clone()))
            .from(address);

        // Send mint with retry logic
        let mint_result = match client.provider.send_transaction(mint_tx.clone()).await {
            Ok(p) => Ok(p),
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on meme mint, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client.provider.send_transaction(mint_tx).await
                } else {
                    Err(e)
                }
            }
        };

        if let Ok(pending) = mint_result {
            let mint_receipt = pending.get_receipt().await;
            match mint_receipt {
                Ok(_r) => {
                    // println!("Initial mint: {:?}", _r.transaction_hash);
                }
                Err(_) => {
                    // println!("Warning: Failed to confirm initial mint");
                }
            }
        }

        // Log to database
        if let Some(db) = &ctx.db {
            let wallet_str = address.to_string();
            let token_str = token_address.to_string();
            if let Err(e) = db
                .log_asset_creation(&wallet_str, &token_str, "meme", &name, &symbol)
                .await
            {
                // println!("Warning: Failed to log meme token to DB: {}", e);
            } else {
                // println!("Logged meme token to database");
            }
        }

        Ok(TaskResult {
            success: true,
            message: format!("Created Meme {} at {:?}", symbol, token_address),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
