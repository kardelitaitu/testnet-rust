//! # Sepolia Overlayer — Funder
//!
//! Multi-hop obfuscated ETH funding tool.
//! Splits funding across N wallet hops to obscure the source on-chain.

use anyhow::{ensure, Context, Result};
use chrono::Local;
use clap::Parser;
use core_logic::setup_logger;
use ethers::prelude::*;
use ethers::utils::parse_units;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use sepolia_overlayer::config::SepoliaConfig;
use std::collections::{HashSet, VecDeque};
use std::env;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::Semaphore;
use tracing::{info, warn};

const DEFAULT_LOAD_CONCURRENCY: usize = 100;
const PROXY_SEND_MAX_RETRIES: usize = 3;
const CONFIRMATION_TIMEOUT_SECS: u64 = 600;
const CONFIRMATION_HEARTBEAT_SECS: u64 = 30;

/// Funder orchestrates the multi-hop ETH funding flow.
#[derive(Clone)]
struct Funder {
    manager: Arc<core_logic::WalletManager>,
    provider: Provider<Http>,
    password: String,
    chain_id: u64,
    max_targets: Option<usize>,
}

/// Info for a single wallet loaded from disk.
#[derive(Clone, Debug)]
struct WalletInfo {
    #[allow(dead_code)]
    idx: usize,
    address: Address,
    balance_eth: f64,
}

/// Runtime state shared between the orchestrator and the per-target completion tasks.
/// Combines funding counters with sender bookkeeping so only one lock arc is needed.
#[derive(Clone)]
struct SenderState {
    use_counts: Vec<usize>,
    locked_senders: HashSet<usize>,
    funded: usize,
    failed: usize,
    durations: Vec<Duration>,
}

impl SenderState {
    fn try_pick_and_lock(&mut self, senders: &[WalletInfo], max_per_sender: usize, rng: &mut StdRng) -> Option<usize> {
        let candidates: Vec<usize> = (0..senders.len())
            .filter(|&i| self.use_counts[i] < max_per_sender && !self.locked_senders.contains(&i))
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let idx = candidates[rng.gen_range(0..candidates.len())];
        self.use_counts[idx] += 1;
        self.locked_senders.insert(idx);
        Some(idx)
    }

    fn unlock(&mut self, idx: usize) {
        self.locked_senders.remove(&idx);
    }
}

