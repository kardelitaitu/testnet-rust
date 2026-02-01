//! Batch Mint Stable Task
//!
//! Mints stablecoins to 3-7 random recipients using atomic batch (0x76).
//!
//! Workflow:
//! 1. Generate random recipients from address.txt
//! 2. Mint stable tokens using atomic batch

use crate::tasks::prelude::*;
use crate::tasks::tempo_tokens::TempoTokens;
use alloy::primitives::{Address, Bytes, FixedBytes, TxKind, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::signers::Signer;
use alloy::sol_types::{SolCall, sol};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use std::str::FromStr;
use tempo_primitives::transaction::{Call, TempoSignature, TempoTransaction};

sol!(
    interface IMintable {
        function mint(address to, uint256 amount) external;
        function decimals() view returns (uint8);
        function symbol() view returns (string);
        function hasRole(address account, bytes32 role) view returns (bool);
        function grantRole(bytes32 role, address account) external;
    }
);

const PATHUSD_ADDRESS: &str = "0x20c0000000000000000000000000000000000000";
const ISSUER_ROLE: [u8; 32] = [
    0x2c, 0xfb, 0x1f, 0xc1, 0x0a, 0x22, 0xd0, 0x6e, 0x48, 0x5a, 0xfd, 0x48, 0xff, 0x86, 0x0e, 0x2e,
    0xbc, 0x30, 0xa5, 0x47, 0x32, 0x71, 0x8a, 0x6e, 0x6e, 0x51, 0xb2, 0x70, 0x56, 0x6a, 0x38, 0xf6,
]; // keccak256("ISSUER_ROLE")

const MINTER_ROLE: [u8; 32] = [
    0x9f, 0x2d, 0xf0, 0xfe, 0xd2, 0xc7, 0x76, 0x48, 0xde, 0x58, 0x60, 0xa4, 0xcc, 0x50, 0x8c, 0xd0,
    0x81, 0x8c, 0x85, 0xb8, 0xb8, 0xa1, 0xab, 0x4c, 0xee, 0xef, 0x8d, 0x98, 0x1c, 0x89, 0x56, 0xa6,
]; // keccak256("MINTER_ROLE")

#[derive(Debug, Clone, Default)]
pub struct BatchMintStableTask;

impl BatchMintStableTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for BatchMintStableTask {
    fn name(&self) -> &'static str {
        "43_batch_mint_stable"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        use rand::seq::SliceRandom;

        let client = &ctx.client;
        let address = ctx.address();
        let chain_id = ctx.chain_id();

        let mut token_addr = Address::from_str(PATHUSD_ADDRESS)?;
        let mut using_created_token = false;

        if let Some(db) = &ctx.db {
            if let Ok(assets) = db
                .get_assets_by_type(&address.to_string(), "stablecoin")
                .await
            {
                if !assets.is_empty() {
                    let mut rng = rand::rngs::OsRng;
                    if let Some(random_asset) = assets.choose(&mut rng) {
                        if let Ok(addr) = Address::from_str(random_asset) {
                            token_addr = addr;
                            using_created_token = true;
                        }
                    }
                }
            }
        }

        let symbol = if using_created_token {
            let call = IMintable::symbolCall {};
            let res = client
                .provider
                .call(
                    TransactionRequest::default()
                        .to(token_addr)
                        .input(call.abi_encode().into()),
                )
                .await?;
            IMintable::symbolCall::abi_decode_returns(&res).unwrap_or_else(|_| "???".to_string())
        } else {
            "PathUSD".to_string()
        };

        tracing::debug!("Batch minting stablecoin: {} ({:?})", symbol, token_addr);

        // 1. Check Role (Try ISSUER first)
        let role_call = IMintable::hasRoleCall {
            account: address,
            role: FixedBytes::from(ISSUER_ROLE),
        };
        let role_res = client
            .provider
            .call(
                TransactionRequest::default()
                    .to(token_addr)
                    .input(role_call.abi_encode().into()),
            )
            .await?;
        let has_role = IMintable::hasRoleCall::abi_decode_returns(&role_res).unwrap_or(false);

        if !has_role {
            tracing::debug!("  -> Wallet lacks ISSUER_ROLE, attempting grant...");

            let mut grant_success = false;

            // Try Granting ISSUER_ROLE
            let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
            let grant_issuer = IMintable::grantRoleCall {
                role: FixedBytes::from(ISSUER_ROLE),
                account: address,
            };
            let tx_issuer = TransactionRequest::default()
                .to(token_addr)
                .input(grant_issuer.abi_encode().into())
                .from(address)
                .nonce(nonce)
                .gas_limit(1_000_000);

            if let Ok(pending) = client.provider.send_transaction(tx_issuer).await {
                if let Ok(receipt) = pending.get_receipt().await {
                    if receipt.status() {
                        tracing::debug!("  -> ISSUER_ROLE granted.");
                        grant_success = true;
                    }
                }
            }

            // Fallback to MINTER_ROLE if ISSUER failed
            if !grant_success {
                tracing::debug!("  -> ISSUER_ROLE grant failed, trying MINTER_ROLE...");
                let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
                let grant_minter = IMintable::grantRoleCall {
                    role: FixedBytes::from(MINTER_ROLE),
                    account: address,
                };
                // Check if we already have MINTER_ROLE before granting
                let role_call_minter = IMintable::hasRoleCall {
                    account: address,
                    role: FixedBytes::from(MINTER_ROLE),
                };
                let role_res_minter = client
                    .provider
                    .call(
                        TransactionRequest::default()
                            .to(token_addr)
                            .input(role_call_minter.abi_encode().into()),
                    )
                    .await?;
                let has_minter =
                    IMintable::hasRoleCall::abi_decode_returns(&role_res_minter).unwrap_or(false);

                if has_minter {
                    grant_success = true;
                } else {
                    let tx_minter = TransactionRequest::default()
                        .to(token_addr)
                        .input(grant_minter.abi_encode().into())
                        .from(address)
                        .nonce(nonce)
                        .gas_limit(1_000_000);

                    if let Ok(pending) = client.provider.send_transaction(tx_minter).await {
                        if let Ok(receipt) = pending.get_receipt().await {
                            if receipt.status() {
                                tracing::debug!("  -> MINTER_ROLE granted.");
                                grant_success = true;
                            }
                        }
                    }
                }
            }

            if !grant_success {
                // Don't bail, return failed cleanly
                return Ok(TaskResult {
                    success: false,
                    message: "Failed to grant ISSUER or MINTER role.".to_string(),
                    tx_hash: None,
                });
            }

            // Wait for propagation
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }

        // 2. Prepare Recipients and Amounts
        let mut rng = rand::rngs::OsRng;
        let count = rng.gen_range(20..31);
        let recipients = get_n_random_addresses(count)?;

        let d_call = IMintable::decimalsCall {};
        let d_res = client
            .provider
            .call(
                TransactionRequest::default()
                    .to(token_addr)
                    .input(d_call.abi_encode().into()),
            )
            .await?;
        let decimals = IMintable::decimalsCall::abi_decode_returns(&d_res).unwrap_or(18);

        let calls: Vec<Call> = recipients
            .iter()
            .map(|&to| {
                let amount_base = rng.gen_range(1000..5000); // 1K - 5K tokens per mint
                let amount = U256::from(amount_base) * U256::from(10_u64.pow(decimals as u32));

                let mint_call = IMintable::mintCall { to, amount };
                Call {
                    to: TxKind::Call(token_addr),
                    value: U256::ZERO,
                    input: Bytes::from(mint_call.abi_encode()),
                }
            })
            .collect();

        tracing::debug!(
            "  -> Constructing atomic batch (0x76) for {} recipients...",
            calls.len()
        );

        // 3. Construct 0x76 Transaction
        let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let gas_price = client.provider.get_gas_price().await?;
        let max_fee = U256::from(gas_price) * U256::from(120) / U256::from(100);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let tx = TempoTransaction {
            chain_id,
            fee_token: None, // Use native token for stability
            max_priority_fee_per_gas: 1_500_000_000,
            max_fee_per_gas: max_fee.to::<u128>(),
            gas_limit: 3_000_000, // Bumped to 3M to handle 30 mints
            calls: calls,
            nonce,
            valid_before: Some(now + 3600),
            valid_after: Some(now - 60),
            ..Default::default()
        };

        // 4. Sign and Broadcast
        let hash = tx.signature_hash();
        let signature = client.signer.sign_hash(&hash).await?;
        let tempo_sig = TempoSignature::from(signature);
        let signed_tx = tx.into_signed(tempo_sig);

        let mut signed_buf = Vec::new();
        signed_tx.eip2718_encode(&mut signed_buf);

        let pending = client
            .provider
            .send_raw_transaction(&signed_buf)
            .await
            .context("Failed to send batch mint 0x76 tx")?;

        let tx_hash = *pending.tx_hash();
        tracing::debug!("  -> Batch sent: {:?}", tx_hash);

        Ok(TaskResult {
            success: true,
            message: format!("Batch minted {} {} to {} recipients", count, symbol, count),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
