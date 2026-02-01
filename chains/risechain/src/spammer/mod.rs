use crate::config::RiseConfig;
use crate::task::t01_check_balance::CheckBalanceTask;
use crate::task::t02_simple_eth_transfer::SimpleEthTransferTask;
use crate::task::t03_deploy_contract::DeployContractTask;
use crate::task::t04_interact_contract::InteractContractTask;
use crate::task::t05_self_transfer::SelfTransferTask;
use crate::task::t06_send_meme::SendMemeTokenTask;
use crate::task::t07_create_meme::CreateMemeTask;
use crate::task::t09_weth_wrap::WethWrapTask;
use crate::task::t10_weth_unwrap::WethUnwrapTask;
use crate::task::t11_batch_transfer::BatchTransferTask;
use crate::task::t12_nft_mint::NftMintTask;
use crate::task::t13_nft_transfer::NftTransferTask;
use crate::task::t14_approve_token::ApproveTokenTask;
use crate::task::t16_multicall::MulticallTask;
use crate::task::t17_read_oracle::ReadOracleTask;
use crate::task::t18_contract_call_raw::ContractCallRawTask;
use crate::task::t19_high_gas_limit::HighGasLimitTask;
use crate::task::t20_gas_price_test::GasPriceTestTask;
use crate::task::t21_erc1155_mint::Erc1155MintTask;
use crate::task::t22_erc1155_transfer::Erc1155TransferTask;
use crate::task::t23_timed_interaction::TimedInteractionTask;
use crate::task::t24_create2_deploy::Create2DeployTask;
use crate::task::t25_message_sign::MessageSignTask;
use crate::task::t26_verify_signature::VerifySignatureTask;
use crate::task::t27_permit_token::PermitTokenTask;
use crate::task::t28_delegatecall::DelegatecallTask;
use crate::task::t29_cross_contract_call::CrossContractCallTask;
use crate::task::t30_revert_test::RevertTestTask;
use crate::task::t31_event_emission::EventEmissionTask;
use crate::task::t32_eth_with_data::EthWithDataTask;
use crate::task::t33_batch_approve::BatchApproveTask;
use crate::task::t34_role_based_access::RoleBasedAccessTask;
use crate::task::t35_pausable_contract::PausableContractTask;
use crate::task::t36_create2_factory::Create2FactoryTask;
use crate::task::t37_uups_proxy::UUPSProxyTask;
use crate::task::t38_transparent_proxy::TransparentProxyTask;
use crate::task::t39_uniswap_v2_swap::UniswapV2SwapTask;
use crate::task::t40_erc4626_vault::ERC4626VaultTask;
use crate::task::t41_flash_loan::FlashLoanTestTask;
use crate::task::t42_erc721_mint::ERC721MintTask;
use crate::task::t43_erc1155_batch::ERC1155BatchTask;
use crate::task::t44_storage_pattern::StoragePatternTask;
use crate::task::t45_custom_error::CustomErrorTestTask;
use crate::task::t46_revert_reason::RevertWithReasonTask;
use crate::task::t47_assert_fail::AssertFailTask;
use crate::task::t48_anonymous_event::AnonymousEventTask;
use crate::task::t49_indexed_topics::IndexedTopicsTask;
use crate::task::t50_large_event::LargeEventDataTask;
use crate::task::t51_memory_expansion::MemoryExpansionTask;
use crate::task::t52_calldata_size::CalldataSizeTask;
use crate::task::t53_gas_stipend::GasStipendTask;
use crate::task::t54_gas_price_zero::GasPriceZeroTask;
use crate::task::t55_block_hash::BlockHashUsageTask;
use crate::task::{RiseTask, TaskContext};
use anyhow::Result;
use async_trait::async_trait;
use core_logic::config::SpamConfig;
use core_logic::traits::Spammer;
use ethers::prelude::*;
use rand::rngs::OsRng;

use rand::distributions::{Distribution, WeightedIndex};
use rand::Rng;
use reqwest::Client;
use tokio::time::{sleep, Duration};
use tracing::{info, warn, Instrument};

use core_logic::database::DatabaseManager;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct EvmSpammer {
    config: SpamConfig,
    provider: Provider<Http>,
    wallet: LocalWallet,
    tasks: Vec<Box<RiseTask>>,
    rise_config: RiseConfig,
    // Context IDs for logging
    wallet_id: String,
    proxy_id: String,
    proxy_url: Option<String>,
    // Database
    db: Option<Arc<DatabaseManager>>,
    gas_manager: Arc<crate::utils::gas::GasManager>,
    dist: WeightedIndex<u32>,
}

