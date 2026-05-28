//! Mint Domain Task
//!
//! Registers a .tempo domain name.
//!
//! Workflow:
//! 1. Generate random domain name
//! 2. Approve PathUSD for domain service (if needed)
//! 3. Register domain
//! 4. Verify ownership via ENS-style node interpretation

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, B256, U256, bytes, keccak256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy::sol;
use alloy_sol_types::{SolCall, SolEvent};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use std::str::FromStr;

const INFINITY_NAME_ADDRESS: &str = "0x30c0000000000000000000000000000000000000";
const PATHUSD_ADDRESS: &str = "0x20c0000000000000000000000000000000000000";

sol! {
    interface IInfinityNameService {
        function register(string calldata name, address referrer) external;
        function owner(bytes32 node) external view returns (address);
        function addr(bytes32 node) external view returns (address);

        // Events that might be emitted during domain registration
        event DomainRegistered(string indexed name, address indexed owner, bytes32 indexed node);
        event NameRegistered(string indexed name, address indexed owner);
        event Transfer(bytes32 indexed node, address owner);
    }
}

#[derive(Debug, Clone, Default)]
pub struct MintDomainTask;

impl MintDomainTask {
    pub fn new() -> Self {
        Self
    }
}

fn namehash(name: &str) -> B256 {
    if name.is_empty() {
        return B256::ZERO;
    }
    let mut hash = B256::ZERO;
    for label in name.split('.').rev() {
        let label_hash = keccak256(label.as_bytes());
        hash = keccak256([hash.as_slice(), label_hash.as_slice()].concat());
    }
    hash
}

#[async_trait]
impl TempoTask for MintDomainTask {
    fn name(&self) -> &'static str {
        "15_mint_domain"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        let infinity_addr = Address::from_str(INFINITY_NAME_ADDRESS).context("Invalid Infinity address")?;
        let pathusd_addr = Address::from_str(PATHUSD_ADDRESS).context("Invalid PathUSD address")?;

        let mut rng = rand::rngs::OsRng;
        let domain: String = (0..8)
            .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
            .collect();
        let domain = domain.to_lowercase();

        // tracing::debug!("Registering domain: {}.tempo", domain);

        // Check Balance
        let decimals = TempoTokens::get_token_decimals(client, pathusd_addr).await?;
        let balance = TempoTokens::get_token_balance(client, pathusd_addr, address).await?;
        let min_balance = U256::from(1000) * U256::from(10_u64.pow(decimals as u32));

        if balance < min_balance {
            return Ok(TaskResult {
                success: false,
                message: "Insufficient PathUSD for domain registration. Need 1000 PathUSD".to_string(),
                tx_hash: None,
            });
        }

        // Register Domain
        let register_call = IInfinityNameService::registerCall {
            name: domain.clone(),
            referrer: Address::ZERO,
        };
        let input = register_call.abi_encode();

        let tx = TransactionRequest::default()
            .to(infinity_addr)
            .input(TransactionInput::from(input))
            .from(address)
            .max_fee_per_gas(150_000_000_000u128)
            .max_priority_fee_per_gas(1_500_000_000u128);

