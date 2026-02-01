//! Mint Stablecoin Task
//!
//! Mints tokens on created stablecoins where wallet has ISSUER_ROLE.
//!
//! Workflow:
//! 1.// query contented_assets table from database for wallet's stablecoins
//! 2. If no tokens, return error
//! 3. For each token, check hasRole(ISSUER_ROLE, wallet)
//! 4. If no role, attempt grantRole(ISSUER_ROLE, wallet)
//! 5. Wait 3s for role propagation
//! 6. Mint random 100k-1M tokens to wallet
//! 7. Log to task_metrics table

use crate::TempoClient;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy_sol_types::SolCall;
use alloy_sol_types::sol;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use rand::prelude::SliceRandom;
use std::str::FromStr;

sol!(
    interface ITIP20Mintable {
        function mint(address to, uint256 amount);
        function grantRole(bytes32 role, address account);
        function hasRole(bytes32 role, address account) returns (bool);
    }
);

const PATHUSD_ADDRESS: &str = "0x20c0000000000000000000000000000000000000";

#[derive(Debug, Clone, Default)]
pub struct MintStableTask;

impl MintStableTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for MintStableTask {
    fn name(&self) -> &'static str {
        "07_mint_stable"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let wallet_addr_str = address.to_string();

        // println!("Looking for stablecoins for wallet: {}", wallet_addr_str);