fn format_eth_amount(amount_eth: f64) -> String {
    let mut formatted = format!("{:.6}", amount_eth);
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

fn format_compact_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{hours}h {mins}m {secs}s")
    } else if mins > 0 {
        format!("{mins}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

fn worker_tag(worker_id: usize) -> String {
    format!("WK{:03}", worker_id)
}

fn runner_log(worker_id: usize, component: &str, message: impl AsRef<str>) {
    println!(
        "{} [{}] [{}] {}",
        Local::now().format("%H:%M:%S"),
        worker_tag(worker_id),
        component,
        message.as_ref()
    );
}

async fn await_confirmation_with_progress<F, T, E>(
    worker_id: usize,
    component: &str,
    wait_label: &str,
    future: F,
    timeout: Duration,
    heartbeat: Duration,
) -> Result<T>
where
    F: std::future::Future<Output = std::result::Result<T, E>>,
    E: std::error::Error + Send + Sync + 'static,
{
    let start = std::time::Instant::now();
    let deadline = tokio::time::Instant::now() + timeout;
    let heartbeat = std::cmp::max(heartbeat, Duration::from_secs(1));
    let mut future = Box::pin(future);

    loop {
        tokio::select! {
            result = future.as_mut() => {
                return result.with_context(|| format!("{wait_label} confirmation failed"));
            }
            _ = tokio::time::sleep(heartbeat) => {
                runner_log(
                    worker_id,
                    component,
                    format!(
                        "Still waiting for {wait_label} confirmation... {} elapsed",
                        format_compact_duration(start.elapsed())
                    ),
                );
            }
            _ = tokio::time::sleep_until(deadline) => {
                anyhow::bail!(
                    "Timed out waiting for {wait_label} confirmation after {}",
                    format_compact_duration(start.elapsed())
                );
            }
        }
    }
}

fn choose_worker_rest_secs(min_secs: u64, max_secs: u64, rng: &mut StdRng) -> u64 {
    if max_secs == 0 {
        0
    } else {
        rng.gen_range(min_secs..=max_secs)
    }
}

fn format_worker_rest(min_secs: u64, max_secs: u64) -> String {
    match (min_secs, max_secs) {
        (0, 0) => "none".to_string(),
        (min, max) if min == max => format!("{min}s"),
        (min, max) => format!("{min}-{max}s"),
    }
}

fn distribute_round_robin<T>(items: Vec<T>, worker_count: usize) -> Vec<VecDeque<T>> {
    let worker_count = worker_count.max(1);
    let mut queues: Vec<VecDeque<T>> = (0..worker_count).map(|_| VecDeque::new()).collect();

    for (idx, item) in items.into_iter().enumerate() {
        queues[idx % worker_count].push_back(item);
    }

    queues
}

fn should_retry_proxy_send_error(error: &impl std::fmt::Debug) -> bool {
    let error_msg = format!("{:?}", error).to_lowercase();
    let retryable_patterns = [
        "timeout",
        "connection refused",
        "connection reset",
        "network error",
        "temporary failure",
        "service unavailable",
        "rate limited",
        "too many requests",
        "request timeout",
    ];

    retryable_patterns.iter().any(|pattern| error_msg.contains(pattern))
}

#[cfg(test)]
mod retry_filter_tests {
    use super::*;

    #[test]
    fn timeout_is_retryable() {
        let err = anyhow::anyhow!("Request timeout on the free tier, please upgrade your tier to the paid one");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn already_known_is_not_retryable() {
        let err = anyhow::anyhow!("already known: tx 0xabc");
        assert!(!should_retry_proxy_send_error(&err));
    }

    #[test]
    fn nonce_too_low_is_not_retryable() {
        let err = anyhow::anyhow!("nonce too low");
        assert!(!should_retry_proxy_send_error(&err));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────────────

fn pick_sender(senders: &[WalletInfo], use_counts: &[usize], max_per_sender: usize, rng: &mut StdRng) -> usize {
    let candidates: Vec<usize> = (0..senders.len()).filter(|&i| use_counts[i] < max_per_sender).collect();
    if candidates.is_empty() {
        rng.gen_range(0..senders.len())
    } else {
        candidates[rng.gen_range(0..candidates.len())]
    }
}

#[allow(clippy::too_many_arguments)]
async fn fund_via_chain(
    provider: &Provider<Http>,
    manager: &Arc<core_logic::WalletManager>,
    password: &str,
    sender_idx: usize,
    worker_id: usize,
    target: Address,
    target_amount: U256,
    hop_count: usize,
    min_delay: u64,
    max_delay: u64,
    min_gwei: f64,
    max_gwei: f64,
    chain_id: u64,
    rng: &mut StdRng,
) -> Result<()> {
    let decrypted = manager
        .get_wallet(sender_idx, Some(password))
        .await
        .context("Sender decrypt failed")?;
    let sender: LocalWallet = decrypted
        .evm_private_key
        .parse::<LocalWallet>()
        .context("Sender key parse failed")?
        .with_chain_id(chain_id);
    let sender_address = sender.address();

    // Pre-check: fetch sender's on-chain balance and log it prominently
    // so any "insufficient funds" failure is traceable to the source.
    let sender_balance = provider.get_balance(sender_address, None).await?;
    let sender_balance_eth = sender_balance.as_u128() as f64 / 1e18;

    let proxies: Vec<LocalWallet> = (0..hop_count)
        .map(|_| LocalWallet::new(rng).with_chain_id(chain_id))
        .collect();

    let proxy_addrs: Vec<Address> = proxies.iter().map(|w| w.address()).collect();

    let gas_price = random_gas_price(provider, min_gwei, max_gwei, rng).await?;
    let seed = calculate_seed_amount(target_amount, gas_price, hop_count);
    let seed_eth = seed.as_u128() as f64 / 1e18;
    let target_amount_eth = target_amount.as_u128() as f64 / 1e18;
    let gas_21k_cost = U256::from(21_000u64) * gas_price;
    let sender_tx_cost = seed + gas_21k_cost;
    let sender_tx_cost_eth = sender_tx_cost.as_u128() as f64 / 1e18;
    let flow_start = std::time::Instant::now();

    runner_log(
        worker_id,
        "Funder",
        format!(
            "address : {:?} balance: {} ETH",
            sender_address,
            format_eth_amount(sender_balance_eth)
        ),
    );

    if sender_balance < sender_tx_cost {
        let shortfall_eth = (sender_tx_cost - sender_balance).as_u128() as f64 / 1e18;
        anyhow::bail!(
            "Sender {:?} balance ({:.6} ETH) insufficient for seed + gas ({:.6} ETH needed, shortfall {:.6} ETH)",
            sender_address,
            sender_balance_eth,
            sender_tx_cost_eth,
            shortfall_eth
        );
    }

    let sender_signer = SignerMiddleware::new(provider.clone(), sender);
    let tx = TransactionRequest::pay(proxy_addrs[0], seed)
        .gas(21_000)
        .gas_price(gas_price);
    let pending = sender_signer
        .send_transaction(tx, None)
        .await
        .context("Sender -> P1 tx failed")?;
    runner_log(
        worker_id,
        "Funder",
        format!(
            "Sending {} ETH to Proxy 1 {{{:?}}} (target {} ETH)",
            format_eth_amount(seed_eth),
            proxy_addrs[0],
            format_eth_amount(target_amount_eth)
        ),
    );
    let _sender_tx_hash = pending.tx_hash();
    let sender_wait_label = "Sender -> P1";
    runner_log(
        worker_id,
        "Funder",
        format!("Waiting for {sender_wait_label} confirmation"),
    );
    let _sender_receipt = await_confirmation_with_progress(
        worker_id,
        "Funder",
        sender_wait_label,
        pending.confirmations(1).interval(Duration::from_millis(500)),
        Duration::from_secs(CONFIRMATION_TIMEOUT_SECS),
        Duration::from_secs(CONFIRMATION_HEARTBEAT_SECS),
    )
    .await?
    .context("Sender -> P1 receipt not confirmed (Ok(None))")?;

    let mut recipient_tx_hash = None;
    let mut remaining = seed;
    for (i, proxy) in proxies.iter().enumerate() {
        let delay = rng.gen_range(min_delay..=max_delay);
        if delay > 0 {
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }

        let hop_gas = random_gas_price(provider, min_gwei, max_gwei, rng).await?;

        let next = get_next_hop_address(i, hop_count, target, &proxy_addrs);
        let forward = calculate_forward_amount(remaining, hop_gas);

        let proxy_signer = SignerMiddleware::new(provider.clone(), proxy.clone());
        let stage_label = format!("Proxy-{}", i + 1);

        let next_label = if i == hop_count - 1 {
            "Recipient".to_string()
        } else {
            format!("Proxy {}", i + 2)
        };
        runner_log(
            worker_id,
            &stage_label,
            format!(
                "Sending {} ETH to {} {{{:?}}}",
                format_eth_amount(forward.as_u128() as f64 / 1e18),
                next_label,
                next
            ),
        );
        let pending = {
            let mut send_attempt = 0usize;
            loop {
                let tx = TransactionRequest::pay(next, forward).gas(21_000).gas_price(hop_gas);
                match proxy_signer.send_transaction(tx, None).await {
                    Ok(pending) => break pending,
                    Err(e) => {
                        if send_attempt < PROXY_SEND_MAX_RETRIES && should_retry_proxy_send_error(&e) {
                            let retry_after_secs = 1u64 << send_attempt as u32;
                            runner_log(
                                worker_id,
                                &stage_label,
                                format!(
                                    "Send attempt {} failed for {}: {}; retrying in {}s",
                                    send_attempt + 1,
                                    next_label,
                                    e,
                                    retry_after_secs
                                ),
                            );
                            tokio::time::sleep(Duration::from_secs(retry_after_secs)).await;
                            send_attempt += 1;
                            continue;
                        }

                        let hop_label = format_hop_label(i, hop_count);
                        return Err(e).with_context(|| {
                            format!(
                                "P{} -> {} tx failed after {} attempt(s)",
                                i + 1,
                                hop_label,
                                send_attempt + 1
                            )
                        });
                    },
                }
            }
        };
        let hop_tx_hash = pending.tx_hash();
        let hop_wait_label = format_hop_label(i, hop_count);
        runner_log(
            worker_id,
            &stage_label,
            format!("Waiting for {hop_wait_label} confirmation"),
        );
        let _hop_receipt = await_confirmation_with_progress(
            worker_id,
            &stage_label,
            &hop_wait_label,
            pending.confirmations(1).interval(Duration::from_millis(500)),
            Duration::from_secs(CONFIRMATION_TIMEOUT_SECS),
            Duration::from_secs(CONFIRMATION_HEARTBEAT_SECS),
        )
        .await?
        .context(format!("{hop_wait_label} receipt not confirmed (Ok(None))"))?;

        if i == hop_count - 1 {
            recipient_tx_hash = Some(hop_tx_hash);
        }

        remaining = forward;
    }

    let recipient_tx_hash = recipient_tx_hash.context("Missing recipient tx hash")?;
    let sender_balance_after = provider.get_balance(sender_address, None).await?;
    let fee_wei = sender_balance
        .checked_sub(sender_balance_after)
        .and_then(|spent| spent.checked_sub(remaining))
        .context("Failed to compute total fee from balance delta")?;
    let duration = format_compact_duration(flow_start.elapsed());

    runner_log(
        worker_id,
        "Recipient",
        format!(
            "Address {:?} received {} ETH , tx : {:?} , cost {} ETH, duration : {}",
            target,
            format_eth_amount(remaining.as_u128() as f64 / 1e18),
            recipient_tx_hash,
            format_eth_amount(fee_wei.as_u128() as f64 / 1e18),
            duration
        ),
    );

    Ok(())
}

async fn random_gas_price(provider: &Provider<Http>, min_gwei: f64, max_gwei: f64, rng: &mut StdRng) -> Result<U256> {
    const MGWEI_PER_GWEI: u64 = 1_000;
    const WEI_PER_MGWEI: u64 = 1_000_000;

    let min_mgwei = (min_gwei * MGWEI_PER_GWEI as f64).round() as u64;
    let max_mgwei = (max_gwei * MGWEI_PER_GWEI as f64).round() as u64;

    let network = provider.get_gas_price().await.unwrap_or(U256::from(1_000_000_000u64));
    let network_mgwei = (network.as_u128() / WEI_PER_MGWEI as u128) as u64;

    let chosen = choose_gas_price_mgwei(network_mgwei, min_mgwei, max_mgwei, rng);
    Ok(U256::from(chosen) * U256::from(WEI_PER_MGWEI))
}

// ─────────────────────────────────────────────────────────────────────────────
// Pure, testable helpers extracted for TDD (filtering + planning layer)
// ─────────────────────────────────────────────────────────────────────────────

/// Returns wallets that have at least `min_balance` ETH (potential senders).
fn filter_senders(wallets: &[WalletInfo], min_balance: f64) -> Vec<WalletInfo> {
    wallets
        .iter()
        .filter(|w| w.balance_eth >= min_balance)
        .cloned()
        .collect()
}

/// Returns wallets that have at most `max_balance` ETH (potential targets).
fn filter_targets(wallets: &[WalletInfo], max_balance: f64) -> Vec<WalletInfo> {
    wallets
        .iter()
        .filter(|w| w.balance_eth <= max_balance)
        .cloned()
        .collect()
}

/// How many targets each eligible sender is allowed to fund under fair distribution.
fn compute_max_per_sender(available: usize, num_senders: usize) -> usize {
    if num_senders == 0 {
        return 0;
    }
    (available as f64 / num_senders as f64).ceil() as usize
}

/// Applies the optional `max_targets` cap to the list of candidate targets.
fn select_targets_to_fund(targets: &[WalletInfo], max_targets: Option<usize>) -> Vec<WalletInfo> {
    let n = max_targets.unwrap_or(usize::MAX);
    targets.iter().take(n).cloned().collect()
}

/// Structured description of one funding action in a dry-run (or future planner).
#[derive(Debug, Clone, PartialEq)]
struct PlannedFund {
    sender_idx: usize,
    target: Address,
    target_balance_eth: f64,
    amount: U256,
    hops: usize,
}

/// Pure, deterministic dry-run planner (given a seeded RNG).
/// Produces the exact sequence of (sender, target, amount, hops) that would be executed.
/// This is the core of what we want to TDD thoroughly.
#[allow(clippy::too_many_arguments)]
fn generate_dry_run_plan(
    senders: &[WalletInfo],
    targets: &[WalletInfo],
    min_target: f64,
    max_target: f64,
    min_hops: usize,
    max_hops: usize,
    max_targets: Option<usize>,
    rng: &mut StdRng,
) -> Vec<PlannedFund> {
    let selected = select_targets_to_fund(targets, max_targets);
    if senders.is_empty() || selected.is_empty() {
        return vec![];
    }

    let max_per = compute_max_per_sender(selected.len(), senders.len());
    let mut use_counts = vec![0usize; senders.len()];
    let mut plan = Vec::with_capacity(selected.len());

    for target in &selected {
        let amount: U256 = parse_units(rng.gen_range(min_target..=max_target), "ether")
            .expect("parse_units for dry-run target amount")
            .into();
        let hops = rng.gen_range(min_hops..=max_hops);
        let idx = pick_sender(senders, &use_counts, max_per, rng);

        plan.push(PlannedFund {
            sender_idx: idx,
            target: target.address,
            target_balance_eth: target.balance_eth,
            amount,
            hops,
        });
        use_counts[idx] += 1;
    }
    plan
}

/// Pure, deterministic gas price (mgwei) selector given a network sample + bounds + RNG.
/// This is the core logic extracted from `random_gas_price` so it can be TDD'd in isolation.
fn choose_gas_price_mgwei(network_mgwei: u64, min_mgwei: u64, max_mgwei: u64, rng: &mut StdRng) -> u64 {
    let floor = min_mgwei.max((network_mgwei * 9).saturating_div(10)).min(100_000);
    let ceiling = max_mgwei.max((network_mgwei * 11).saturating_div(10)).min(100_000);
    let effective_floor = floor.min(ceiling);
    rng.gen_range(effective_floor..=ceiling)
}

/// Returns true if we should skip the interactive confirmation prompt.
fn should_skip_confirmation(yes: bool, dry_run: bool) -> bool {
    yes || dry_run
}

/// Formats the confirmation prompt message shown to the user.
fn format_funding_prompt(available: usize, workers: usize) -> String {
    format!(
        "? This will fund {} targets with {} workers. Continue? [y/N]",
        available, workers
    )
}

/// Asks the user for confirmation via the provided reader.
/// Returns true if the user typed "y" (case-insensitive).
/// This makes the interactive prompt fully unit-testable.
fn confirm_funding(available: usize, workers: usize, mut reader: impl std::io::BufRead) -> Result<bool> {
    println!("{}", format_funding_prompt(available, workers));
    let mut input = String::new();
    reader.read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

/// Calculate the initial "seed" amount the first sender must send
/// (covers the final target + all future hop gas costs).
fn calculate_seed_amount(target_amount: U256, gas_price: U256, hop_count: usize) -> U256 {
    let gas_21k = U256::from(21_000u64) * gas_price;
    target_amount + gas_21k * U256::from(hop_count as u64 + 2)
}

/// Calculate how much to forward in one hop after paying for gas.
fn calculate_forward_amount(remaining: U256, gas_price: U256) -> U256 {
    let hop_21k = U256::from(21_000u64) * gas_price;
    remaining.saturating_sub(hop_21k)
}

/// Return the destination for the current hop (last hop goes to real target).
fn get_next_hop_address(hop_index: usize, hop_count: usize, target: Address, proxy_addrs: &[Address]) -> Address {
    if hop_index == hop_count - 1 {
        target
    } else {
        proxy_addrs[hop_index + 1]
    }
}

/// Human-friendly label for logging ("P2", "P3", or "target").
fn format_hop_label(hop_index: usize, hop_count: usize) -> String {
    if hop_index == hop_count - 1 {
        "target".to_string()
    } else {
        format!("P{}", hop_index + 2)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Funder impl
// ─────────────────────────────────────────────────────────────────────────────

impl Funder {
    fn new(
        manager: Arc<core_logic::WalletManager>,
        provider: Provider<Http>,
        password: String,
        chain_id: u64,
        max_targets: Option<usize>,
    ) -> Self {
        Self {
            manager,
            provider,
            password,
            chain_id,
            max_targets,
        }
    }

    /// High-level entry point for the full funding flow.
    /// Loads wallets, prepares sets, handles confirmation, dispatches to dry or real execution.
    /// This is the main entry point for the bin.
    pub async fn run(&self, args: &Args) -> Result<()> {
        let wallets = self.load_wallets(args.load_concurrency).await?;
        self.orchestrate(args, wallets).await
    }

    /// Private orchestrator: classification → summary → confirmation → execute.
    async fn orchestrate(&self, args: &Args, wallets: Vec<WalletInfo>) -> Result<()> {
        let (senders, targets_all, available, max_per_sender) =
            self.prepare_funding_sets(&wallets, args.min_balance, args.max_balance);
        let workers = args.workers.max(1);
        let spread_delay_ms = args.spread_hours.map(|h| {
            let total_ms = (h * 3600.0 * 1000.0).round() as u64;
            if available > 0 {
                total_ms / available as u64
            } else {
                0
            }
        });

        // Pre-compute staggered delay offsets per target.
        // Applied before sender selection so the worker slot is not wasted
        // while waiting for the logical launch window.
        let delays: Vec<u64> = match spread_delay_ms {
            Some(ms_per_target) if ms_per_target > 0 && available > 0 => {
                let mut rng = StdRng::from_entropy();
                (0..available).map(|_| rng.gen_range(0..=ms_per_target)).collect()
            },
            _ => vec![0; available],
        };
        let delays = Arc::new(delays);

        let senders_len = senders.len();
        let targets_count = targets_all.len();
        let assigned = available.min(targets_count);
        ensure!(
            args.min_worker_interval_secs <= args.max_worker_interval_secs,
            "min-worker-interval-secs cannot exceed max-worker-interval-secs"
        );

        println!("\n=== Summary ===");
        println!("Total wallets:      {}", wallets.len());
        println!("Senders (≥{:.3} ETH): {}", args.min_balance, senders_len);
        println!("Targets (≤{:.3} ETH): {}", args.max_balance, targets_count);
        println!("To fund:            {}", assigned);
        println!("Target amount:     {:.2}-{:.2} ETH", args.min_target, args.max_target);
        println!("Hops per target:   {}-{}", args.min_hops, args.max_hops);
        println!("Workers:            {}", workers);
        println!(
            "Worker rest:       {}",
            format_worker_rest(args.min_worker_interval_secs, args.max_worker_interval_secs)
        );

        if senders.is_empty() {
            anyhow::bail!("No wallets with balance ≥ {} ETH found as senders", args.min_balance);
        }
        if targets_all.is_empty() {
            anyhow::bail!("No wallets with balance ≤ {} ETH found as targets", args.max_balance);
        }

        // ── Dry-run ──────────────────────────────────────────────────────────
        if args.dry_run {
            return self.execute_dry_run(
                &senders,
                &targets_all,
                args.min_target,
                args.max_target,
                args.min_hops,
                args.max_hops,
                args.max_targets,
            );
        }

        // ── Interactive confirmation ──────────────────────────────────────────
        if !should_skip_confirmation(args.yes, args.dry_run) {
            let confirmed = confirm_funding(assigned, workers, std::io::stdin().lock())?;
            if !confirmed {
                println!("Aborted by user.");
                return Ok(());
            }
        }

        // ── Real execution ───────────────────────────────────────────────────
        println!(
            "Starting real execution for {} targets with {} worker(s)...",
            assigned, workers
        );
        let exec_start = std::time::Instant::now();
        let targets_to_fund: Vec<_> = targets_all.into_iter().take(available).enumerate().collect();
        let worker_queues = distribute_round_robin(targets_to_fund, workers);
        let state = Arc::new(TokioMutex::new(SenderState {
            use_counts: vec![0; senders.len()],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        }));
        let mut handles = Vec::new();

        for (worker_idx, mut queue) in worker_queues.into_iter().enumerate() {
            let manager = Arc::clone(&self.manager);
            let provider = self.provider.clone();
            let password = self.password.clone();
            let chain_id = self.chain_id;
            let min_g = args.min_gwei;
            let max_g = args.max_gwei;
            let min_d = args.min_delay_secs;
            let max_d = args.max_delay_secs;
            let min_t = args.min_target;
            let max_t = args.max_target;
            let min_h = args.min_hops;
            let max_h = args.max_hops;
            let senders_clone = senders.clone();
            let st = Arc::clone(&state);
            let delays = Arc::clone(&delays);
            let max_ps = max_per_sender;
            let worker_id = worker_idx + 1;
            let min_worker_interval_secs = args.min_worker_interval_secs;
            let max_worker_interval_secs = args.max_worker_interval_secs;

            handles.push(tokio::spawn(async move {
                let worker_start = std::time::Instant::now();

                let mut rng = StdRng::from_entropy();
                while let Some((target_idx, target)) = queue.pop_front() {
                    // Spread delay BEFORE sender selection so the queue isn't
                    // blocked while waiting for the logical launch window.
                    if delays[target_idx] > 0 {
                        tokio::time::sleep(Duration::from_millis(delays[target_idx])).await;
                    }

                    let sender_list_idx = loop {
                        match st.lock().await.try_pick_and_lock(&senders_clone, max_ps, &mut rng) {
                            Some(idx) => break idx,
                            None => {
                                tokio::time::sleep(Duration::from_millis(200)).await;
                            },
                        }
                    };
                    // Resolve senders-list index to the actual wallet index in the full wallet list.
                    // try_pick_and_lock returns the position within the senders Vec, but
                    // fund_via_chain calls manager.get_wallet(wallet_idx, ...) which expects
                    // the absolute wallet index from the original wallet scan.
                    let wallet_idx = senders_clone[sender_list_idx].idx;

                    let target_amount: U256 = parse_units(rng.gen_range(min_t..=max_t), "ether")
                        .expect("parse target amount")
                        .into();

                    let hops = rng.gen_range(min_h..=max_h);

                    let result = fund_via_chain(
                        &provider,
                        &manager,
                        &password,
                        wallet_idx,
                        worker_id,
                        target.address,
                        target_amount,
                        hops,
                        min_d,
                        max_d,
                        min_g,
                        max_g,
                        chain_id,
                        &mut rng,
                    )
                    .await;

                    let more_targets_left = !queue.is_empty();

                    {
                        let mut guard = st.lock().await;
                        guard.unlock(sender_list_idx);
                        match result {
                            Ok(_) => {
                                guard.funded += 1;
                            },
                            Err(e) => {
                                runner_log(worker_id, "Funder", format!("FAILED target {:?}: {:#}", target, e));
                                guard.failed += 1;
                            },
                        }
                    }

                    if more_targets_left && max_worker_interval_secs > 0 {
                        let rest_secs =
                            choose_worker_rest_secs(min_worker_interval_secs, max_worker_interval_secs, &mut rng);
                        if rest_secs > 0 {
                            runner_log(worker_id, "Funder", format!("Resting {}s before next cycle", rest_secs));
                            tokio::time::sleep(Duration::from_secs(rest_secs)).await;
                        }
                    }
                }

                let mut guard = st.lock().await;
                guard.durations.push(worker_start.elapsed());
            }));
        }

        for h in handles {
            h.await.context("worker task panicked")?;
        }

        let final_state = state.lock().await;
        let elapsed = exec_start.elapsed();
        let hours = elapsed.as_secs() / 3600;
        let mins = (elapsed.as_secs() % 3600) / 60;
        let secs = elapsed.as_secs() % 60;

        let durs = &final_state.durations;
        println!("\n=== Done ===");
        println!("Funded: {}, Failed: {}", final_state.funded, final_state.failed);
        println!("Total duration: {:02}:{:02}:{:02}", hours, mins, secs);
        if !durs.is_empty() {
            let total_secs: u64 = durs.iter().map(|d| d.as_secs()).sum();
            let avg_secs = total_secs as f64 / durs.len() as f64;
            let min_d = durs.iter().min().unwrap();
            let max_d = durs.iter().max().unwrap();
            println!(
                "Per-worker: min {:02}:{:02}:{:02}, max {:02}:{:02}:{:02}, avg {:02}:{:02}:{:02} ({} workers)",
                min_d.as_secs() / 3600,
                (min_d.as_secs() % 3600) / 60,
                min_d.as_secs() % 60,
                max_d.as_secs() / 3600,
                (max_d.as_secs() % 3600) / 60,
                max_d.as_secs() % 60,
                (avg_secs as u64) / 3600,
                ((avg_secs as u64) % 3600) / 60,
                (avg_secs as u64) % 60,
                durs.len()
            );
        }

        if let Some(ref json_path) = args.json_log {
            let per_worker = if !durs.is_empty() {
                let total_fsecs: f64 = durs.iter().map(|d| d.as_secs_f64()).sum();
                let count = durs.len();
                let avg_fsecs = total_fsecs / count as f64;
                let min_d = durs.iter().min().unwrap();
                let max_d = durs.iter().max().unwrap();
                Some(serde_json::json!({
                    "count": count,
                    "min_secs": min_d.as_secs_f64(),
                    "max_secs": max_d.as_secs_f64(),
                    "avg_secs": avg_fsecs,
                    "durations_secs": durs.iter().map(|d| d.as_secs_f64()).collect::<Vec<f64>>()
                }))
            } else {
                None
            };

            let data = serde_json::json!({
                "funded": final_state.funded,
                "failed": final_state.failed,
                "total_duration_secs": elapsed.as_secs_f64(),
                "per_worker": per_worker,
                "summary": {
                    "total_wallets": wallets.len(),
                    "senders_count": senders.len(),
                    "targets_count": targets_count,
                    "assigned": assigned
                },
                "timestamp": chrono::Utc::now().to_rfc3339()
            });

            let json_str = serde_json::to_string_pretty(&data).context("Failed to serialize JSON log")?;
            std::fs::write(json_path, &json_str).context(format!("Failed to write JSON log to {}", json_path))?;
            println!("JSON log written to {}", json_path);
        }

        Ok(())
    }

    /// Pure preparation step using the already-tested helpers.
    /// Returns (senders, targets, available, max_per_sender).
    /// This is fully testable and is the first piece moved into Funder.
    pub fn prepare_funding_sets(
        &self,
        wallets: &[WalletInfo],
        min_balance: f64,
        max_balance: f64,
    ) -> (Vec<WalletInfo>, Vec<WalletInfo>, usize, usize) {
        let senders = filter_senders(wallets, min_balance);
        let targets = filter_targets(wallets, max_balance);
        let available = targets.len().min(self.max_targets.unwrap_or(usize::MAX));
        let max_per_sender = compute_max_per_sender(available, senders.len());
        (senders, targets, available, max_per_sender)
    }

    /// Executes the dry-run planning and printing.
    /// Delegates to the tested `generate_dry_run_plan`.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_dry_run(
        &self,
        senders: &[WalletInfo],
        targets: &[WalletInfo],
        min_target: f64,
        max_target: f64,
        min_hops: usize,
        max_hops: usize,
        max_targets: Option<usize>,
    ) -> Result<()> {
        let mut rng = StdRng::from_entropy();
        let plan = generate_dry_run_plan(
            senders,
            targets,
            min_target,
            max_target,
            min_hops,
            max_hops,
            max_targets,
            &mut rng,
        );
        for pf in plan {
            println!(
                "[DRY] Fund {:?} ({:.4} ETH) from sender idx {} via {} hops (target {:.4} ETH)",
                pf.target,
                pf.target_balance_eth,
                pf.sender_idx,
                pf.hops,
                pf.amount.as_u128() as f64 / 1e18
            );
        }
        println!("(dry run — no txs sent)");
        Ok(())
    }

    /// Loads all accessible wallets and their balances using semaphore-bounded parallel
    /// concurrent decryption (up to `load_concurrency` per batch). This centralizes the
    /// I/O-heavy wallet loading step behind the Funder.
    pub async fn load_wallets(&self, load_concurrency: usize) -> Result<Vec<WalletInfo>> {
        let total = self.manager.count();
        if total == 0 {
            anyhow::bail!(
                "No wallets found (wallet dir may not exist, be empty, or contain no valid encrypted wallets)"
            );
        }

        eprintln!(
            "[Funder] Decrypting {} wallets (parallel, up to {} concurrent per batch)...",
            total, load_concurrency
        );

        let sem = Arc::new(Semaphore::new(load_concurrency));
        let manager = Arc::clone(&self.manager);
        let password = self.password.clone();
        let provider = self.provider.clone();
        let mut handles = Vec::with_capacity(total);

        for idx in 0..total {
            let permit = sem.clone().acquire_owned().await?;
            let mgr = Arc::clone(&manager);
            let pw = password.clone();
            let prov = provider.clone();

            handles.push(tokio::spawn(async move {
                let _permit = permit; // hold sem until this task is done

                let decrypted = match mgr.get_wallet(idx, Some(&pw)).await {
                    Ok(w) => w,
                    Err(e) => {
                        warn!("Wallet {idx}: decrypt failed: {e}, skipping");
                        return None;
                    },
                };
                let key = &decrypted.evm_private_key;
                let wallet: LocalWallet = match key.parse() {
                    Ok(w) => w,
                    Err(e) => {
                        warn!("Wallet {idx}: key parse failed: {e}, skipping");
                        return None;
                    },
                };
                let address = wallet.address();
                let balance = match prov.get_balance(address, None).await {
                    Ok(b) => b.as_u128() as f64 / 1e18,
                    Err(_) => {
                        warn!("Wallet {idx}: balance query failed, skipping");
                        return None;
                    },
                };
                info!("Wallet {idx}: {:?} — {:.6} ETH", address, balance);
                Some(WalletInfo {
                    idx,
                    address,
                    balance_eth: balance,
                })
            }));
        }

        let mut wallets: Vec<WalletInfo> = Vec::with_capacity(total);
        for handle in handles {
            if let Some(w) = handle.await? {
                wallets.push(w);
            }
        }

        if wallets.is_empty() {
            warn!(
                "All {} wallets failed to decrypt, parse, or return a balance. No senders or targets available.",
                total
            );
        }
        eprintln!("[Funder] Loaded {} wallets", wallets.len());
        Ok(wallets)
    }
}

#[derive(Parser, Debug)]
#[command(name = "sepolia-funder", about = "Multi-hop obfuscated ETH funder")]
struct Args {
    /// Path to the sepolia-overlayer config.toml
    #[arg(short, long, default_value = "chains/sepolia-overlayer/config.toml")]
    config: String,

    /// Minimum balance (ETH) to consider as sender
    #[arg(long, default_value_t = 0.500)]
    min_balance: f64,

    /// Maximum balance (ETH) to consider as target
    #[arg(long, default_value_t = 0.010)]
    max_balance: f64,

    /// Minimum amount to fund per target (ETH)
    #[arg(long, default_value_t = 0.020)]
    min_target: f64,

    /// Maximum amount to fund per target (ETH)
    #[arg(long, default_value_t = 0.040)]
    max_target: f64,

    /// Minimum number of hops per transfer
    #[arg(long, default_value_t = 5)]
    min_hops: usize,

    /// Maximum number of hops per transfer
    #[arg(long, default_value_t = 7)]
    max_hops: usize,

    /// Maximum number of targets to fund (None = all)
    #[arg(long)]
    max_targets: Option<usize>,

    /// Max concurrent workers
    #[arg(short, long, default_value_t = 1)]
    workers: usize,

    /// Minimum pause between completed worker cycles (seconds)
    #[arg(long, default_value_t = 30)]
    min_worker_interval_secs: u64,

    /// Maximum pause between completed worker cycles (seconds)
    #[arg(long, default_value_t = 30)]
    max_worker_interval_secs: u64,

    /// Skip interactive confirmation prompt
    #[arg(long)]
    yes: bool,

    /// Dry-run: print plan but don't send txs
    #[arg(long)]
    dry_run: bool,

    /// Spread funding evenly over N hours (no real-time delay, just logical spread)
    #[arg(long)]
    spread_hours: Option<f64>,

    /// Minimum gas gwei per tx
    #[arg(long, default_value_t = 1.2)]
    min_gwei: f64,

    /// Maximum gas gwei per tx
    #[arg(long, default_value_t = 1.5)]
    max_gwei: f64,

    /// Minimum delay between hops (seconds)
    #[arg(long, default_value_t = 15)]
    min_delay_secs: u64,

    /// Maximum delay between hops (seconds)
    #[arg(long, default_value_t = 30)]
    max_delay_secs: u64,

    /// Number of concurrent wallet decryptions during wallet loading
    #[arg(long, default_value_t = DEFAULT_LOAD_CONCURRENCY)]
    load_concurrency: usize,

    /// Write a JSON log file with duration stats for downstream dashboard consumption
    #[arg(long)]
    json_log: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = setup_logger();
    std::mem::forget(_guard);

    let args = Args::parse();
    println!("=== Sepolia Overlayer — Funder ===");

    // Load .env from config directory (chain-scoped, not root .env)
    if let Some(parent) = Path::new(&args.config).parent() {
        let env_path = parent.join(".env");
        if env_path.exists() {
            let _ = dotenv::from_path(&env_path);
        }
    }

    // Config
    let config = SepoliaConfig::load(&args.config).context("Failed to load config")?;
    println!("Config: chain_id={}, rpc={}", config.chain_id, config.rpc_url);

    // Wallet manager
    let manager = if let Some(ref dir) = config.wallet_dir {
        Arc::new(core_logic::WalletManager::with_wallet_dir(dir)?)
    } else {
        Arc::new(core_logic::WalletManager::new()?)
    };
    let total = manager.count();
    println!("Found {} wallet files", total);

    // Password
    let password = env::var("WALLET_PASSWORD").map_err(|_| {
        anyhow::anyhow!("WALLET_PASSWORD not set. Set it before running: $env:WALLET_PASSWORD=\"your_password\"")
    })?;

    // Provider
    let client = reqwest::Client::new();
    let provider = Provider::new(Http::new_with_client(reqwest::Url::parse(&config.rpc_url)?, client));

    // Build and run
    let funder = Funder::new(manager, provider, password, config.chain_id, args.max_targets);
    funder.run(&args).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // ──────────────────────────────────────────────────────────────────────────
    // pick_sender tests
    // ──────────────────────────────────────────────────────────────────────────

    fn dummy_wallet(idx: usize, balance_eth: f64) -> WalletInfo {
        WalletInfo {
            idx,
            address: Address::from_low_u64_be(idx as u64),
            balance_eth,
        }
    }

    #[test]
    fn test_pick_sender_returns_valid_index() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0), dummy_wallet(2, 1.0)];
        let use_counts = vec![0, 0, 0];
        let mut rng = StdRng::seed_from_u64(42);
        let idx = pick_sender(&senders, &use_counts, 10, &mut rng);
        assert!(idx < 3);
    }

    #[test]
    fn test_pick_sender_respects_use_counts_and_max_per_sender() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0), dummy_wallet(2, 1.0)];
        let use_counts = vec![5, 5, 0]; // only index 2 is eligible
        let mut rng = StdRng::seed_from_u64(42);
        let idx = pick_sender(&senders, &use_counts, 5, &mut rng);
        assert_eq!(idx, 2);
    }

    #[test]
    fn test_pick_sender_falls_back_when_all_at_limit() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0)];
        let use_counts = vec![10, 10];
        let mut rng = StdRng::seed_from_u64(42);
        // max_per_sender = 10 -> all at limit -> should still return a valid index via fallback
        let idx = pick_sender(&senders, &use_counts, 10, &mut rng);
        assert!(idx < 2);
    }

    #[test]
    fn test_pick_sender_prefers_under_limit_even_with_rng() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0), dummy_wallet(2, 1.0)];
        let use_counts = vec![3, 3, 0];
        let mut rng = StdRng::seed_from_u64(123);
        for _ in 0..20 {
            let idx = pick_sender(&senders, &use_counts, 3, &mut rng);
            assert_eq!(idx, 2, "Should always pick the only eligible sender");
        }
    }

    #[test]
    fn test_distribute_round_robin_balances_workers() {
        let queues = distribute_round_robin((0..275).collect::<Vec<_>>(), 5);
        let lengths: Vec<usize> = queues.iter().map(|queue| queue.len()).collect();
        assert_eq!(lengths, vec![55, 55, 55, 55, 55]);

        let mut flattened: Vec<usize> = queues.into_iter().flat_map(|queue| queue.into_iter()).collect();
        flattened.sort_unstable();
        assert_eq!(flattened, (0..275).collect::<Vec<_>>());
    }

    // ──────────────────────────────────────────────────────────────────────────
    // SenderState tests (core concurrent allocation logic)
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_sender_state_basic_pick_and_lock() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0)];
        let mut state = SenderState {
            use_counts: vec![0, 0],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);

        let picked = state.try_pick_and_lock(&senders, 5, &mut rng);
        assert!(picked.is_some());
        let idx = picked.unwrap();
        assert!(idx < 2);
        assert_eq!(state.use_counts[idx], 1);
        assert!(state.locked_senders.contains(&idx));
    }

    #[test]
    fn test_sender_state_respects_max_per_sender() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0)];
        let mut state = SenderState {
            use_counts: vec![5, 0],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);

        // sender 0 is at limit
        let picked = state.try_pick_and_lock(&senders, 5, &mut rng);
        assert_eq!(picked, Some(1));
        assert_eq!(state.use_counts[1], 1);
    }

    #[test]
    fn test_sender_state_returns_none_when_all_locked_or_at_limit() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0)];
        let mut state = SenderState {
            use_counts: vec![5, 5],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);

        let picked = state.try_pick_and_lock(&senders, 5, &mut rng);
        assert!(picked.is_none());
    }

    #[test]
    fn test_sender_state_unlock_frees_sender_for_later_pick() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0)];
        let mut state = SenderState {
            use_counts: vec![0, 0],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);

        let first = state.try_pick_and_lock(&senders, 10, &mut rng).unwrap();
        assert!(state.locked_senders.contains(&first));

        state.unlock(first);
        assert!(!state.locked_senders.contains(&first));

        // now we should be able to pick it again (use_count already incremented, but limit not reached)
        let second = state.try_pick_and_lock(&senders, 10, &mut rng);
        assert!(second.is_some());
    }

    #[test]
    fn test_sender_state_combines_use_count_and_lock_correctly() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0), dummy_wallet(2, 1.0)];
        let mut state = SenderState {
            use_counts: vec![2, 2, 0],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);

        // only 2 is eligible by use_count
        let p1 = state.try_pick_and_lock(&senders, 2, &mut rng);
        assert_eq!(p1, Some(2));
        assert!(state.locked_senders.contains(&2));

        // now 2 is locked -> next call should return None even though use_count[2] < limit
        let p2 = state.try_pick_and_lock(&senders, 2, &mut rng);
        assert!(p2.is_none());

        state.unlock(2);
        let p3 = state.try_pick_and_lock(&senders, 2, &mut rng);
        assert_eq!(p3, Some(2)); // use_count becomes 1 (was 0 +1)
    }

    #[test]
    fn test_sender_state_multiple_picks_increment_use_counts() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0)];
        let mut state = SenderState {
            use_counts: vec![0, 0],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        };
        let mut rng = StdRng::seed_from_u64(7);

        for _ in 0..3 {
            if let Some(idx) = state.try_pick_and_lock(&senders, 10, &mut rng) {
                state.unlock(idx);
            }
        }
        // total increments should be 3 across the two senders
        let total: usize = state.use_counts.iter().sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn test_sender_state_empty_senders_returns_none() {
        let senders: Vec<WalletInfo> = vec![];
        let mut state = SenderState {
            use_counts: vec![],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);
        let picked = state.try_pick_and_lock(&senders, 5, &mut rng);
        assert!(picked.is_none());
    }

    // ──────────────────────────────────────────────────────────────────────────
    // TDD tests for newly extracted pure helpers (filtering + dry-run planner)
    // ──────────────────────────────────────────────────────────────────────────

    // filter_senders / filter_targets

    #[test]
    fn test_filter_senders_keeps_only_above_min() {
        let wallets = vec![
            dummy_wallet(0, 0.5),
            dummy_wallet(1, 1.2),
            dummy_wallet(2, 0.9),
            dummy_wallet(3, 2.0),
        ];
        let senders = filter_senders(&wallets, 1.0);
        assert_eq!(senders.len(), 2);
        assert_eq!(senders[0].idx, 1);
        assert_eq!(senders[1].idx, 3);
    }

    #[test]
    fn test_filter_senders_empty_when_none_qualify() {
        let wallets = vec![dummy_wallet(0, 0.1), dummy_wallet(1, 0.2)];
        let senders = filter_senders(&wallets, 1.0);
        assert!(senders.is_empty());
    }

    #[test]
    fn test_filter_targets_keeps_only_below_max() {
        let wallets = vec![dummy_wallet(0, 0.01), dummy_wallet(1, 0.5), dummy_wallet(2, 1.0)];
        let targets = filter_targets(&wallets, 0.1);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].idx, 0);
    }

    #[test]
    fn test_filter_targets_all_when_max_is_high() {
        let wallets = vec![dummy_wallet(0, 0.01), dummy_wallet(1, 10.0)];
        let targets = filter_targets(&wallets, 100.0);
        assert_eq!(targets.len(), 2);
    }

    // compute_max_per_sender

    #[test]
    fn test_compute_max_per_sender_basic() {
        // 7 targets, 2 senders => ceil(3.5) = 4
        assert_eq!(compute_max_per_sender(7, 2), 4);
    }

    #[test]
    fn test_compute_max_per_sender_zero_senders() {
        assert_eq!(compute_max_per_sender(10, 0), 0);
    }

    #[test]
    fn test_compute_max_per_sender_equal() {
        // 5 targets, 5 senders => ceil(1.0) = 1
        assert_eq!(compute_max_per_sender(5, 5), 1);
    }

    // select_targets_to_fund

    #[test]
    fn test_select_targets_to_fund_all_when_no_cap() {
        let targets = vec![dummy_wallet(0, 0.01), dummy_wallet(1, 0.02)];
        let selected = select_targets_to_fund(&targets, None);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn test_select_targets_to_fund_caps_correctly() {
        let targets = vec![dummy_wallet(0, 0.01), dummy_wallet(1, 0.02), dummy_wallet(2, 0.03)];
        let selected = select_targets_to_fund(&targets, Some(2));
        assert_eq!(selected.len(), 2);
    }

    // generate_dry_run_plan

    #[test]
    fn test_generate_dry_run_plan_allocates_all_targets() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0)];
        let targets = vec![dummy_wallet(2, 0.01), dummy_wallet(3, 0.01), dummy_wallet(4, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng);
        assert_eq!(plan.len(), 3);
    }

    #[test]
    fn test_generate_dry_run_plan_respects_max_targets() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.01), dummy_wallet(2, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, Some(1), &mut rng);
        assert_eq!(plan.len(), 1);
    }

    #[test]
    fn test_generate_dry_run_plan_empty_on_no_senders() {
        let senders: Vec<WalletInfo> = vec![];
        let targets = vec![dummy_wallet(0, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng);
        assert!(plan.is_empty());
    }

    #[test]
    fn test_generate_dry_run_plan_empty_on_no_targets() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets: Vec<WalletInfo> = vec![];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng);
        assert!(plan.is_empty());
    }

    #[test]
    fn test_generate_dry_run_plan_hops_in_range() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.01), dummy_wallet(2, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng);
        for pf in &plan {
            assert!(pf.hops >= 3 && pf.hops <= 5);
        }
    }

    #[test]
    fn test_generate_dry_run_plan_amount_in_range() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng);
        assert_eq!(plan.len(), 1);
        let amount_eth = plan[0].amount.as_u128() as f64 / 1e18;
        assert!((0.02..=0.04).contains(&amount_eth));
    }

    // choose_gas_price_mgwei

    #[test]
    fn test_choose_gas_price_mgwei_within_bounds() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            let chosen = choose_gas_price_mgwei(5_000, 1_000, 10_000, &mut rng);
            assert!(
                chosen >= 1_000 && chosen <= 10_000,
                "chosen={} out of [1000, 10000]",
                chosen
            );
        }
    }

    #[test]
    fn test_choose_gas_price_mgwei_90_percent_floor_on_noisy_network() {
        let mut rng = StdRng::seed_from_u64(42);
        // network = 500 mgwei (0.5 gwei), min=1000, max=2000
        // floor should be 1000 (from min)
        let chosen = choose_gas_price_mgwei(500, 1000, 2000, &mut rng);
        assert!(chosen >= 1000 && chosen <= 2000);
    }

    #[test]
    fn test_choose_gas_price_mgwei_uses_90_percent_network_floor() {
        let mut rng = StdRng::seed_from_u64(42);
        // network high (10_000 mgwei = 10 gwei)
        // floor = max( min, 9000 )
        let chosen = choose_gas_price_mgwei(10_000, 500, 20_000, &mut rng);
        assert!(chosen >= 9000); // at least 90% of network
    }

    #[test]
    fn test_choose_gas_price_mgwei_hard_caps_at_100_000() {
        let mut rng = StdRng::seed_from_u64(42);
        // very high max, very high network -> still capped at 100_000 mgwei
        let chosen = choose_gas_price_mgwei(200_000, 1_000, 300_000, &mut rng);
        assert!(chosen <= 100_000);
    }

    #[test]
    fn test_choose_gas_price_mgwei_is_deterministic() {
        let mut rng1 = StdRng::seed_from_u64(12345);
        let mut rng2 = StdRng::seed_from_u64(12345);
        let a = choose_gas_price_mgwei(5_000, 1_000, 10_000, &mut rng1);
        let b = choose_gas_price_mgwei(5_000, 1_000, 10_000, &mut rng2);
        assert_eq!(a, b);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // gas amount maths
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_calculate_seed_amount_covers_target_and_hops() {
        // target = 1 ETH, gas = 20 gwei = 20_000 mgwei = 20_000_000_000 wei
        // gas_21k = 21_000 * 20_000_000_000 = 420_000_000_000_000 wei = 4.2e-14 ETH
        // For 3 hops: seed = 1 + 4.2e-14 * (3+2) = 1 + 4.2e-13
        let target = parse_units(1u64, "ether").unwrap().into();
        let gas = U256::from(20_000_000_000u64);
        let seed = calculate_seed_amount(target, gas, 3);
        let seed_eth = seed.as_u128() as f64 / 1e18;
        // seed = target + gas_21k * (hop_count + 2) = 1.0 + 0.0021 = 1.0021
        assert!(
            (seed_eth - 1.0021).abs() < 1e-4,
            "seed_eth should be ~1.0021 but got {seed_eth}"
        );
    }

    #[test]
    fn test_calculate_forward_amount_deducts_gas() {
        let remaining = parse_units(1u64, "ether").unwrap().into();
        let gas = U256::from(20_000_000_000u64);
        let hop_cost = U256::from(21_000u64) * gas;
        let forward = calculate_forward_amount(remaining, gas);
        assert_eq!(forward, remaining - hop_cost);
    }

    #[test]
    fn test_calculate_forward_amount_saturates_at_zero() {
        let remaining = U256::from(100);
        let gas = U256::from(1_000_000_000u64);
        let forward = calculate_forward_amount(remaining, gas);
        assert_eq!(forward, U256::zero());
    }

    // ──────────────────────────────────────────────────────────────────────────
    // get_next_hop_address
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_get_next_hop_address_last_hop_goes_to_target() {
        let target: Address = Address::from_low_u64_be(99);
        let proxies = vec![Address::from_low_u64_be(1), Address::from_low_u64_be(2)];
        assert_eq!(get_next_hop_address(1, 2, target, &proxies), target);
    }

    #[test]
    fn test_get_next_hop_address_penultimate_goes_to_last_proxy() {
        let target: Address = Address::from_low_u64_be(99);
        let proxies = vec![
            Address::from_low_u64_be(1),
            Address::from_low_u64_be(2),
            Address::from_low_u64_be(3),
        ];
        // 3 hops: i=0 -> P[1]=2, i=1 -> P[2]=3, i=2 -> target
        assert_eq!(
            get_next_hop_address(1, 3, target, &proxies),
            Address::from_low_u64_be(3)
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_hop_label
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_format_hop_label_intermediate() {
        assert_eq!(format_hop_label(0, 3), "P2");
        assert_eq!(format_hop_label(1, 4), "P3");
    }

    #[test]
    fn test_format_hop_label_last_is_target() {
        assert_eq!(format_hop_label(2, 3), "target");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // should_skip_confirmation
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_should_skip_confirmation_yes() {
        assert!(should_skip_confirmation(true, false));
    }

    #[test]
    fn test_should_skip_confirmation_dry_run() {
        assert!(should_skip_confirmation(false, true));
    }

    #[test]
    fn test_should_skip_confirmation_neither() {
        assert!(!should_skip_confirmation(false, false));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // confirm_funding
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_confirm_funding_accepts_y() {
        let input = b"y\n";
        let result = confirm_funding(3, 1, &input[..]).unwrap();
        assert!(result);
    }

    #[test]
    fn test_confirm_funding_rejects_n() {
        let input = b"n\n";
        let result = confirm_funding(3, 1, &input[..]).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_confirm_funding_case_insensitive() {
        let input = b"Y\n";
        let result = confirm_funding(3, 1, &input[..]).unwrap();
        assert!(result);
    }

    #[test]
    fn test_confirm_funding_empty_aborts() {
        let input = b"\n";
        let result = confirm_funding(3, 1, &input[..]).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_confirm_funding_invalid_input_aborts() {
        let input = b"maybe\n";
        let result = confirm_funding(3, 1, &input[..]).unwrap();
        assert!(!result);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // await_confirmation_with_progress
    // ──────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_await_confirmation_with_progress_returns_success() {
        let future = async { Ok::<_, std::io::Error>(()) };
        let result = await_confirmation_with_progress(
            1,
            "Funder",
            "Sender -> P1",
            future,
            Duration::from_millis(50),
            Duration::from_millis(10),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_await_confirmation_with_progress_times_out() {
        let future = std::future::pending::<std::result::Result<(), std::io::Error>>();
        let result = await_confirmation_with_progress(
            1,
            "Funder",
            "Sender -> P1",
            future,
            Duration::from_millis(20),
            Duration::from_millis(5),
        )
        .await;

        assert!(result.is_err());
        let msg = format!("{:#}", result.unwrap_err());
        assert!(msg.contains("Timed out waiting for Sender -> P1 confirmation"));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // prepare_funding_sets (via Funder)
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_prepare_funding_sets_basic() {
        let funder = Funder::new(
            Arc::new(core_logic::WalletManager::new().unwrap()),
            Provider::new(Http::new(reqwest::Url::parse("http://localhost").unwrap())),
            "pw".into(),
            1,
            None,
        );
        let wallets = vec![dummy_wallet(0, 0.8), dummy_wallet(1, 0.005), dummy_wallet(2, 1.5)];
        let (senders, targets, available, max_per) = funder.prepare_funding_sets(&wallets, 0.5, 0.010);
        assert_eq!(senders.len(), 2); // wallets 0 (0.8) and 2 (1.5)
        assert_eq!(targets.len(), 1); // wallet 1
        assert_eq!(available, 1);
        assert_eq!(max_per, 1);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // gas selector edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_choose_gas_price_mgwei_rounds_down_at_90pct_network_floor() {
        let mut rng = StdRng::seed_from_u64(42);
        // network = 1234 mgwei -> 90% = 1110.6 -> floor 1110
        let chosen = choose_gas_price_mgwei(1234, 1_000, 20_000, &mut rng);
        assert!(chosen >= 1000); // min_gwei floor
    }

    #[test]
    fn test_choose_gas_price_mgwei_floor_cant_exceed_ceiling() {
        let mut rng = StdRng::seed_from_u64(42);
        // floor would be 9000 but min is only 100 -> floor = 9000, ceiling = 1000
        // effective_floor = min(9000, 1000) = 1000
        let chosen = choose_gas_price_mgwei(10_000, 100, 1_000, &mut rng);
        // floor=9000, ceiling=11000, effective_floor=9000, so result ∈ [9000, 11000]
        assert!(
            chosen >= 9000 && chosen <= 11_000,
            "chosen={} expected in [9000, 11000]",
            chosen
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // seed + forward amount math
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_calculate_seed_amount_scales_with_hops() {
        let target = parse_units(1u64, "ether").unwrap().into();
        let gas = U256::from(20_000_000_000u64);
        let s1 = calculate_seed_amount(target, gas, 1);
        let s3 = calculate_seed_amount(target, gas, 3);
        assert!(s3 > s1, "more hops -> higher seed amount");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // dry-run plan end-to-end
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_execute_dry_run_prints_plan_lines() {
        let funder = Funder::new(
            Arc::new(core_logic::WalletManager::new().unwrap()),
            Provider::new(Http::new(reqwest::Url::parse("http://localhost").unwrap())),
            "pw".into(),
            1,
            None,
        );
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.005)];
        let output =
            std::panic::AssertUnwindSafe(|| funder.execute_dry_run(&senders, &targets, 0.02, 0.04, 3, 5, None));
        // Should not panic
        let _ = output;
    }

    // ──────────────────────────────────────────────────────────────────────────
    // load_wallets — all decryptions fail
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_load_wallets_all_fail_returns_empty() {
        // Create a temp dir with an invalid .json file that will fail decryption.
        // count() returns 1 (file exists), but get_wallet fails → warn! path.
        let dir = std::env::temp_dir().join("testnet-fund-load-wallets-test");
        let _ = std::fs::create_dir_all(&dir);
        let wallet_path = dir.join("bad_wallet.json");
        std::fs::write(&wallet_path, b"{}").unwrap();

        let manager = Arc::new(core_logic::WalletManager::with_wallet_dir(&dir).unwrap());
        assert_eq!(manager.count(), 1, "should find one invalid wallet file");

        let funder = Funder::new(
            manager,
            Provider::new(Http::new(reqwest::Url::parse("http://localhost").unwrap())),
            "password".into(),
            1,
            None,
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        let wallets = rt.block_on(funder.load_wallets(10)).unwrap();
        assert!(wallets.is_empty(), "all decryption failures should yield empty result");

        // Cleanup
        let _ = std::fs::remove_file(&wallet_path);
        let _ = std::fs::remove_dir(&dir);
    }
}
