//! Mint Meme Token Task
//!
//! Mints additional supply on created meme tokens.
//!
//! Workflow:
//! 1. Query meme tokens from DB
//! 2. Select random token
//! 3. Check/grant ISSUER_ROLE
//! 4. Mint tokens

use crate::TempoClient;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, U256, keccak256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy_sol_types::{SolCall, sol};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use std::str::FromStr;

sol!(
    interface ITIP20Mintable {
        function mint(address to, uint256 amount);
        function grantRole(bytes32 role, address account);
        function hasRole(bytes32 role, address account) view returns (bool);
    }
);

#[derive(Debug, Clone, Default)]
pub struct MintMemeTask;

impl MintMemeTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for MintMemeTask {
    fn name(&self) -> &'static str {
        "22_mint_meme"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let wallet_addr_str = address.to_string();

        let mut meme_tokens = if let Some(db) = &ctx.db {
            match db.get_assets_by_type(&wallet_addr_str, "meme").await {
                Ok(addresses) => addresses,
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        if meme_tokens.is_empty() {
            return Ok(TaskResult {
                success: true, // Mark as success/skipped to avoid alarming errors in sequence
                message: "Skipped: No created meme tokens found".to_string(),
                tx_hash: None,
            });
        }

        let mut rng = rand::rngs::OsRng;
        let token_addr_str = meme_tokens[rng.gen_range(0..meme_tokens.len())].clone();
        let token_addr = if let Ok(addr) = Address::from_str(&token_addr_str) {
            addr
        } else {
            return Ok(TaskResult {
                success: false,
                message: "Invalid token address".to_string(),
                tx_hash: None,
            });
        };

        let symbol = token_addr_str.get(..8).unwrap_or("MEME").to_string();
        let decimals = get_token_decimals(client, token_addr).await?;
        let amount_base = rng.gen_range(1000..5000);
        let amount_wei = U256::from(amount_base) * U256::from(10_u64.pow(decimals as u32));

        let issuer_role = keccak256(b"ISSUER_ROLE");

        // Check role using sol! call
        let has_role_call = ITIP20Mintable::hasRoleCall {
            role: alloy_primitives::B256::from(issuer_role),
            account: address,
        };
        let has_role_tx = TransactionRequest::default()
            .to(token_addr)
            .input(TransactionInput::from(has_role_call.abi_encode()));

        let mut has_role = false;
        if let Ok(data) = client.provider.call(has_role_tx).await {
            if let Ok(decoded) = ITIP20Mintable::hasRoleCall::abi_decode_returns(&data) {
                has_role = decoded;
            }
        }

        if !has_role {
            // println!("Granting ISSUER_ROLE for {}...", symbol);

            let grant_call = ITIP20Mintable::grantRoleCall {
                role: alloy_primitives::B256::from(issuer_role),
                account: address,
            };

            let grant_tx = TransactionRequest::default()
                .to(token_addr)
                .input(TransactionInput::from(grant_call.abi_encode()))
                .from(address);

            match client.provider.send_transaction(grant_tx).await {
                Ok(pending) => {
                    let _ = pending.get_receipt().await;
                    // println!("ISSUER_ROLE granted");
                }
                Err(e) => {
                    // println!(
                    //     "Warning: Failed to grant role (might already have it): {}",
                    //     e
                    // );
                }
            }
        }

        // println!("Minting {} {}...", amount_base, symbol);

        let mint_call = ITIP20Mintable::mintCall {
            to: address,
            amount: amount_wei,
        };

        // Retry logic for nonce races
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 3;

        let pending = loop {
            let mint_tx = TransactionRequest::default()
                .to(token_addr)
                .input(TransactionInput::from(mint_call.clone().abi_encode()))
                .from(address);

            match client.provider.send_transaction(mint_tx).await {
                Ok(p) => break p,
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();

                    // Check for nonce too low error
                    if err_str.contains("nonce too low") && retry_count < MAX_RETRIES {
                        retry_count += 1;
                        tracing::warn!(
                            "Nonce too low on mint attempt {}/{}, refreshing and retrying...",
                            retry_count,
                            MAX_RETRIES
                        );
                        // Reset nonce cache and retry
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        continue;
                    }

                    if err_str.contains("execution reverted") || err_str.contains("aa4bc69a") {
                        if err_str.contains("aa4bc69a") {
                            return Ok(TaskResult {
                                success: false,
                                message: "Mint reverted: Likely Sold Out or Already Claimed"
                                    .to_string(),
                                tx_hash: None,
                            });
                        }
                    }
                    return Err(e).context("Failed to mint");
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
                message: "Mint reverted".to_string(),
                tx_hash: Some(format!("{:?}", tx_hash)),
            });
        }

        // Re-construct success message since we are inside control flow
        // Actually, the structure of the original code had 'let pending = ...' then 'let tx_hash = ...'.
        // I need to adapt to not break variable scope.
        // Let's rewrite the block to be safer.

        // println!("✅ Minted {} {} at {:?}", amount_base, symbol, tx_hash);

        Ok(TaskResult {
            success: true,
            message: format!(
                "Minted {} {} ({} attempt(s)). Tx: {}",
                amount_base,
                symbol,
                retry_count + 1,
                tx_hash
            ),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}

async fn get_token_decimals(client: &crate::TempoClient, token: Address) -> Result<u8> {
    let mut calldata = Vec::new();
    calldata.extend_from_slice(&[0x31, 0x3f, 0x13, 0xa0]);
    let query = TransactionRequest::default()
        .to(token)
        .input(calldata.into());
    if let Ok(data) = client.provider.call(query).await {
        let bytes = data.as_ref();
        if !bytes.is_empty() {
            return Ok(bytes[bytes.len() - 1]);
        }
    }
    Ok(6)
}
