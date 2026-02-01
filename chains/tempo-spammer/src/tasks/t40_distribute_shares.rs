//! Distribute Shares Task
//!
//! Deploys a Splitter contract with 5-15 random payees, funds it, and distributes shares.
//!
//! Workflow:
//! 1. Select 5-15 random unique payees
//! 2. Deploy TempoSplitter with random shares (sum 1000)
//! 3. Fund contract with system token
//! 4. Call distributeNative function

use crate::TempoClient;
use crate::tasks::TaskContext;
use crate::tasks::prelude::*;
use alloy::primitives::{Address, Bytes, TxKind, U256};
use alloy::rpc::types::TransactionRequest;
use alloy::sol_types::{SolCall, SolConstructor, SolEvent};
use alloy_sol_types::sol;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use rand::seq::SliceRandom;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::str::FromStr;

// Embed the contract bytecode
// Embed the contract bytecode from file
const BYTECODE_HEX: &str = include_str!("../contracts/build/TempoSplitter.bin");

use crate::tasks::tempo_tokens::TempoTokens;

sol!(
    interface ITempoSplitter {
        constructor(address[] memory payees, uint256[] memory shares_, string[] memory memos_) payable;
        function distributeNative();
        function distribute(address token);
        event PayeeAdded(address account, uint256 shares, string memo);
    }
    interface IERC20 {
        function transfer(address to, uint256 amount) external returns (bool);
    }
);

#[derive(Debug, Clone, Default)]
pub struct DistributeSharesTask;

impl DistributeSharesTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for DistributeSharesTask {
    fn name(&self) -> &'static str {
        "40_distribute_shares"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();
        let mut rng = rand::rngs::OsRng;

        // 1. Get random payees
        let payees = get_n_random_addresses(15)?;
        let count = payees.len();

        if count < 5 {
            return Ok(TaskResult {
                success: false,
                message: "Not enough addresses in address.txt to run task.".to_string(),
                tx_hash: None,
            });
        }

        let selected_count = rng.gen_range(5..=count.min(15));
        let mut selected_payees = payees.into_iter().collect::<Vec<_>>();
        selected_payees.shuffle(&mut rng);
        let selected_payees: Vec<Address> =
            selected_payees.into_iter().take(selected_count).collect();

        // 2. Generate random shares
        let shares = generate_random_shares(selected_count, 10000);
        let memos: Vec<String> = vec!["".to_string(); selected_count];

        // 3. Prepare Deployment Data
        let args = ITempoSplitter::constructorCall {
            payees: selected_payees.clone(),
            shares_: shares.iter().map(|&s| U256::from(s)).collect(),
            memos_: memos,
        };
        let bytecode = hex::decode(BYTECODE_HEX)?;
        let constructor_args = args.abi_encode();
        let deploy_data = [bytecode.as_slice(), constructor_args.as_slice()].concat();

        // 4. Select Token for Funding
        let token_info = TempoTokens::get_random_system_token();
        let token_addr = token_info.address;
        let fund_amount = U256::from(rng.gen_range(500000000..1000000000));

        // 5. Retry loop for nonce races
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 3;

        loop {
            // Get fresh nonce for each attempt
            let start_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
            let predicted_address = address.create(start_nonce);

            tracing::debug!(
                "🚀 Pipelining Attempt {}: Deploying to predicted {:?} (nonce {})",
                retry_count + 1,
                predicted_address,
                start_nonce
            );

            // Construct All Transactions with fresh nonces
            let mut deploy_tx = TransactionRequest::default()
                .input(deploy_data.clone().into())
                .from(address)
                .nonce(start_nonce)
                .gas_limit(10_000_000);
            deploy_tx.to = Some(TxKind::Create);

            let transfer_call = IERC20::transferCall {
                to: predicted_address,
                amount: fund_amount,
            };
            let fund_tx = TransactionRequest::default()
                .to(token_addr)
                .input(transfer_call.abi_encode().into())
                .from(address)
                .nonce(start_nonce + 1)
                .gas_limit(1_000_000);

            let distribute_call = ITempoSplitter::distributeCall { token: token_addr };
            let distribute_tx = TransactionRequest::default()
                .to(predicted_address)
                .input(distribute_call.abi_encode().into())
                .from(address)
                .nonce(start_nonce + 2)
                .gas_limit(4_000_000);

            // Execute concurrently
            let (p1, p2, p3) = tokio::join!(
                client.provider.send_transaction(deploy_tx),
                client.provider.send_transaction(fund_tx),
                client.provider.send_transaction(distribute_tx)
            );

            // Check results
            match (p1, p2, p3) {
                (Ok(deploy_pending), Ok(fund_pending), Ok(dist_pending)) => {
                    let deploy_hash = deploy_pending.tx_hash().clone();
                    let fund_hash = fund_pending.tx_hash().clone();
                    let dist_hash = dist_pending.tx_hash().clone();

                    tracing::debug!(
                        "Pipeline Sent! hashes: {:?}, {:?}, {:?}",
                        deploy_hash,
                        fund_hash,
                        dist_hash
                    );

                    // Update Nonce Manager
                    if let Some(manager) = &client.nonce_manager {
                        manager.set(address, start_nonce + 3).await;
                    }

                    return Ok(TaskResult {
                        success: true,
                        tx_hash: Some(format!("{:?}", dist_hash)),
                        message: format!(
                            "Pipelined 3 Txs (attempt {}): Deploy({:?}) -> Fund -> Distribute. Splitter: {:?}",
                            retry_count + 1,
                            deploy_hash,
                            predicted_address
                        ),
                    });
                }
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nonce too low") && retry_count < MAX_RETRIES {
                        retry_count += 1;
                        tracing::warn!(
                            "Nonce too low in pipeline (attempt {}/{}), retrying with fresh nonce...",
                            retry_count,
                            MAX_RETRIES
                        );
                        // Reset nonce cache and retry
                        client.reset_nonce_cache().await;
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        continue;
                    } else {
                        return Err(e).context("Pipeline transaction failed");
                    }
                }
            }
        }
    }
}
