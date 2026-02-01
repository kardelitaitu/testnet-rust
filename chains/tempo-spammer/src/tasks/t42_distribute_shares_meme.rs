//! Distribute Shares Meme Task
//!
//! Deploys a Splitter contract with 5-15 random payees, funds it with Created Meme Token,
//! and distributes shares.
//!
//! Workflow:
//! 1. Select 5-15 random unique payees
//! 2. Find a "meme" token from DB
//! 3. Deploy TempoSplitter with random shares (sum 10000)
//! 4. Fund contract with meme token
//! 5. Call distribute(token) function

use crate::TempoClient;
use crate::tasks::TaskContext;
use crate::tasks::prelude::*;
use crate::tasks::tempo_tokens::TempoTokens;
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

// Embed the contract bytecode (Same as Task 40/41)
const BYTECODE_HEX: &str = include_str!("../contracts/build/TempoSplitter.bin");

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
pub struct DistributeSharesMemeTask;

impl DistributeSharesMemeTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for DistributeSharesMemeTask {
    fn name(&self) -> &'static str {
        "42_distribute_shares_meme"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        // 1. Determine Meme Token Address (Created Meme coin or PathUSD)
        let mut token_addr = TempoTokens::get_path_usd_address();
        let mut using_created_token = false;

        if let Some(db) = &ctx.db {
            if let Ok(assets) = db.get_assets_by_type(&address.to_string(), "meme").await {
                if !assets.is_empty() {
                    let mut rng = rand::thread_rng();
                    if let Some(random_asset) = assets.choose(&mut rng) {
                        if let Ok(addr) = Address::from_str(random_asset) {
                            token_addr = addr;
                            using_created_token = true;
                        }
                    }
                }
            }
        }

        if using_created_token {
            tracing::debug!("Using random created meme token: {:?}", token_addr);
        } else {
            tracing::debug!("No created meme token found. Falling back to PathUSD.");
        }

        // 2. Get random payees
        let payees = get_n_random_addresses(15)?;
        let count = payees.len();
        if count < 5 {
            return Ok(TaskResult {
                success: false,
                message: "Not enough addresses to run task.".to_string(),
                tx_hash: None,
            });
        }

        let mut rng = rand::rngs::OsRng;
        let selected_count = rng.gen_range(5..=count.min(15));
        let mut selected_payees = payees.into_iter().collect::<Vec<_>>();
        selected_payees.shuffle(&mut rng);
        let selected_payees: Vec<Address> =
            selected_payees.into_iter().take(selected_count).collect();

        // 3. Generate random shares summing to 10000
        let shares = generate_random_shares(selected_count, 10000);
        let memos: Vec<String> = vec!["".to_string(); selected_count];

        // 4. Prepare Deployment Data
        let args = ITempoSplitter::constructorCall {
            payees: selected_payees.clone(),
            shares_: shares.iter().map(|&s| U256::from(s)).collect(),
            memos_: memos,
        };
        let bytecode = hex::decode(BYTECODE_HEX)?;
        let constructor_args = args.abi_encode();
        let deploy_data = [bytecode.as_slice(), constructor_args.as_slice()].concat();

        // 5. Get Start Nonce & Predict Address
        let start_nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let predicted_address = address.create(start_nonce);

        tracing::debug!(
            "🚀 Optimistic Pipelining (Meme): Deploying to predicted {:?}",
            predicted_address
        );

        // 6. Select Fund Amount (Optimistic - Skip Balance Check for Speed)
        let fund_amount = U256::from(rng.gen_range(500000000..1000000000));

        // 7. Construct All Transactions

        // Tx1: Deploy
        let mut deploy_tx = TransactionRequest::default()
            .input(deploy_data.into())
            .from(address)
            .nonce(start_nonce)
            .gas_limit(10_000_000);
        deploy_tx.to = Some(TxKind::Create);

        // Tx2: Fund
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

        // Tx3: Distribute
        let distribute_call = ITempoSplitter::distributeCall { token: token_addr };
        let distribute_tx = TransactionRequest::default()
            .to(predicted_address)
            .input(distribute_call.abi_encode().into())
            .from(address)
            .nonce(start_nonce + 2)
            .gas_limit(4_000_000);

        // 8. Execute concurrently
        tracing::debug!(
            "Bursting 3 Transactions (Nonce {}..{})",
            start_nonce,
            start_nonce + 2
        );

        let (p1, p2, p3) = tokio::join!(
            client.provider.send_transaction(deploy_tx),
            client.provider.send_transaction(fund_tx),
            client.provider.send_transaction(distribute_tx)
        );

        let deploy_hash = p1.context("Deploy Tx Submission Failed")?.tx_hash().clone();
        let _fund_hash = p2.context("Fund Tx Submission Failed")?.tx_hash().clone();
        let dist_hash = p3
            .context("Distribute Tx Submission Failed")?
            .tx_hash()
            .clone();

        // Update Nonce Manager after all 3 txs submitted
        if let Some(manager) = &client.nonce_manager {
            manager.set(address, start_nonce + 3).await;
        }

        Ok(TaskResult {
            success: true,
            tx_hash: Some(format!("{:?}", dist_hash)),
            message: format!(
                "Pipelined 3 Txs (Meme): Deploy({:?}) -> Fund -> Distribute. Splitter: {:?}",
                deploy_hash, predicted_address
            ),
        })
    }
}