        let created_token_addresses = if let Some(db) = &ctx.db {
            match db.get_assets_by_type(&wallet_addr_str, "stablecoin").await {
                Ok(addresses) => addresses,
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        if created_token_addresses.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No created stablecoins found in DB".to_string(),
                tx_hash: None,
            });
        }

        let mut rng = rand::rngs::OsRng;
        let mut shuffled_addresses = created_token_addresses.clone();
        shuffled_addresses.shuffle(&mut rng);

        let token_addr_str = &shuffled_addresses[0];
        let token_addr = if let Ok(addr) = Address::from_str(token_addr_str) {
            addr
        } else {
            return Ok(TaskResult {
                success: false,
                message: "Invalid token address".to_string(),
                tx_hash: None,
            });
        };

        let token_symbol = token_addr_str.get(..8).unwrap_or("Unknown").to_string();
        let decimals = get_token_decimals(client, token_addr).await?;
        let amount_base = rng.gen_range(100_000..1_000_000);
        let amount_wei = U256::from(amount_base) * U256::from(10_u64.pow(decimals as u32));

        // println!(
        //     "Minting {} {} ({:?})...",
        //     amount_base, token_symbol, address
        // );

        // Check if we already have the role
        let issuer_role: [u8; 32] = alloy_primitives::keccak256(b"ISSUER_ROLE").into();
        let has_role = check_has_role(client, token_addr, issuer_role, address).await?;

        if !has_role {
            // println!("Wallet doesn't have ISSUER_ROLE, granting...");
            let grant_call = ITIP20Mintable::grantRoleCall {
                role: alloy_primitives::B256::from(issuer_role),
                account: address,
            };
            let grant_calldata = grant_call.abi_encode();

            let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
            let grant_tx = TransactionRequest::default()
                .to(token_addr)
                .input(TransactionInput::from(grant_calldata.clone()))
                .from(address)
                .nonce(nonce);

            let mut granted = false;

            // Send grant with retry logic
            let grant_result = match client.provider.send_transaction(grant_tx.clone()).await {
                Ok(p) => Ok(p),
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nonce too low") || err_str.contains("already known") {
                        tracing::warn!(
                            "Nonce error on grantRole (mint_stable), resetting cache and retrying..."
                        );
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
                // println!("ISSUER_ROLE granted, waiting for propagation...");

                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                granted = true;
            } else {
                // println!("Failed to grant ISSUER_ROLE, trying MINTER_ROLE...");
                let minter_role = alloy_primitives::keccak256(b"MINTER_ROLE");
                let grant_call = ITIP20Mintable::grantRoleCall {
                    role: alloy_primitives::B256::from(minter_role),
                    account: address,
                };
                let grant_calldata = grant_call.abi_encode();

                let grant_tx = TransactionRequest::default()
                    .to(token_addr)
                    .input(TransactionInput::from(grant_calldata.clone()))
                    .from(address);

                // Send MINTER_ROLE grant with retry logic
                let minter_grant_result = match client
                    .provider
                    .send_transaction(grant_tx.clone())
                    .await
                {
                    Ok(p) => Ok(p),
                    Err(e) => {
                        let err_str = e.to_string().to_lowercase();
                        if err_str.contains("nonce too low") || err_str.contains("already known") {
                            tracing::warn!(
                                "Nonce error on MINTER_ROLE grant, resetting cache and retrying..."
                            );
                            client.reset_nonce_cache().await;
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            client.provider.send_transaction(grant_tx).await
                        } else {
                            Err(e)
                        }
                    }
                };

                if let Ok(pending) = minter_grant_result {
                    let _ = pending.get_receipt().await;
                    // println!("MINTER_ROLE granted, waiting for propagation...");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    granted = true;
                }
            }

            if !granted {
                return Ok(TaskResult {
                    success: false,
                    message: "Failed to grant role (ISSUER/MINTER)".to_string(),
                    tx_hash: None,
                });
            }
        }

        // Mint using alloy's ABI encoding (same as task 4)
        let mint_call = ITIP20Mintable::mintCall {
            to: address,
            amount: amount_wei,
        };
        let mint_calldata = mint_call.abi_encode();

        let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let mint_tx = TransactionRequest::default()
            .to(token_addr)
            .input(TransactionInput::from(mint_calldata.clone()))
            .from(address)
            .nonce(nonce);

        // Send mint with retry logic
        let mint_result = match client.provider.send_transaction(mint_tx.clone()).await {
            Ok(p) => Ok(p),
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!(
                        "Nonce error on mint (mint_stable), resetting cache and retrying..."
                    );
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client.provider.send_transaction(mint_tx).await
                } else {
                    Err(e)
                }
            }
        };

        match mint_result {
            Ok(pending) => {
                let tx_hash = *pending.tx_hash();
                let receipt = pending
                    .get_receipt()
                    .await
                    .context("Failed to get receipt")?;

                if receipt.inner.status() {
                    return Ok(TaskResult {
                        success: true,
                        message: format!(
                            "Minted {} {} to {:?}",
                            amount_base, token_symbol, address
                        ),
                        tx_hash: Some(format!("{:?}", tx_hash)),
                    });
                } else {
                    return Ok(TaskResult {
                        success: false,
                        message: "Mint reverted".to_string(),
                        tx_hash: Some(format!("{:?}", tx_hash)),
                    });
                }
            }
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("aa4bc69a") {
                    return Ok(TaskResult {
                        success: false,
                        message: "Mint skipped: Likely Sold Out or Already Claimed (0xaa4bc69a)"
                            .to_string(),
                        tx_hash: None,
                    });
                }
                return Err(e).context("Failed to mint stablecoin");
            }
        }

        // This part is now unreachable if we return in all paths above,
        // but if we want to be safe we can leave a fallback or just remove the fallback below.
        // The original code fell through to line 201.
        // We can just return an error or unreachable here.
        /*
        Ok(TaskResult {
            success: false,
            message: format!("Failed to mint {} {}", token_symbol, address),
            tx_hash: None,
        })
        */
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

async fn check_has_role(
    client: &crate::TempoClient,
    token: Address,
    role: [u8; 32],
    account: Address,
) -> Result<bool> {
    let has_role_call = ITIP20Mintable::hasRoleCall {
        role: alloy_primitives::B256::from(role),
        account: account,
    };
    let calldata = has_role_call.abi_encode();

    let query = TransactionRequest::default()
        .to(token)
        .input(calldata.into());

    if let Ok(data) = client.provider.call(query).await {
        if !data.is_empty() {
            // Parse the boolean return value
            return Ok(data.as_ref()[31] != 0);
        }
    }
    Ok(false)
}