fn get_task_weight(name: &str) -> u32 {
    match name {
        "11_batchTransfer" => 50,
        "02_simpleEthTransfer" => 50,
        _ => 1, //default
    }
}

impl EvmSpammer {
    // Modified constructor to accept IDs
    pub fn new_with_signer(
        spam_config: SpamConfig,
        rise_config: RiseConfig,
        signer: LocalWallet,
        proxy_config: Option<core_logic::config::ProxyConfig>,
        wallet_id: String,
        proxy_id: String,
        db: Option<Arc<DatabaseManager>>,
    ) -> Result<Self> {
        // ... (client builder logic same as before)
        let mut client_builder = Client::builder();
        if let Some(proxy_conf) = &proxy_config {
            let mut proxy = reqwest::Proxy::all(&proxy_conf.url)?;
            if let (Some(u), Some(p)) = (&proxy_conf.username, &proxy_conf.password) {
                proxy = proxy.basic_auth(u, p);
            }
            client_builder = client_builder.proxy(proxy);
        }
        let client = client_builder.build()?;

        let provider = Provider::new(Http::new_with_client(
            reqwest::Url::parse(&spam_config.rpc_url)?,
            client,
        ));

        let tasks: Vec<Box<RiseTask>> = vec![
            Box::new(CheckBalanceTask),
            Box::new(SimpleEthTransferTask),
            Box::new(DeployContractTask),
            Box::new(InteractContractTask),
            Box::new(SelfTransferTask),
            Box::new(CreateMemeTask),
            Box::new(SendMemeTokenTask),
            Box::new(WethWrapTask),
            Box::new(WethUnwrapTask),
            Box::new(BatchTransferTask),
            Box::new(NftMintTask),
            Box::new(NftTransferTask),
            Box::new(ApproveTokenTask),
            Box::new(MulticallTask),
            Box::new(ReadOracleTask),
            Box::new(ContractCallRawTask),
            Box::new(HighGasLimitTask),
            Box::new(GasPriceTestTask),
            Box::new(Erc1155MintTask),
            Box::new(Erc1155TransferTask),
            Box::new(TimedInteractionTask),
            Box::new(Create2DeployTask),
            Box::new(MessageSignTask),
            Box::new(VerifySignatureTask),
            Box::new(PermitTokenTask),
            Box::new(DelegatecallTask),
            Box::new(CrossContractCallTask),
            Box::new(RevertTestTask),
            Box::new(EventEmissionTask),
            Box::new(EthWithDataTask),
            Box::new(BatchApproveTask),
            Box::new(RoleBasedAccessTask),
            Box::new(PausableContractTask),
            Box::new(Create2FactoryTask),
            Box::new(UUPSProxyTask),
            Box::new(TransparentProxyTask),
            Box::new(UniswapV2SwapTask),
            Box::new(ERC4626VaultTask),
            Box::new(FlashLoanTestTask),
            Box::new(ERC721MintTask),
            Box::new(ERC1155BatchTask),
            Box::new(StoragePatternTask),
            Box::new(CustomErrorTestTask),
            Box::new(RevertWithReasonTask),
            Box::new(AssertFailTask),
            Box::new(AnonymousEventTask),
            Box::new(IndexedTopicsTask),
            Box::new(LargeEventDataTask),
            Box::new(MemoryExpansionTask),
            Box::new(CalldataSizeTask),
            Box::new(GasStipendTask),
            Box::new(GasPriceZeroTask),
            Box::new(BlockHashUsageTask),
        ];

        let gas_manager = Arc::new(crate::utils::gas::GasManager::new(Arc::new(
            provider.clone(),
        )));

        // Calculate weights
        let weights: Vec<u32> = tasks
            .iter()
            .map(|t| {
                let w = get_task_weight(t.name());
                info!("Task '{}': Weight {}", t.name(), w);
                w
            })
            .collect();

        // Create weighted distribution with fallback for invalid weights
        let dist = match WeightedIndex::new(&weights) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(
                    target: "smart_main",
                    "Failed to create weighted distribution for tasks, using uniform distribution: {}",
                    e
                );
                // Fallback: all tasks have equal weight
                WeightedIndex::new(&vec![1; weights.len()]).unwrap_or_else(|e| {
                    // Ultimate fallback - single task with weight 1
                    tracing::error!(target: "smart_main", "Critical error creating distribution: {}", e);
                    WeightedIndex::new(&vec![1]).expect("Failed to create fallback distribution")
                })
            }
        };

        Ok(Self {
            provider,
            wallet: signer.with_chain_id(spam_config.chain_id),
            config: spam_config,
            tasks,
            rise_config,
            wallet_id,
            proxy_id,
            proxy_url: proxy_config.map(|p| p.url),
            db,
            gas_manager,
            dist,
        })
    }
}