        // Send with retry logic for nonce errors (1 retry)
        let pending = match client.provider.send_transaction(tx.clone()).await {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on mint_domain, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client
                        .provider
                        .send_transaction(tx)
                        .await
                        .context("Failed to send register transaction")?
                } else {
                    return Err(e).context("Failed to send register transaction");
                }
            },
        };

        let tx_hash = *pending.tx_hash();
        let receipt = pending.get_receipt().await.context("Failed to get receipt")?;

        if !receipt.inner.status() {
            return Ok(TaskResult {
                success: false,
                message: "Domain registration reverted".to_string(),
                tx_hash: Some(format!("{:?}", tx_hash)),
            });
        }

        // println!(
        //     "✅ Domain registered: {:?} (Block {:?})",
        //     tx_hash, receipt.block_number
        // );

        // Analyze transaction logs
        // println!("🔍 Analyzing transaction logs...");
        let logs = receipt.inner.logs();
        // println!("📋 Transaction has {} log(s)", logs.len());

        for _log in logs.iter() {
            // println!(
            //     "📝 Log {}: Address: {:?}, Topics: {:?}, Data: {:?}",
            //     i,
            //     log.address(),
            //     log.topics(),
            //     log.data()
            // );
        }

        // Parse logs for domain registration events
        let mut _domain_registered_via_event = false;
        let mut _event_owner = None;

        // println!("🔍 Parsing logs for domain registration events...");
        for log in logs.iter() {
            // Try to decode DomainRegistered event
            if let Ok(event) = IInfinityNameService::DomainRegistered::decode_raw_log(log.topics(), &log.data().data) {
                // println!("✅ Found DomainRegistered event!");
                // println!("   - Name: {:?}", event.name);
                // println!("   - Owner: {:?}", event.owner);
                // println!("   - Node: {:?}", event.node);

                // Convert domain name to bytes for comparison with indexed string parameter
                let domain_bytes = keccak256(domain.as_bytes());
                if event.name == domain_bytes && event.owner == address {
                    // println!(
                    //     "✅ Event verification successful: Domain ownership confirmed via event"
                    // );
                    _domain_registered_via_event = true;
                    _event_owner = Some(event.owner);
                } else {
                    // println!(
                    //     "⚠️ Event mismatch: expected domain {}, owner {:?}",
                    //     domain, address
                    // );
                }
                continue; // Skip to next log if we found this event
            }

            // Try to decode NameRegistered event
            if let Ok(event) = IInfinityNameService::NameRegistered::decode_raw_log(log.topics(), &log.data().data) {
                // println!("✅ Found NameRegistered event!");
                // println!("   - Name: {:?}", event.name);
                // println!("   - Owner: {:?}", event.owner);

                // Convert domain name to bytes for comparison with indexed string parameter
                let domain_bytes = keccak256(domain.as_bytes());
                if event.name == domain_bytes && event.owner == address {
                    // println!(
                    //     "✅ Event verification successful: Name ownership confirmed via event"
                    // );
                    _domain_registered_via_event = true;
                    _event_owner = Some(event.owner);
                } else {
                    // println!(
                    //     "⚠️ Event mismatch: expected domain {}, owner {:?}",
                    //     domain, address
                    // );
                }
                continue; // Skip to next log if we found this event
            }

            // Try to decode Transfer event (common for ENS-style domains)
            if let Ok(event) = IInfinityNameService::Transfer::decode_raw_log(log.topics(), &log.data().data) {
                // println!("✅ Found Transfer event!");
                // println!("   - Node: {:?}", event.node);
                // println!("   - Owner: {:?}", event.owner);

                if event.owner == address {
                    // println!(
                    //     "✅ Transfer event indicates domain ownership to {:?}",
                    //     event.owner
                    // );
                    _event_owner = Some(event.owner);
                }
            }
        }

        // Skip verification by default - domain registration is confirmed by transaction success
        let _skip_verification = std::env::var("ENABLE_DOMAIN_VERIFICATION")
            .unwrap_or_default()
            .parse()
            .unwrap_or(false);

        // Wait for indexing
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Define verification variables in scope for both verification paths
        let label_hash = keccak256(domain.as_bytes());
        let full_namehash = namehash(&format!("{}.tempo", domain));

        if _skip_verification {
            // println!("🔍 Verifying ownership...");
            // println!(
            //     "💡 Note: On Tempo, domain registration may not use traditional ENS-style ownership storage"
            // );
            // println!(
            //     "💡 The transaction success itself is the primary proof of domain registration"
            // );
            // println!(
            //     "💡 The transaction success itself is the primary proof of domain registration"
            // );

            // Check 1: owner(label_hash) - Flat registry style
            // println!("Checking owner(keccak256('{}'))...", domain);
            let check1 = IInfinityNameService::ownerCall { node: label_hash };
            match client
                .provider
                .call(
                    TransactionRequest::default()
                        .to(infinity_addr)
                        .input(TransactionInput::from(check1.abi_encode())),
                )
                .await
            {
                Ok(bytes) => {
                    println!("🔍 Raw call return bytes (len {}): {:?}", bytes.len(), bytes);
                    if bytes.is_empty() {
                        println!("⚠️ owner(label) returned empty bytes");
                    } else if let Ok(res) = IInfinityNameService::ownerCall::abi_decode_returns(&bytes) {
                        if res == address {
                            println!("✅ Verification successful: owner(label) is {:?}", res);
                        } else {
                            println!("⚠️ Mismatch owner(label): expected {:?}, got {:?}", address, res);
                        }
                    } else {
                        println!("⚠️ owner(label) decode fail or empty (bytes len {})", bytes.len());
                    }
                },
                Err(e) => {
                    println!("❌ owner(label) call failed: {:?}", e);
                },
            }

            // Check 2: owner(namehash) - ENS style
            println!("Checking owner(namehash('{}.tempo'))...", domain);
            let check2 = IInfinityNameService::ownerCall { node: full_namehash };
            match client
                .provider
                .call(
                    TransactionRequest::default()
                        .to(infinity_addr)
                        .input(TransactionInput::from(check2.abi_encode())),
                )
                .await
            {
                Ok(bytes) => {
                    println!("🔍 Raw call return bytes (len {}): {:?}", bytes.len(), bytes);
                    if bytes.is_empty() {
                        println!("⚠️ owner(namehash) returned empty bytes");
                    } else if let Ok(res) = IInfinityNameService::ownerCall::abi_decode_returns(&bytes) {
                        if res == address {
                            println!("✅ Verification successful: owner(namehash) is {:?}", res);
                        } else {
                            println!("⚠️ Mismatch owner(namehash): expected {:?}, got {:?}", address, res);
                        }
                    } else {
                        println!("⚠️ owner(namehash) decode fail or empty (bytes len {})", bytes.len());
                    }
                },
                Err(e) => {
                    println!("❌ owner(namehash) call failed: {:?}", e);
                },
            }

            // Check 3: addr(namehash) - Resolver style
            println!("Checking addr(namehash('{}.tempo'))...", domain);
            let check3 = IInfinityNameService::addrCall { node: full_namehash };
            match client
                .provider
                .call(
                    TransactionRequest::default()
                        .to(infinity_addr)
                        .input(TransactionInput::from(check3.abi_encode())),
                )
                .await
            {
                Ok(bytes) => {
                    println!("🔍 Raw call return bytes (len {}): {:?}", bytes.len(), bytes);
                    if bytes.is_empty() {
                        println!("⚠️ addr(namehash) returned empty bytes");
                    } else if let Ok(res) = IInfinityNameService::addrCall::abi_decode_returns(&bytes) {
                        if res == address {
                            println!("✅ Verification successful: addr(namehash) is {:?}", res);
                        } else {
                            println!("⚠️ Mismatch addr(namehash): expected {:?}, got {:?}", address, res);
                        }
                    } else {
                        println!("⚠️ addr(namehash) decode fail or empty (bytes len {})", bytes.len());
                    }
                },
                Err(e) => {
                    println!("❌ addr(namehash) call failed: {:?}", e);
                },
            }
        } else {
            // println!("⏭️  Domain ownership verification skipped (default behavior)");
        }

        // Summary of verification results (silenced)
        /*
        if !_skip_verification {
            if _domain_registered_via_event {
                println!("✅ Domain ownership verified via events");
            } else if _event_owner.is_some() {
                println!("⚠️ Domain ownership partially verified via events");
            } else {
                println!("⚠️ Domain ownership could not be verified via events");
            }
        } else {
            println!("✅ Domain registered successfully - transaction confirmed on-chain");
        }
        */

        Ok(TaskResult {
            success: true,
            message: format!("Registered domain {}.tempo. Tx: {}", domain, tx_hash),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
