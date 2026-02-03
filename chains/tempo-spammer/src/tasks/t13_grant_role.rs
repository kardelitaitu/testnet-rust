//! Grant Role Task
//!
//! Grants a role (ISSUER_ROLE or PAUSE_ROLE) on a created stablecoin.
//!
//! Workflow:
//! 1. Query created stablecoins from DB
//! 2. Select random stablecoin and role
//! 3. Check if role already granted
//! 4. Grant role if not already held

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
    interface IAccessControl {
        function grantRole(bytes32 role, address account);
        function hasRole(bytes32 role, address account) view returns (bool);
    }
);

#[derive(Debug, Clone, Default)]
pub struct GrantRoleTask;

impl GrantRoleTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for GrantRoleTask {
    fn name(&self) -> &'static str {
        "13_grant_role"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let wallet_addr_str = address.to_string();
        // println!(
        //     "DEBUG: Querying DB for stablecoins for wallet: {}",
        //     wallet_addr_str
        // );

        let created_tokens = if let Some(db) = &ctx.db {
            // println!("DEBUG: DB is initialized");
            match db.get_assets_by_type(&wallet_addr_str, "stablecoin").await {
                Ok(mut addresses) => {
                    // println!("DEBUG: Found {} stablecoins", addresses.len());
                    if addresses.len() > 3 {
                        addresses.truncate(3);
                        // println!("DEBUG: Optimization - Limited to first 3 results");
                    }
                    addresses
                }
                Err(_e) => {
                    // println!("DEBUG: DB Error: {}", _e);
                    Vec::new()
                }
            }
        } else {
            // println!("DEBUG: DB is NOT initialized");
            Vec::new()
        };

        if created_tokens.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No created stablecoins found in DB to grant roles.".to_string(),
                tx_hash: None,
            });
        }

        let mut rng = rand::rngs::OsRng;
        let token_addr_str = created_tokens[rng.gen_range(0..created_tokens.len())].clone();
        let token_addr = if let Ok(addr) = Address::from_str(&token_addr_str) {
            addr
        } else {
            return Ok(TaskResult {
                success: false,
                message: format!("Invalid token address: {}", token_addr_str),
                tx_hash: None,
            });
        };

        let is_issuer = rng.gen_bool(0.8);
        let role_name = if is_issuer {
            "ISSUER_ROLE"
        } else {
            "PAUSE_ROLE"
        };

        let role_hash = keccak256(role_name.as_bytes());

        // Check if role is already granted using sol! macro
        let has_role_call = IAccessControl::hasRoleCall {
            role: alloy_primitives::B256::from(role_hash),
            account: address,
        };

        let has_role_tx = TransactionRequest::default()
            .to(token_addr)
            .input(TransactionInput::from(has_role_call.abi_encode())); // Fix: encode correctly

        let mut has_role = false;
        if let Ok(data) = client.provider.call(has_role_tx).await {
            if let Ok(decoded) = IAccessControl::hasRoleCall::abi_decode_returns(&data) {
                has_role = decoded;
            }
        }

        if has_role {
            return Ok(TaskResult {
                success: true,
                message: format!(
                    "Role {} already granted on {}.",
                    role_name,
                    &token_addr_str[..10]
                ),
                tx_hash: None,
            });
        }

        // println!(
        //     "Granting {} to {:?} on token {}...",
        //     role_name,
        //     address,
        //     &token_addr_str[..10]
        // );

        let grant_call = IAccessControl::grantRoleCall {
            role: alloy_primitives::B256::from(role_hash),
            account: address,
        };

        // Retry logic for nonce races with explicit nonce management
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 3;

        let pending = loop {
            // Get fresh nonce BEFORE building transaction
            let nonce = match client.get_pending_nonce(&ctx.config.rpc_url).await {
                Ok(n) => n,
                Err(e) => {
                    retry_count += 1;
                    tracing::error!(
                        "Failed to get nonce for grant role (attempt {}/{}): {}",
                        retry_count,
                        MAX_RETRIES,
                        e
                    );
                    if retry_count >= MAX_RETRIES {
                        return Ok(TaskResult {
                            success: false,
                            message: format!(
                                "Failed to get nonce after {} retries: {}",
                                MAX_RETRIES, e
                            ),
                            tx_hash: None,
                        });
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    continue;
                }
            };

            let tx = TransactionRequest::default()
                .to(token_addr)
                .input(TransactionInput::from(grant_call.clone().abi_encode()))
                .from(address)
                .nonce(nonce) // EXPLICIT NONCE - prevents race conditions
                .max_fee_per_gas(150_000_000_000u128)
                .max_priority_fee_per_gas(1_500_000_000u128);

            match client.provider.send_transaction(tx).await {
                Ok(p) => break p,
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();

                    // Check for nonce errors
                    if (err_str.contains("nonce too low") || err_str.contains("already known"))
                        && retry_count < MAX_RETRIES
                    {
                        retry_count += 1;
                        tracing::warn!(
                            "Nonce error on grant role attempt {}/{}, resetting cache and retrying...",
                            retry_count,
                            MAX_RETRIES
                        );
                        // Reset nonce cache and retry with fresh nonce
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        continue;
                    }

                    // Other errors - return failure
                    return Ok(TaskResult {
                        success: false,
                        message: format!("Grant role failed: {}", e),
                        tx_hash: None,
                    });
                }
            }
        };

        let tx_hash = *pending.tx_hash();
        match pending.get_receipt().await {
            Ok(receipt) => {
                if receipt.inner.status() {
                    Ok(TaskResult {
                        success: true,
                        message: format!(
                            "Granted {} on {} ({} attempt(s)). Tx: {}",
                            role_name,
                            &token_addr_str[..10],
                            retry_count + 1,
                            tx_hash
                        ),
                        tx_hash: Some(format!("{:?}", tx_hash)),
                    })
                } else {
                    // Reverted
                    Ok(TaskResult {
                        success: false,
                        message: "grantRole reverted".to_string(),
                        tx_hash: Some(format!("{:?}", tx_hash)),
                    })
                }
            }
            Err(e) => Ok(TaskResult {
                success: false,
                message: format!("Failed to get receipt: {}", e),
                tx_hash: Some(format!("{:?}", tx_hash)),
            }),
        }
    }
}