#[async_trait]
impl Spammer for EvmSpammer {
    async fn new(_config: SpamConfig) -> Result<Self> {
        Err(anyhow::anyhow!("Use new_with_signer construction"))
    }

    async fn start(
        &self,
        cancellation_token: CancellationToken,
    ) -> Result<core_logic::traits::SpammerStats> {
        // Create context span
        let span = tracing::info_span!(
            "spammer_context",
            wallet_id = self.wallet_id.as_str(),
            proxy_id = self.proxy_id.as_str()
        );

        async move {
            info!("RISE Spammer started for chain {}", self.config.chain_id);
            let mut stats = core_logic::traits::SpammerStats::default();

            loop {
                // Check if cancelled before starting task
                if cancellation_token.is_cancelled() {
                    info!("Worker stopping (cancelled).");
                    break;
                }

                let task = {
                    let mut rng = OsRng;
                    let idx = self.dist.sample(&mut rng);
                    self.tasks.get(idx)
                };

                if let Some(task) = task {
                    // info!("Executing task: {}", task.name()); // User wants specific format, avoid raw info

                    let ctx = TaskContext {
                        provider: self.provider.clone(),
                        wallet: self.wallet.clone(),
                        config: self.rise_config.clone(),
                        proxy: self.proxy_url.clone(),
                        db: self.db.clone(),
                        gas_manager: self.gas_manager.clone(),
                    };

                    let start_time = std::time::Instant::now();
                    match task.run(ctx).await {
                        Ok(res) => {
                            stats.success += 1;
                            let duration = start_time.elapsed();
                            let block_num = match self.provider.get_block_number().await {
                                Ok(n) => n.to_string(),
                                Err(_) => "???".to_string(),
                            };

                            use colored::*;
                            // Helper for coloring
                            fn format_colored_message(msg: &str) -> String {
                                // Regex to find addresses 0x... and numbers
                                use regex::Regex;

                                // Color Addresses (Orange approx) -> using custom color if terminal supports, or Yellow/Red mix?
                                // colored crate supports .truecolor(r,g,b) or .custom("color")?
                                // Actually colored::Color::TrueColor usually works on modern terms.
                                // User asked for Orange. RGB (255, 165, 0).
                                // User asked for Orange. RGB (255, 165, 0).

                                // Replace numbers (decimals or integers) that are NOT part of address (hard with pure regex replacement on string that already has ansi codes).
                                // better approach: Regex find all tokens, colorize based on type.
                                // Simplest: Just regex numbers that are surrounded by space or start/end of string?
                                // \b\d+(\.\d+)?\b
                                // CAUTION: If we run this AFTER address coloring, the ANSI codes themselves have numbers (e.g. [38;2;...]).
                                // So we must be careful.
                                // Strategy: Capture text parts, reconstruct.
                                // OR: strict regex that excludes the ANSI patterns.

                                // Let's try to match numbers that are likely amounts/blocks.
                                // Given complexity, let's just color numbers in the raw message FIRST, BUT addresses contain numbers.
                                // Addresses start with 0x.

                                // CORRECT APPROACH:
                                // 1. Identify addresses and color them.
                                // 2. Identify numbers that are NOT inside addresses and color them.
                                // This is hard to do in two passes on string.
                                // One pass regex: (0x[a-fA-F0-9]+)|(\d+(\.\d+)?)
                                let token_regex =
                                    Regex::new(r"(0x[a-fA-F0-9]+)|(\d+(\.\d+)?)").unwrap();

                                let final_str = token_regex
                                    .replace_all(msg, |caps: &regex::Captures| {
                                        if let Some(addr) = caps.get(1) {
                                            addr.as_str().truecolor(255, 165, 0).to_string()
                                        // Orange
                                        } else {
                                            // Number
                                            caps[0].yellow().to_string()
                                        }
                                    })
                                    .to_string();

                                final_str
                            }

                            // Clip content to ensure total line length < 200 chars
                            // Overhead is ~75 chars, so 125 chars for message is safe.
                            let raw_msg = res.message.replace("\n", " | ");
                            let msg_limit = 125;
                            let clipped_msg = if raw_msg.chars().count() > msg_limit {
                                let truncated: String =
                                    raw_msg.chars().take(msg_limit - 3).collect();
                                format!("{}...", truncated)
                            } else {
                                raw_msg
                            };

                            let colored_msg = format_colored_message(&clipped_msg);
                            let colored_block = format_colored_message(&block_num); // It's just a number

                            // Smart duration color
                            let dur_secs = duration.as_secs_f64();
                            let dur_str = format!("{:.1}s", dur_secs);
                            let colored_dur = if dur_secs < 5.0 {
                                dur_str.green()
                            } else if dur_secs < 10.0 {
                                dur_str.truecolor(255, 165, 0) // Orange
                            } else {
                                dur_str.red()
                            };

                            // Status color
                            let status_str = "Success".green().bold();

                            // User requested format: Success [TaskName] Message (B: X) in Ys
                            info!(
                                target: "task_result",
                                "[WK:{}][WL:{}][P:{}] {} [{}] {} (B: {}) in {}",
                                self.wallet_id,
                                self.wallet_id,
                                self.proxy_id,
                                status_str,
                                task.name(),
                                colored_msg,
                                colored_block,
                                colored_dur
                            );

                            if let Some(db) = &self.db {
                                // DB expects clean string? remove ansi? Or keep it?
                                // Usually clean. Removing ANSI is annoying.
                                // Let's just log the RAW params to DB for now, modifying message would require re-cleaning.
                                // Current implementation passed regex-replaced string to `info!`.
                                // Code block re-uses `res.message` for DB. Excellent.

                                let _ = db
                                    .log_task_result(
                                        &self.wallet_id,
                                        &format!("{:?}", self.wallet.address()),
                                        task.name(),
                                        true,
                                        &format!("{} (B: {})", res.message, block_num),
                                        duration.as_millis() as u64,
                                    )
                                    .await;
                            }
                        }
                        Err(e) => {
                            stats.failed += 1;
                            let duration = start_time.elapsed();
                            use colored::*; // Ensure trait is in scope
                            let raw_err = format!("{:#}", e).replace("\n", " | ");
                            let msg_limit = 125;
                            let clipped_err = if raw_err.chars().count() > msg_limit {
                                let truncated: String =
                                    raw_err.chars().take(msg_limit - 3).collect();
                                format!("{}...", truncated)
                            } else {
                                raw_err
                            };

                            // Status color
                            // Added trailing space for alignment with "Success" (7 chars)
                            let status_str = "Failed ".red().bold();

                            warn!(
                                target: "task_result",
                                "[WK:{}][WL:{}][P:{}] {} [{}] {} in {:.1}s",
                                self.wallet_id,
                                self.wallet_id,
                                self.proxy_id,
                                status_str,
                                task.name(),
                                clipped_err,
                                duration.as_secs_f64()
                            );

                            if let Some(db) = &self.db {
                                let _ = db
                                    .log_task_result(
                                        &self.wallet_id,
                                        &format!("{:?}", self.wallet.address()),
                                        task.name(),
                                        false,
                                        &e.to_string(),
                                        duration.as_millis() as u64,
                                    )
                                    .await;
                            }
                        }
                    }
                }

                // Rate limit logic
                let sleep_ms = if let (Some(min), Some(max)) =
                    (self.rise_config.min_delay_ms, self.rise_config.max_delay_ms)
                {
                    let mut rng = OsRng;
                    rng.gen_range(min..=max)
                } else {
                    1000 / self.config.target_tps.max(1) as u64
                };

                // Use tokio::select! to listen for cancellation DURING sleep
                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        info!("Worker stopping (cancelled during sleep).");
                        break;
                    }
                    _ = sleep(Duration::from_millis(sleep_ms)) => {}
                }
            }
            Ok(stats)
        }
        .instrument(span)
        .await
    }

    async fn stop(&self) -> Result<()> {
        info!("RISE Spammer stopping...");
        Ok(())
    }
}
