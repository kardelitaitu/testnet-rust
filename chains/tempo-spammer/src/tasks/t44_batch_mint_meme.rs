//! Batch Mint Meme Task
//!
//! Mints meme tokens to random recipients using atomic batch (0x76).
//!
//! Workflow:
//! 1. Query meme token from DB
//! 2. Generate random recipients from address.txt
//! 3. Mint meme tokens using atomic batch

use crate::tasks::prelude::*;
use crate::tasks::tempo_tokens::TempoTokens;
use alloy::primitives::{Address, Bytes, FixedBytes, TxKind, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::signers::Signer;
use alloy::sol_types::{SolCall, sol};
use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::BufMut;
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

const ISSUER_ROLE: [u8; 32] = [
    0x2c, 0xfb, 0x1f, 0xc1, 0x0a, 0x22, 0xd0, 0x6e, 0x48, 0x5a, 0xfd, 0x48, 0xff, 0x86, 0x0e, 0x2e,
    0xbc, 0x30, 0xa5, 0x47, 0x32, 0x71, 0x8a, 0x6e, 0x6e, 0x51, 0xb2, 0x70, 0x56, 0x6a, 0x38, 0xf6,
]; // keccak256("ISSUER_ROLE")

#[derive(Debug, Clone, Default)]
pub struct BatchMintMemeTask;

impl BatchMintMemeTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for BatchMintMemeTask {
    fn name(&self) -> &'static str {
        "44_batch_mint_meme"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        use rand::seq::SliceRandom;

        let client = &ctx.client;
        let address = ctx.address();
        let chain_id = ctx.chain_id();

        let meme_tokens = if let Some(db) = &ctx.db {
            match db.get_assets_by_type(&address.to_string(), "meme").await {
                Ok(addresses) => addresses,
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        if meme_tokens.is_empty() {
            return Ok(TaskResult {
                success: false,
                message: "No created meme tokens found for batch minting.".to_string(),
                tx_hash: None,
            });
        }

        let mut rng = rand::rngs::OsRng;
        let token_addr_str = meme_tokens.choose(&mut rng).unwrap();
        let token_addr = Address::from_str(token_addr_str)?;

        let s_call = IMintable::symbolCall {};
        let s_res = client
            .provider
            .call(
                TransactionRequest::default()
                    .to(token_addr)
                    .input(s_call.abi_encode().into()),
            )
            .await?;
        let symbol =
            IMintable::symbolCall::abi_decode_returns(&s_res).unwrap_or_else(|_| "???".to_string());

        tracing::debug!("Batch minting meme token: {} ({:?})", symbol, token_addr);

        // 1. Check Role
        let role_call = IMintable::hasRoleCall {
            account: address,
            role: FixedBytes::from(ISSUER_ROLE),
        };
        let r_res = client
            .provider
            .call(
                TransactionRequest::default()
                    .to(token_addr)
                    .input(role_call.abi_encode().into()),
            )
            .await?;
        let has_role = IMintable::hasRoleCall::abi_decode_returns(&r_res).unwrap_or(false);

        if !has_role {
            tracing::debug!("  -> Wallet lacks ISSUER_ROLE, granting...");
            let grant_call = IMintable::grantRoleCall {
                role: FixedBytes::from(ISSUER_ROLE),
                account: address,
            };
            let tx = TransactionRequest::default()
                .to(token_addr)
                .input(grant_call.abi_encode().into())
                .from(address)
                .gas_limit(1_000_000);

            let pending = client.provider.send_transaction(tx).await?;
            let receipt = pending.get_receipt().await?;
            if !receipt.status() {
                anyhow::bail!("Failed to grant ISSUER_ROLE: transaction reverted");
            }
            tracing::debug!("  -> Role granted.");
        }

        // 2. Prepare Recipients and Amounts
        let mut os_rng = rand::rngs::OsRng;
        let count = os_rng.gen_range(20..31);
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
                let amount_base = os_rng.gen_range(500..2000); // 500 - 2000 tokens per mint
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

        // 3. Construct 0x76 Transaction with retry logic for nonce races
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        // Get fresh nonce right before signing (after any prior transactions like grantRole)
        let gas_price = client.provider.get_gas_price().await?;
        let max_fee = U256::from(gas_price) * U256::from(120) / U256::from(100);

        // Retry loop for nonce too low errors
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 3;

        let tx_hash = loop {
            // Get fresh nonce each attempt
            let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;

            let tx = TempoTransaction {
                chain_id,
                fee_token: None, // Use native token for stability
                max_priority_fee_per_gas: 1_500_000_000,
                max_fee_per_gas: max_fee.to::<u128>(),
                gas_limit: 3_000_000, // Bumped to 3M to handle 30 mints
                calls: calls.clone(),
                nonce,
                valid_before: Some(now + 3600),
                valid_after: Some(now - 60),
                ..Default::default()
            };

            // Sign and try to broadcast
            let hash = tx.signature_hash();
            let signature = client.signer.sign_hash(&hash).await?;
            let tempo_sig = TempoSignature::from(signature);
            let signed_tx = tx.into_signed(tempo_sig);

            let mut signed_buf = bytes::BytesMut::new();
            signed_tx.eip2718_encode(&mut signed_buf);

            match client.provider.send_raw_transaction(&signed_buf).await {
                Ok(pending) => {
                    break *pending.tx_hash();
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nonce too low") && retry_count < MAX_RETRIES {
                        retry_count += 1;
                        tracing::warn!(
                            "Nonce too low (attempt {}/{}), refreshing nonce and retrying...",
                            retry_count,
                            MAX_RETRIES
                        );
                        // Reset nonce cache to force fresh fetch
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        continue;
                    } else {
                        return Err(e).context("Failed to send batch mint 0x76 tx");
                    }
                }
            }
        };

        tracing::debug!(
            "  -> Batch sent after {} attempt(s): {:?}",
            retry_count + 1,
            tx_hash
        );

        Ok(TaskResult {
            success: true,
            message: format!(
                "Batch minted {} {} to {} recipients ({} attempt(s))",
                count,
                symbol,
                count,
                retry_count + 1
            ),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
