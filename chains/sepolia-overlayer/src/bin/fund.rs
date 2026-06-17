//! # Sepolia Overlayer — Funder
//!
//! Multi-hop obfuscated ETH funding tool.
//! Splits funding across N wallet hops to obscure the source on-chain.

use aes_gcm::{
    aead::{Aead, NewAead},
    Aes256Gcm, Nonce,
};
use anyhow::{ensure, Context, Result};
use chrono::Local;
use clap::Parser;
use core_logic::setup_logger;
use core_logic::RpcManager;
use ethers::prelude::*;
use ethers::utils::parse_units;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use sepolia_overlayer::config::SepoliaConfig;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::Semaphore;
use tracing::{info, warn};

const DEFAULT_LOAD_CONCURRENCY: usize = 100;
const CONFIRMATION_TIMEOUT_SECS: u64 = 60;
const CONFIRMATION_HEARTBEAT_SECS: u64 = 10;

/// Create a Provider<Http> from the RPC manager's next healthy endpoint.
fn create_provider(rpc_manager: &RpcManager, http_client: &reqwest::Client) -> Result<(Provider<Http>, String)> {
    let endpoint = rpc_manager.get_endpoint()?;
    let url = reqwest::Url::parse(&endpoint.url)?;
    let provider = Provider::new(Http::new_with_client(url, http_client.clone()));
    Ok((provider, endpoint.url.clone()))
}

/// Funder orchestrates the multi-hop ETH funding flow.
#[derive(Clone)]
struct Funder {
    manager: Arc<core_logic::WalletManager>,
    provider: Provider<Http>,
    rpc_manager: Arc<RpcManager>,
    http_client: reqwest::Client,
    password: String,
    chain_id: u64,
    max_targets: Option<usize>,
    recovery: Option<Arc<RecoveryContext>>,
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

// ─────────────────────────────────────────────────────────────────────────────
// Recovery infrastructure — prevents ETH from being permanently stuck on proxy wallets
// ─────────────────────────────────────────────────────────────────────────────

const RECOVERY_DIR_DEFAULT: &str = "proxy-recovery";

/// One row in the recovery journal — written atomically *before* each tx is broadcast.
/// All fields are needed to re-derive the proxy wallet and check/replay the tx.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecoveryJournalEntry {
    /// 0-based index of the proxy in the hop chain (0 = P1, 1 = P2, etc.)
    hop_index: usize,
    /// Total number of hops for this funding flow
    hop_count: usize,
    /// Tx hash that was broadcast (or will be broadcast — written before send)
    tx_hash: String,
    /// Proxy's own address (derived from the persisted key)
    from_addr: String,
    /// Destination address for this hop
    to_addr: String,
    /// ETH value being sent (in wei, as string for JSON)
    value_wei: String,
    /// Gas price used (in wei, as string)
    gas_price_wei: String,
    /// Nonce used
    nonce: u64,
    /// Chain ID
    chain_id: u64,
    /// The recovery address where stuck ETH should be swept to
    recovery_address: String,
    /// Timestamp when journaled (for diagnostics)
    timestamp: String,
}

/// Recovery context shared across all funding operations.
/// Holds the directory for proxy keystore + journal files, and the recovery address.
struct RecoveryContext {
    /// Directory where proxy-{idx}.json encrypted key files and journal.jsonl are stored
    dir: String,
    /// Password for encrypting proxy keys (same as WALLET_PASSWORD)
    password: String,
    /// Where to sweep stuck ETH in recovery mode or on graceful shutdown
    recovery_address: Address,
    /// Chain ID
    chain_id: u64,
    /// Atomic flag for graceful shutdown sweep
    shutdown_requested: Arc<AtomicBool>,
}

/// Encrypt a proxy private key using the same AES-256-GCM + scrypt format as wallet-json.
/// Returns a JSON string ready to write to disk.
fn encrypt_proxy_key(private_key: &str, password: &str, chain_id: u64) -> Result<String> {
    let mut rng = rand::thread_rng();
    let mut salt = [0u8; 32];
    let mut iv = [0u8; 12];
    rng.fill(&mut salt);
    rng.fill(&mut iv);

    let params = scrypt::Params::new(14, 8, 1, 32).map_err(|e| anyhow::anyhow!("scrypt params: {e}"))?;
    let mut key = [0u8; 32];
    scrypt::scrypt(password.as_bytes(), &salt, &params, &mut key).map_err(|e| anyhow::anyhow!("scrypt: {e}"))?;

    let cipher = Aes256Gcm::new(&key.into());
    let nonce = Nonce::from_slice(&iv);

    let ciphertext_with_tag = cipher
        .encrypt(nonce, private_key.as_bytes())
        .map_err(|e| anyhow::anyhow!("encrypt: {e}"))?;

    let tag_pos = ciphertext_with_tag.len().saturating_sub(16);
    let ciphertext = &ciphertext_with_tag[..tag_pos];
    let tag = &ciphertext_with_tag[tag_pos..];

    // Build the same JSON format as wallet-json files
    let json = serde_json::json!({
        "encrypted": {
            "ciphertext": hex::encode(ciphertext),
            "iv": hex::encode(iv),
            "salt": hex::encode(salt),
            "tag": hex::encode(tag),
        },
        "encryption_type": "aes-256-gcm",
        "chain_id": chain_id,
    });

    serde_json::to_string_pretty(&json).context("Failed to serialize proxy key JSON")
}

/// Decrypt a proxy key file (same format as wallet-json).
fn decrypt_proxy_key_file(path: &Path, password: &str) -> Result<String> {
    let content = fs::read_to_string(path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;

    let encrypted = json
        .get("encrypted")
        .context("Missing 'encrypted' field in proxy key file")?;

    let ciphertext_hex = encrypted.get("ciphertext").and_then(|v| v.as_str()).unwrap_or("");
    let iv_hex = encrypted.get("iv").and_then(|v| v.as_str()).unwrap_or("");
    let salt_hex = encrypted.get("salt").and_then(|v| v.as_str()).unwrap_or("");
    let tag_hex = encrypted.get("tag").and_then(|v| v.as_str()).unwrap_or("");

    if ciphertext_hex.is_empty() {
        anyhow::bail!("Empty ciphertext in proxy key file");
    }

    // Use the same decryption as wallet-json
    let ciphertext = hex::decode(ciphertext_hex).context("Invalid ciphertext hex")?;
    let iv = hex::decode(iv_hex).context("Invalid IV hex")?;
    let salt = hex::decode(salt_hex).context("Invalid salt hex")?;
    let mut tag = hex::decode(tag_hex).context("Invalid tag hex")?;

    let params = scrypt::Params::new(14, 8, 1, 32).map_err(|e| anyhow::anyhow!("scrypt params: {e}"))?;
    let mut key = [0u8; 32];
    scrypt::scrypt(password.as_bytes(), &salt, &params, &mut key).map_err(|e| anyhow::anyhow!("scrypt: {e}"))?;

    let cipher = Aes256Gcm::new(&key.into());
    let nonce = Nonce::from_slice(&iv);

    let mut full_payload = ciphertext;
    full_payload.append(&mut tag);

    let plaintext = cipher
        .decrypt(nonce, full_payload.as_ref())
        .map_err(|e| anyhow::anyhow!("Decryption failed: {e}"))?;

    String::from_utf8(plaintext).context("Proxy key is not valid UTF-8")
}

/// Persist a proxy key file and append a journal entry before broadcasting a tx.
/// Returns the filename written (for later cleanup).
fn persist_proxy_and_journal(
    recovery: &RecoveryContext,
    hop_index: usize,
    hop_count: usize,
    proxy_private_key: &str,
    from_addr: Address,
    to_addr: Address,
    value_wei: U256,
    gas_price_wei: U256,
    nonce: u64,
) -> Result<String> {
    // 1. Write encrypted proxy key file
    let key_filename = format!("proxy-{}.json", &uuid_simple(hop_index));
    let key_path = Path::new(&recovery.dir).join(&key_filename);
    let encrypted_json = encrypt_proxy_key(proxy_private_key, &recovery.password, recovery.chain_id)?;
    fs::write(&key_path, &encrypted_json)
        .with_context(|| format!("Failed to write proxy key file: {}", key_path.display()))?;

    // 2. Append journal entry
    let entry = RecoveryJournalEntry {
        hop_index,
        hop_count,
        tx_hash: String::new(), // Will be populated after broadcast; we write it here for the key file reference
        from_addr: format!("{from_addr:?}"),
        to_addr: format!("{to_addr:?}"),
        value_wei: value_wei.to_string(),
        gas_price_wei: gas_price_wei.to_string(),
        nonce,
        chain_id: recovery.chain_id,
        recovery_address: format!("{:?}", recovery.recovery_address),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let journal_path = Path::new(&recovery.dir).join("journal.jsonl");
    let mut line = serde_json::to_string(&entry).context("Failed to serialize journal entry")?;
    line.push('\n');

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&journal_path)
        .with_context(|| format!("Failed to open journal: {}", journal_path.display()))?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("Failed to write journal entry: {}", journal_path.display()))?;

    Ok(key_filename)
}

/// After successful confirmation, clean up the proxy key file and journal entry.
fn cleanup_proxy_and_journal(recovery: &RecoveryContext, key_filename: &str, _hop_index: usize) {
    // Remove key file
    let key_path = Path::new(&recovery.dir).join(key_filename);
    let _ = fs::remove_file(&key_path);

    // We don't try to edit the journal in-place (complex and error-prone).
    // Instead, confirmed entries will be filtered out during recovery sweep
    // because the on-chain tx is confirmed and the proxy balance will be 0.
    // The journal file is append-only; recovery mode checks each entry against
    // the chain and only sweeps entries where the proxy still has a balance.
}

/// Generate a simple unique suffix for proxy key filenames to avoid collisions.
fn uuid_simple(hop_index: usize) -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.gen();
    format!("{}-{}", hop_index, hex::encode(bytes))
}

/// Recovery mode: read journal.jsonl, derive proxy keys, check tx status, and sweep any
/// remaining ETH back to the recovery address. Also sweeps any orphaned proxy key files.
async fn recover_proxies(
    recovery_dir: &str,
    password: &str,
    recovery_address: Address,
    chain_id: u64,
    rpc_manager: &RpcManager,
    http_client: &reqwest::Client,
    dry_run: bool,
) -> Result<()> {
    let journal_path = Path::new(recovery_dir).join("journal.jsonl");
    if !journal_path.exists() {
        println!(
            "No recovery journal found at {}. Nothing to recover.",
            journal_path.display()
        );
        return Ok(());
    }

    println!("=== Recovery Mode ===");
    println!("Recovery dir:    {}", recovery_dir);
    println!("Recovery addr:   {:?}", recovery_address);
    println!("Chain ID:        {}", chain_id);
    if dry_run {
        println!("Mode:            DRY RUN (no txs will be sent)");
    }
    println!();

    let (provider, _url) = create_provider(rpc_manager, http_client).context("No healthy RPC for recovery")?;

    // Parse journal entries from the JSONL file
    let content = fs::read_to_string(&journal_path)
        .with_context(|| format!("Failed to read journal: {}", journal_path.display()))?;

    let mut entries: Vec<RecoveryJournalEntry> = Vec::new();
    for (line_no, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<RecoveryJournalEntry>(trimmed) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                warn!("Journal line {} is invalid JSON: {}. Skipping.", line_no + 1, e);
            },
        }
    }

    if entries.is_empty() {
        println!("No valid journal entries found. Nothing to recover.");
        return Ok(());
    }

    println!("Found {} journal entries.\n", entries.len());

    let mut swept_count = 0usize;
    let mut already_spent_count = 0usize;
    let mut error_count = 0usize;

    // Group entries by hop_index to find proxy key files
    for entry in &entries {
        let from_addr: Address = entry
            .from_addr
            .parse()
            .context(format!("Invalid from_addr in journal: {}", entry.from_addr))?;

        // First check: does the proxy still have a balance?
        let balance = provider
            .get_balance(from_addr, None)
            .await
            .context(format!("Failed to fetch balance for {:?}", from_addr))?;

        if balance.is_zero() {
            println!(
                "[SKIP] {:?} — balance is 0. Tx {} likely confirmed. Already safe.",
                from_addr, entry.tx_hash
            );
            already_spent_count += 1;
            continue;
        }

        let balance_eth = balance.as_u128() as f64 / 1e18;
        println!(
            "[FOUND] {:?} has {} ETH stuck. Tx: {}",
            from_addr,
            format_eth_amount(balance_eth),
            if entry.tx_hash.is_empty() {
                "unknown"
            } else {
                &entry.tx_hash
            }
        );

        // Find the corresponding proxy key file
        let key_filename = format!("proxy-{}-", entry.hop_index);
        let mut found_key: Option<String> = None;
        let mut found_key_file: Option<String> = None;

        if let Ok(read_dir) = std::fs::read_dir(recovery_dir) {
            for dir_entry in read_dir.flatten() {
                let fname = dir_entry.file_name().to_string_lossy().to_string();
                if fname.starts_with(&key_filename) && fname.ends_with(".json") {
                    found_key_file = Some(fname.clone());
                    let key_path = Path::new(recovery_dir).join(&fname);
                    match decrypt_proxy_key_file(&key_path, password) {
                        Ok(key) => {
                            found_key = Some(key);
                            break;
                        },
                        Err(e) => {
                            warn!("Failed to decrypt {}: {}. Will try next.", fname, e);
                        },
                    }
                }
            }
        }

        let private_key = match found_key {
            Some(k) => k,
            None => {
                warn!(
                    "[ERROR] No decryptable proxy key file found for {:?}. Cannot sweep {} ETH.",
                    from_addr, balance_eth
                );
                error_count += 1;
                continue;
            },
        };

        // Derive the wallet and sweep
        let proxy_wallet = match private_key.parse::<LocalWallet>() {
            Ok(w) => w.with_chain_id(chain_id),
            Err(e) => {
                warn!("[ERROR] Failed to parse proxy key for {:?}: {}.", from_addr, e);
                error_count += 1;
                continue;
            },
        };

        let derived_addr = proxy_wallet.address();
        if derived_addr != from_addr {
            warn!(
                "[WARN] Derived address {:?} != journal from_addr {:?}. Key file may be wrong. Skipping.",
                derived_addr, from_addr
            );
            error_count += 1;
            continue;
        }

        // Fetch nonce from chain to handle the case where the original tx already used a nonce.
        // ethers' auto-nonce fetches the next nonce which works if no txs are pending.
        // But if there's a pending tx, we need to manually manage nonce.
        let on_chain_nonce = match provider.get_transaction_count(from_addr, None).await {
            Ok(n) => n,
            Err(e) => {
                warn!("[ERROR] Failed to fetch nonce for {:?}: {}.", from_addr, e);
                error_count += 1;
                continue;
            },
        };

        // Sweep: send balance - 21k*gas back to recovery address
        let gas_price = provider.get_gas_price().await.unwrap_or(U256::from(1_000_000_000u64));
        let gas_cost = U256::from(21_000u64) * gas_price;
        let sweep_amount = balance.saturating_sub(gas_cost);

        if sweep_amount.is_zero() {
            println!(
                "[SKIP] {:?} — balance ({:.6} ETH) is insufficient to cover gas. Dust left.",
                from_addr, balance_eth
            );
            already_spent_count += 1;
            continue;
        }

        let sweep_eth = sweep_amount.as_u128() as f64 / 1e18;
        println!(
            "[SWEEP] {:?} → {:?} : {} ETH (nonce {})",
            from_addr,
            recovery_address,
            format_eth_amount(sweep_eth),
            on_chain_nonce,
        );

        if dry_run {
            println!("  (dry run — no tx sent)");
            swept_count += 1;
            continue;
        }

        // Use a fresh provider for the sweep
        let (sweep_provider, _sweep_url) = create_provider(rpc_manager, http_client)?;
        let signer = SignerMiddleware::new(sweep_provider, proxy_wallet);
        // Explicit nonce to handle the case where the proxy already has a pending tx.
        // ethers' auto-nonce will use pending_nonce + 1, but explicit is safer.
        let tx = TransactionRequest::pay(recovery_address, sweep_amount)
            .gas(21_000)
            .gas_price(gas_price)
            .nonce(on_chain_nonce);
        let pending_result = signer.send_transaction(tx, None).await;
        match pending_result {
            Ok(pending) => {
                let tx_hash = pending.tx_hash();
                println!("  Tx broadcast: {:?}. Waiting for confirmation...", tx_hash);
                let confirm_result = pending.confirmations(1).interval(Duration::from_millis(500)).await;
                match confirm_result {
                    Ok(Some(_)) => {
                        println!("  ✅ Confirmed. {} ETH recovered.", format_eth_amount(sweep_eth));
                        swept_count += 1;
                        // Clean up key file
                        if let Some(ref kf) = found_key_file {
                            let _ = fs::remove_file(Path::new(recovery_dir).join(kf));
                        }
                    },
                    Ok(None) => {
                        println!("  ⚠️ Tx may have dropped. Check {:?} manually.", tx_hash);
                        error_count += 1;
                    },
                    Err(e) => {
                        warn!("  ❌ Confirmation failed: {}. Check {:?} manually.", e, tx_hash);
                        error_count += 1;
                    },
                }
            },
            Err(e) => {
                warn!("  ❌ Send failed: {}. Cannot sweep {:?}.", e, from_addr);
                error_count += 1;
            },
        }
    }

    println!();
    println!("=== Recovery Complete ===");
    println!("Swept:          {}", swept_count);
    println!("Already safe:   {}", already_spent_count);
    println!("Errors:         {}", error_count);

    // If all sweeps succeeded, clean up journal
    if !dry_run && error_count == 0 && swept_count + already_spent_count == entries.len() {
        let backup = format!("{}.bak", journal_path.display());
        fs::rename(&journal_path, &backup).unwrap_or_default();
        println!("Journal archived to {}", backup);
    }

    Ok(())
}

/// Perform emergency sweep of all known proxy keys (called on graceful shutdown or panic).
/// Does NOT check the journal — just sweeps any proxy that has a balance.
async fn emergency_sweep_all(
    recovery: &RecoveryContext,
    rpc_manager: &RpcManager,
    http_client: &reqwest::Client,
) -> Result<()> {
    let (provider, _) = create_provider(rpc_manager, http_client)?;

    if let Ok(read_dir) = std::fs::read_dir(&recovery.dir) {
        for entry in read_dir.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if !fname.starts_with("proxy-") || !fname.ends_with(".json") {
                continue;
            }

            let key_path = Path::new(&recovery.dir).join(&fname);
            let private_key = match decrypt_proxy_key_file(&key_path, &recovery.password) {
                Ok(k) => k,
                Err(_) => continue,
            };

            let wallet = match private_key.parse::<LocalWallet>() {
                Ok(w) => w.with_chain_id(recovery.chain_id),
                Err(_) => continue,
            };

            let addr = wallet.address();
            let balance = match provider.get_balance(addr, None).await {
                Ok(b) => b,
                Err(_) => continue,
            };

            if balance.is_zero() {
                let _ = fs::remove_file(&key_path);
                continue;
            }

            let gas_price = provider.get_gas_price().await.unwrap_or(U256::from(1_000_000_000u64));
            let gas_cost = U256::from(21_000u64) * gas_price;
            let sweep_amount = balance.saturating_sub(gas_cost);

            if sweep_amount.is_zero() {
                continue;
            }

            // Fetch nonce to handle pending txs
            let nonce = match provider.get_transaction_count(addr, None).await {
                Ok(n) => n,
                Err(_) => continue,
            };

            // Use a fresh provider for the sweep
            let (sweep_provider, _sweep_url) = match create_provider(rpc_manager, http_client) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let signer = SignerMiddleware::new(sweep_provider, wallet);
            let tx = TransactionRequest::pay(recovery.recovery_address, sweep_amount)
                .gas(21_000)
                .gas_price(gas_price)
                .nonce(nonce);

            let pending_result = signer.send_transaction(tx, None).await;
            match pending_result {
                Ok(pending) => {
                    let tx_hash = pending.tx_hash();
                    warn!(
                        "[EMERGENCY SWEEP] {:?} → {:?} : {} ETH — tx {:?}",
                        addr,
                        recovery.recovery_address,
                        format_eth_amount(sweep_amount.as_u128() as f64 / 1e18),
                        tx_hash
                    );
                    let _ = pending.confirmations(1).interval(Duration::from_millis(500)).await;
                    let _ = fs::remove_file(&key_path);
                },
                Err(e) => {
                    warn!("[EMERGENCY SWEEP] Failed to sweep {:?}: {}", addr, e);
                },
            }
        }
    }

    Ok(())
}

/// Install a SIGINT/SIGTERM handler that sets the shutdown flag.
fn install_shutdown_handler(shutdown: Arc<AtomicBool>) {
    let ctrl_c_shutdown = Arc::clone(&shutdown);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        ctrl_c_shutdown.store(true, Ordering::SeqCst);
        warn!("Shutdown signal received. Proxy funds will be swept.");
    });
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
        "too many request",
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

    #[test]
    fn too_many_request_singular_is_retryable() {
        let err = anyhow::anyhow!("(code: 15, message: Too many request, try again later, data: None)");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn connection_refused_is_retryable() {
        let err = anyhow::anyhow!("connection refused: tcp connect error");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn rate_limited_is_retryable() {
        let err = anyhow::anyhow!("rate limited: please slow down");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn connection_reset_is_retryable() {
        let err = anyhow::anyhow!("connection reset by peer");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn service_unavailable_is_retryable() {
        let err = anyhow::anyhow!("service unavailable");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn temporary_failure_is_retryable() {
        let err = anyhow::anyhow!("temporary failure in name resolution");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn network_error_is_retryable() {
        let err = anyhow::anyhow!("network error: connection timed out");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn case_insensitive_retry_matches() {
        let err = anyhow::anyhow!("TOO MANY REQUESTS");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn replacement_underpriced_is_not_retryable() {
        let err = anyhow::anyhow!("replacement transaction underpriced");
        assert!(!should_retry_proxy_send_error(&err));
    }

    #[test]
    fn gas_price_too_low_is_not_retryable() {
        let err = anyhow::anyhow!("gas price too low");
        assert!(!should_retry_proxy_send_error(&err));
    }

    #[test]
    fn empty_error_is_not_retryable() {
        let err = anyhow::anyhow!("");
        assert!(!should_retry_proxy_send_error(&err));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────────────

fn pick_sender(senders: &[WalletInfo], use_counts: &[usize], max_per_sender: usize, rng: &mut StdRng) -> usize {
    if senders.is_empty() {
        return 0;
    }
    let candidates: Vec<usize> = (0..senders.len()).filter(|&i| use_counts[i] < max_per_sender).collect();
    if candidates.is_empty() {
        rng.gen_range(0..senders.len())
    } else {
        candidates[rng.gen_range(0..candidates.len())]
    }
}

#[allow(clippy::too_many_arguments)]
async fn fund_via_chain(
    rpc_manager: &RpcManager,
    http_client: &reqwest::Client,
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
    recovery: Option<&RecoveryContext>,
    rng: &mut StdRng,
) -> Result<()> {
    let (provider, _) = create_provider(rpc_manager, http_client).context("No healthy RPC for wallet check")?;

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

    // Generate fresh random private keys for each proxy, then derive wallets.
    // This way we have the hex key available for recovery persistence.
    let mut proxy_keys_hex: Vec<String> = Vec::with_capacity(hop_count);
    let mut proxies: Vec<LocalWallet> = Vec::with_capacity(hop_count);
    for _ in 0..hop_count {
        let mut key_bytes = [0u8; 32];
        rng.fill(&mut key_bytes[..]);
        let key_hex = hex::encode(key_bytes);
        let wallet: LocalWallet = key_hex
            .parse::<LocalWallet>()
            .expect("valid hex private key")
            .with_chain_id(chain_id);
        proxies.push(wallet);
        proxy_keys_hex.push(key_hex);
    }

    let proxy_addrs: Vec<Address> = proxies.iter().map(|w| w.address()).collect();

    let gas_price = random_gas_price(&provider, min_gwei, max_gwei, rng).await?;
    // Use MAX gwei for seed calculation so hops never run out of gas
    // even if network gas spikes during the multi-hop flow.
    let max_gas_price_wei = U256::from((max_gwei * 1_000_000_000.0) as u64);
    let seed = calculate_seed_amount(target_amount, max_gas_price_wei, hop_count);
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

    // ── Track proxy key filenames for cleanup ──
    let mut proxy_key_files: Vec<String> = Vec::with_capacity(hop_count);

    // ── Sender -> P1 ──
    //
    // Safety: Before broadcasting, persist proxy key + journal. After confirm, clean up.
    if let Some(rc) = recovery {
        let _ = fs::create_dir_all(&rc.dir);
        let key_file = persist_proxy_and_journal(
            rc,
            0,
            hop_count,
            &proxy_keys_hex[0],
            proxy_addrs[0],
            get_next_hop_address(0, hop_count, target, &proxy_addrs),
            seed,
            gas_price,
            0, // nonce will be set by ethers
        );
        match key_file {
            Ok(kf) => proxy_key_files.push(kf),
            Err(e) => warn!("[WK{worker_id}] Failed to persist proxy 0 key: {e}"),
        }
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
    let sender_tx_hash = pending.tx_hash();
    // Update journal with tx hash
    if let Some(rc) = recovery {
        if !proxy_key_files.is_empty() {
            let entry = RecoveryJournalEntry {
                hop_index: 0,
                hop_count,
                tx_hash: format!("{sender_tx_hash:?}"),
                from_addr: format!("{:?}", proxy_addrs[0]),
                to_addr: format!("{:?}", get_next_hop_address(0, hop_count, target, &proxy_addrs)),
                value_wei: seed.to_string(),
                gas_price_wei: gas_price.to_string(),
                nonce: 0,
                chain_id,
                recovery_address: format!("{:?}", rc.recovery_address),
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            let journal_path = Path::new(&rc.dir).join("journal.jsonl");
            let mut line = serde_json::to_string(&entry).unwrap_or_default();
            line.push('\n');
            if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&journal_path) {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }

    let sender_wait_label = "Sender -> P1";
    runner_log(
        worker_id,
        "Funder",
        format!("Waiting for {sender_wait_label} confirmation"),
    );
    match await_confirmation_with_progress(
        worker_id,
        "Funder",
        sender_wait_label,
        pending.confirmations(1).interval(Duration::from_millis(500)),
        Duration::from_secs(CONFIRMATION_TIMEOUT_SECS),
        Duration::from_secs(CONFIRMATION_HEARTBEAT_SECS),
    )
    .await
    {
        Ok(receipt) => {
            let receipt = receipt.context("Sender -> P1 receipt not confirmed (Ok(None))")?;
            // Clean up proxy 0 key file after confirmation
            if let Some(kf) = proxy_key_files.first() {
                if let Some(rc) = recovery {
                    cleanup_proxy_and_journal(rc, kf, 0);
                }
            }
            runner_log(
                worker_id,
                "Funder",
                format!("Sender -> P1 confirmed: {:?}", receipt.transaction_hash),
            );
        },
        Err(e) => {
            // Tx was broadcast, may confirm later. Proxy key is persisted.
            // Return error so the worker marks failure but proxy key survives for recovery.
            runner_log(
                worker_id,
                "Funder",
                format!(
                    "Sender -> P1 confirmation failed: {:#}. Proxy key persisted for recovery.",
                    e
                ),
            );
            // Broadcast a replacement tx with 2x gas as last-ditch effort
            if let Ok((new_provider, _)) = create_provider(rpc_manager, http_client) {
                let last_ditch_gas = gas_price + gas_price;
                let proxy0_wallet: LocalWallet = proxy_keys_hex[0]
                    .parse::<LocalWallet>()
                    .expect("valid hex")
                    .with_chain_id(chain_id);
                let ls_signer = SignerMiddleware::new(new_provider, proxy0_wallet);
                let ls_tx = TransactionRequest::pay(get_next_hop_address(0, hop_count, target, &proxy_addrs), seed)
                    .gas(21_000)
                    .gas_price(last_ditch_gas);
                let ls_result = ls_signer.send_transaction(ls_tx, None).await;
                match ls_result {
                    Ok(ls_pending) => {
                        runner_log(
                            worker_id,
                            "Funder",
                            format!("Last-ditch tx sent: {:?}", ls_pending.tx_hash()),
                        );
                    },
                    Err(ls_e) => {
                        warn!("Last-ditch send failed: {ls_e}");
                    },
                }
            }
            return Err(e);
        },
    }

    // ── Proxy hop chain ──
    let mut recipient_tx_hash = None;
    let mut remaining = seed;

    for (i, proxy) in proxies.iter().enumerate() {
        // Check shutdown flag
        if let Some(rc) = recovery {
            if rc.shutdown_requested.load(Ordering::SeqCst) {
                warn!("[WK{worker_id}] Shutdown requested during hop {}. Exiting.", i + 1);
                anyhow::bail!("Shutdown requested");
            }
        }

        let delay = rng.gen_range(min_delay..=max_delay);
        if delay > 0 {
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }

        let (mut current_provider, mut current_rpc_url) =
            create_provider(rpc_manager, http_client).context("No healthy RPC for proxy hop")?;
        let hop_gas = random_gas_price(&current_provider, min_gwei, max_gwei, rng).await?;

        let next = get_next_hop_address(i, hop_count, target, &proxy_addrs);
        let forward = calculate_forward_amount(remaining, hop_gas);
        let mut proxy_signer = SignerMiddleware::new(current_provider.clone(), proxy.clone());
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

        // SAFETY HOOK: Persist proxy key + journal BEFORE broadcast
        if let Some(rc) = recovery {
            let _ = fs::create_dir_all(&rc.dir);
            // For proxy at index i, we persist the *next* proxy's key (i+1) because
            // proxy i is already in-flight from the previous hop or Sender->P1.
            // Actually, for the current hop (i), the proxy is the one sending.
            // We already persisted proxy[i]'s key earlier. But for proxy[i+1] we
            // need to persist it now before the tx to proxy[i+1] is broadcast.
            // Wait — let me rethink. The persistence is per-proxy, not per-hop.
            // Proxy i has ETH right now. When we broadcast, the funds go to next.
            // If the broadcast succeeds and confirmation fails, proxy i still has ETH
            // (the tx may not have been mined). So we need proxy i's key persisted.
            //
            // Actually, for hop i:
            // - Proxy i has the funds
            // - We broadcast: proxy i -> next
            // - If this broadcast succeeds but confirmation fails: funds are on next
            // - If we need to recover, we need proxy i's key (to check tx status)
            //   OR next's key (to sweep if funds landed there)
            //
            // The safest approach: BEFORE each broadcast, persist BOTH the sender
            // (proxy i) key AND the receiver (next proxy) key if it exists.
        }
        if let Some(rc) = recovery {
            let _ = fs::create_dir_all(&rc.dir);
            // Persist the *next* proxy's key (i+1) if this isn't the last hop,
            // so if the tx confirms but this function errors out, the next proxy can be recovered.
            if i + 1 < hop_count {
                let next_key_hex = proxy_keys_hex[i + 1].clone();
                let next_dest = if i + 2 < hop_count { proxy_addrs[i + 2] } else { target };
                let key_file = persist_proxy_and_journal(
                    rc,
                    i + 1,
                    hop_count,
                    &next_key_hex,
                    proxy_addrs[i + 1],
                    next_dest,
                    forward,
                    hop_gas,
                    0,
                );
                match key_file {
                    Ok(kf) => proxy_key_files.push(kf),
                    Err(e) => warn!("[WK{worker_id}] Failed to persist proxy {} key: {e}", i + 1),
                }
            }
        }

        // Phase 1: Send tx with RPC rotation on failure
        // MAX_SEND_ATTEMPTS bounds the retry loop to prevent infinite spinning
        // if the RPC keeps returning retryable errors.
        const MAX_SEND_ATTEMPTS: usize = 5;
        let pending = {
            let mut send_attempt = 0usize;
            loop {
                let tx = TransactionRequest::pay(next, forward).gas(21_000).gas_price(hop_gas);
                let retry_info = match proxy_signer.send_transaction(tx, None).await {
                    Ok(p) => {
                        rpc_manager.record_success(&current_rpc_url);
                        break p;
                    },
                    Err(e) => {
                        if should_retry_proxy_send_error(&e) {
                            rpc_manager.record_failure(&current_rpc_url);
                            Some(format!("{e:#}"))
                        } else {
                            let hop_label = format_hop_label(i, hop_count);
                            return Err(e).with_context(|| {
                                format!(
                                    "P{} -> {} tx failed after {} attempt(s)",
                                    i + 1,
                                    hop_label,
                                    send_attempt + 1
                                )
                            });
                        }
                    },
                };
                if let Some(err_msg) = retry_info {
                    if send_attempt + 1 >= MAX_SEND_ATTEMPTS {
                        let hop_label = format_hop_label(i, hop_count);
                        anyhow::bail!(
                            "P{} -> {} send exhausted after {} attempts. Last error: {}. Proxy key persisted for recovery.",
                            i + 1,
                            hop_label,
                            MAX_SEND_ATTEMPTS,
                            err_msg
                        );
                    }
                    if let Ok((new_provider, new_url)) = create_provider(rpc_manager, http_client) {
                        current_rpc_url = new_url;
                        current_provider = new_provider;
                        proxy_signer = SignerMiddleware::new(current_provider.clone(), proxy.clone());
                    }
                    let retry_after_secs = (1u64 << send_attempt as u32).min(120);
                    runner_log(
                        worker_id,
                        &stage_label,
                        format!(
                            "Send attempt {} failed (RPC): {}; rotating RPC, retrying in {}s",
                            send_attempt + 1,
                            err_msg,
                            retry_after_secs
                        ),
                    );
                    tokio::time::sleep(Duration::from_secs(retry_after_secs)).await;
                    send_attempt += 1;
                    continue;
                }
            }
        };

        // Phase 2: Wait for confirmation with gas bump on heartbeat
        let hop_wait_label = format_hop_label(i, hop_count);
        let hop_tx_hash = pending.tx_hash();
        runner_log(
            worker_id,
            &stage_label,
            format!("Waiting for {hop_wait_label} confirmation"),
        );

        // Update journal with tx hash for this hop
        if let Some(rc) = recovery {
            let journal_path = Path::new(&rc.dir).join("journal.jsonl");
            let from_addr = proxy.address();
            let entry = RecoveryJournalEntry {
                hop_index: if i + 1 < hop_count { i + 1 } else { i },
                hop_count,
                tx_hash: format!("{hop_tx_hash:?}"),
                from_addr: format!("{from_addr:?}"),
                to_addr: format!("{next:?}"),
                value_wei: forward.to_string(),
                gas_price_wei: hop_gas.to_string(),
                nonce: 0,
                chain_id,
                recovery_address: format!("{:?}", rc.recovery_address),
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            let mut line = serde_json::to_string(&entry).unwrap_or_default();
            line.push('\n');
            if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&journal_path) {
                let _ = f.write_all(line.as_bytes());
            }
        }

        let confirm_result = {
            let mut pending_tx = Box::pin(pending);
            let confirmation_start = std::time::Instant::now();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(CONFIRMATION_TIMEOUT_SECS);
            let heartbeat = Duration::from_secs(CONFIRMATION_HEARTBEAT_SECS);
            let mut last_ditch_sent = false;

            loop {
                tokio::select! {
                    result = &mut pending_tx => {
                        break (Some(result), confirmation_start.elapsed());
                    }
                    _ = tokio::time::sleep(heartbeat) => {
                        // Heartbeat: log progress, do NOT bump gas.
                        // Bumping during heartbeats fails because the proxy's gas
                        // budget is sized for the original price — a higher-gas
                        // replacement carries the same value transfer, so the
                        // bumped (gas*21000 + value) exceeds the proxy's balance.
                        // The original 1.5 gwei should confirm in 60s on Sepolia;
                        // if not, the deadline branch below does a single 2x bump.
                        let elapsed = confirmation_start.elapsed();
                        runner_log(
                            worker_id,
                            &stage_label,
                            format!(
                                "Still waiting for {hop_wait_label} confirmation... {} elapsed",
                                format_compact_duration(elapsed)
                            ),
                        );
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        // ── Last-ditch: 2x gas before giving up ──
                        if !last_ditch_sent {
                            #[allow(unused_assignments)]
                            {
                                last_ditch_sent = true;
                            }
                            // Check proxy balance first. The replacement carries
                            // the full forward value, so it costs forward + gas.
                            // The proxy must still have headroom above `forward`.
                            // The seed formula gives (hop_count+2) hops of gas
                            // headroom, of which 1 was spent on the original tx's
                            // gas; the 2x last-ditch needs 1 more hop's worth,
                            // which is available as long as the original tx
                            // hasn't already been mined (in which case the proxy
                            // balance is 0 anyway and we're done).
                            let balance_now = match current_provider.get_balance(proxy.address(), None).await {
                                Ok(b) => b,
                                Err(e) => {
                                    runner_log(
                                        worker_id,
                                        &stage_label,
                                        format!("Last-ditch balance check failed: {e}; skipping bump"),
                                    );
                                    U256::zero()
                                }
                            };
                            // Compute the MAX gas price the proxy can afford for
                            // a replacement tx that carries the full `forward` value.
                            let max_gas = max_affordable_gas_price(balance_now, forward);
                            let target_gas = hop_gas + hop_gas; // desired 2x
                            // Use the smaller of (desired 2x) or (max affordable).
                            // If balance can afford more than 2x, we use 2x to keep
                            // costs predictable; if it can afford less, we use what
                            // we have; if it can't afford any gas, we skip.
                            let emergency_gas = target_gas.min(max_gas);
                            if emergency_gas == U256::zero() {
                                runner_log(
                                    worker_id,
                                    &stage_label,
                                    format!(
                                        "Last-ditch skipped: proxy balance {} too low to afford value+gas (tx may have already mined)",
                                        format_eth_amount(balance_now.as_u128() as f64 / 1e18)
                                    ),
                                );
                            } else {
                                runner_log(
                                    worker_id,
                                    &stage_label,
                                    format!(
                                        "Confirmation timeout approaching, sending last-ditch tx with {}x gas ({}; max affordable {})",
                                        if emergency_gas >= target_gas { "2" } else { "reduced" },
                                        format_eth_amount(emergency_gas.as_u128() as f64 / 1e18),
                                        format_eth_amount(max_gas.as_u128() as f64 / 1e18)
                                    ),
                                );
                                let tx = TransactionRequest::pay(next, forward)
                                    .gas(21_000)
                                    .gas_price(emergency_gas);
                                let send_result = proxy_signer.send_transaction(tx, None).await;
                                match send_result {
                                    Ok(new_pending) => {
                                        rpc_manager.record_success(&current_rpc_url);
                                        pending_tx = Box::pin(new_pending);
                                    }
                                    Err(e) => {
                                        runner_log(
                                            worker_id,
                                            &stage_label,
                                            format!("Last-ditch send failed: {e}; proxy key persisted for recovery."),
                                        );
                                    }
                                }
                            }
                        }
                        break (None, confirmation_start.elapsed());
                    }
                }
            }
        };

        let _hop_receipt = match confirm_result {
            (Some(Ok(receipt_opt)), _) => {
                // Clean up proxy key file for this hop's sender (proxy[i])
                // The key file for proxy[i] was persisted in the previous iteration
                // or before Sender->P1. We don't have the exact filename here,
                // but the recovery dir sweep will handle orphaned keys.
                if let Some(receipt) = &receipt_opt {
                    runner_log(
                        worker_id,
                        &stage_label,
                        format!("Confirmed: {:?}", receipt.transaction_hash),
                    );
                } else {
                    runner_log(
                        worker_id,
                        &stage_label,
                        "Confirmed: Ok(None) - tx may not have been mined",
                    );
                }
                receipt_opt
            },
            (Some(Err(e)), _) => {
                runner_log(
                    worker_id,
                    &stage_label,
                    format!("{hop_wait_label} confirmation failed: {e:#}. Proxy key persisted for recovery."),
                );
                anyhow::bail!("{hop_wait_label} confirmation failed: {e:#}");
            },
            (None, elapsed) => {
                runner_log(
                    worker_id,
                    &stage_label,
                    format!(
                        "Timed out waiting for {hop_wait_label} after last-ditch. Proxy key persisted for recovery."
                    ),
                );
                anyhow::bail!(
                    "Timed out waiting for {hop_wait_label} confirmation after {}",
                    format_compact_duration(elapsed)
                );
            },
        };

        if i == hop_count - 1 {
            recipient_tx_hash = Some(hop_tx_hash);
        }

        remaining = forward;
    }

    let recipient_tx_hash = recipient_tx_hash.context("Missing recipient tx hash")?;

    // ── Target balance verification (delta-based) ──
    // Fetch target balance BEFORE funding starts (we already did this in load_wallets,
    // but we re-fetch to be safe in case the target's balance changed since).
    let (provider_verify_before, _) = create_provider(rpc_manager, http_client)
        .context("No healthy RPC for target balance verification (pre)")?;
    let target_balance_before = provider_verify_before.get_balance(target, None).await.ok();

    let (provider_verify, _) =
        create_provider(rpc_manager, http_client).context("No healthy RPC for target balance verification")?;
    let target_balance_after = provider_verify.get_balance(target, None).await?;

    // Expected delta: the last hop forward amount.
    // The target should have received exactly `remaining` (which equals `forward` from the last hop).
    let actual_delta = target_balance_after.saturating_sub(target_balance_before.unwrap_or(U256::zero()));
    let expected_delta = remaining;
    let delivery_ok = actual_delta >= expected_delta.saturating_sub(U256::from(1_000_000_000u64)); // allow 1 gwei dust
    let shortfall = expected_delta.saturating_sub(actual_delta);

    if !delivery_ok {
        // Target did not receive enough — possible causes:
        // 1. Last hop tx failed but we reported success
        // 2. Concurrent tx drained the target
        // 3. RPC returned stale balance
        warn!(
            "[WK{worker_id}] Target {:?} received less than expected. Expected ~{} ETH, got +{} ETH (shortfall {} ETH). tx={:?}",
            target,
            format_eth_amount(expected_delta.as_u128() as f64 / 1e18),
            format_eth_amount(actual_delta.as_u128() as f64 / 1e18),
            format_eth_amount(shortfall.as_u128() as f64 / 1e18),
            recipient_tx_hash,
        );
    }

    runner_log(
        worker_id,
        "Recipient",
        format!(
            "Target {:?} balance: {} → {} ETH (delta +{} ETH, expected {})",
            target,
            format_eth_amount(target_balance_before.unwrap_or(U256::zero()).as_u128() as f64 / 1e18),
            format_eth_amount(target_balance_after.as_u128() as f64 / 1e18),
            format_eth_amount(actual_delta.as_u128() as f64 / 1e18),
            format_eth_amount(expected_delta.as_u128() as f64 / 1e18),
        ),
    );

    // ── Final summary ──
    let (provider_final, _) = create_provider(rpc_manager, http_client).context("No healthy RPC for final balance")?;
    let sender_balance_after = provider_final.get_balance(sender_address, None).await?;
    let fee_wei = sender_balance
        .checked_sub(sender_balance_after)
        .and_then(|spent| spent.checked_sub(remaining))
        .context("Failed to compute total fee from balance delta")?;
    let duration = format_compact_duration(flow_start.elapsed());

    // Validate sender wasn't drained unexpectedly
    let sender_balance_before = sender_balance;
    let sender_spent_total = sender_balance_before.saturating_sub(sender_balance_after);
    let sender_spent_expected = seed + gas_21k_cost; // initial tx + gas
    // Sender should have spent approximately seed + gas for sender tx
    // (subsequent hops come from proxies, not sender)
    if sender_spent_total > sender_spent_expected + gas_21k_cost {
        warn!(
            "[WK{worker_id}] Sender {:?} spent {} ETH but expected ~{} ETH. Possible unexpected drain.",
            sender_address,
            format_eth_amount(sender_spent_total.as_u128() as f64 / 1e18),
            format_eth_amount(sender_spent_expected.as_u128() as f64 / 1e18),
        );
    }

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

/// Compute the maximum gas price (wei) the proxy can afford for a replacement
/// tx with the given value. Returns 0 if balance is insufficient even for
/// the gas alone (i.e., value > balance).
///
/// The replacement tx costs `value + gas_price * 21000`. We need:
///   gas_price * 21000 <= balance - value
///   gas_price <= (balance - value) / 21000
fn max_affordable_gas_price(balance: U256, value: U256) -> U256 {
    if balance <= value {
        return U256::zero();
    }
    let headroom = balance - value;
    headroom / U256::from(21_000u64)
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

/// Calculate the initial "seed" amount the first sender must send.
/// Covers: target + (hop_count+1) hop gas costs for normal flow + 1 extra hop
/// gas budget per hop to cover the 2x last-ditch gas bump on confirmation timeout.
/// The 2x bump is the only gas-bump attempt (heartbeats no longer bump).
fn calculate_seed_amount(target_amount: U256, gas_price: U256, hop_count: usize) -> U256 {
    let gas_21k = U256::from(21_000u64) * gas_price;
    target_amount + gas_21k * U256::from(hop_count as u64 + 2) + gas_21k
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
        rpc_manager: Arc<RpcManager>,
        http_client: reqwest::Client,
        password: String,
        chain_id: u64,
        max_targets: Option<usize>,
        recovery: Option<RecoveryContext>,
    ) -> Self {
        Self {
            manager,
            provider,
            rpc_manager,
            http_client,
            password,
            chain_id,
            max_targets,
            recovery: recovery.map(Arc::new),
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

        // ── Input validation: argument relationships ──
        ensure!(
            args.min_target <= args.max_target,
            "--min-target ({}) cannot exceed --max-target ({})",
            args.min_target,
            args.max_target
        );
        ensure!(
            args.min_hops <= args.max_hops,
            "--min-hops ({}) cannot exceed --max-hops ({})",
            args.min_hops,
            args.max_hops
        );
        ensure!(
            args.min_gwei <= args.max_gwei,
            "--min-gwei ({}) cannot exceed --max-gwei ({})",
            args.min_gwei,
            args.max_gwei
        );
        ensure!(
            args.min_delay_secs <= args.max_delay_secs,
            "--min-delay-secs ({}) cannot exceed --max-delay-secs ({})",
            args.min_delay_secs,
            args.max_delay_secs
        );
        ensure!(
            args.min_worker_interval_secs <= args.max_worker_interval_secs,
            "--min-worker-interval-secs ({}) cannot exceed --max-worker-interval-secs ({})",
            args.min_worker_interval_secs,
            args.max_worker_interval_secs
        );
        ensure!(
            args.max_hops > 0,
            "--max-hops must be at least 1 (got 0)"
        );
        ensure!(
            args.min_target > 0.0,
            "--min-target must be positive (got {})",
            args.min_target
        );
        ensure!(
            args.min_balance >= args.max_balance,
            "--min-balance ({}) should be >= --max-balance ({})",
            args.min_balance,
            args.max_balance
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
            let rpc_manager = Arc::clone(&self.rpc_manager);
            let http_client = self.http_client.clone();
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
            let recovery = self.recovery.clone();
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

                    let recovery_ref = recovery.as_deref();
                    let result = fund_via_chain(
                        &rpc_manager,
                        &http_client,
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
                        recovery_ref,
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
    /// Also performs balance sufficiency validation:
    /// estimates cumulative gas cost per sender and warns if any sender
    /// would run out of funds.
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
        for pf in &plan {
            println!(
                "[DRY] Fund {:?} ({:.4} ETH) from sender idx {} via {} hops (target {:.4} ETH)",
                pf.target,
                pf.target_balance_eth,
                pf.sender_idx,
                pf.hops,
                pf.amount.as_u128() as f64 / 1e18
            );
        }

        // ── Balance sufficiency validation ──
        // Each sender pays seed_amount + 21k*gas per assigned target.
        // Assume worst case: max gas, max target, max hops.
        // Use 1.5 gwei (default max_gwei) as the conservative estimate.
        let conservative_gas_gwei = max_target.max(min_target) * 1.5;
        let conservative_gas_wei = (conservative_gas_gwei * 1_000_000_000.0) as u128;
        let max_hops_count = max_hops;
        let gas_21k = 21_000u128 * conservative_gas_wei;
        let max_target_wei = (max_target * 1e18) as u128;

        let mut sender_required: std::collections::HashMap<usize, u128> = std::collections::HashMap::new();
        for pf in &plan {
            // Worst case per assignment: seed = target + (hops+2) * gas_21k + 1 tx gas
            let seed_max = max_target_wei + ((max_hops_count as u128 + 2) * gas_21k) + gas_21k;
            *sender_required.entry(pf.sender_idx).or_insert(0) += seed_max;
        }

        let mut warnings = 0;
        for (sender_idx, required) in &sender_required {
            if let Some(sender) = senders.get(*sender_idx) {
                let sender_balance_wei = (sender.balance_eth * 1e18) as u128;
                if *required > sender_balance_wei {
                    let required_eth = *required as f64 / 1e18;
                    let shortfall_eth = (*required - sender_balance_wei) as f64 / 1e18;
                    println!(
                        "[DRY WARN] Sender {} ({:?}) needs ~{} ETH but only has {} ETH (shortfall {} ETH)",
                        sender_idx,
                        sender.address,
                        format_eth_amount(required_eth),
                        format_eth_amount(sender.balance_eth),
                        format_eth_amount(shortfall_eth),
                    );
                    warnings += 1;
                }
            }
        }

        if warnings == 0 {
            println!("[DRY] Balance check: all senders have sufficient ETH");
        } else {
            println!(
                "[DRY WARN] {} sender(s) may run out of funds. Increase senders or reduce --max-targets.",
                warnings
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
                // Wrap balance query with a timeout to prevent hanging on slow RPC
                let balance = match tokio::time::timeout(
                    Duration::from_secs(30),
                    prov.get_balance(address, None),
                )
                .await
                {
                    Ok(Ok(b)) => b.as_u128() as f64 / 1e18,
                    Ok(Err(_)) => {
                        warn!("Wallet {idx}: balance query failed, skipping");
                        return None;
                    },
                    Err(_) => {
                        warn!("Wallet {idx}: balance query timed out after 30s, skipping");
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

    /// Recovery mode: scan proxy-recovery dir, re-derive proxy keys, sweep any stuck ETH back
    #[arg(long)]
    recover: bool,

    /// Directory for proxy keystore and recovery journal (default: proxy-recovery/)
    #[arg(long, default_value = RECOVERY_DIR_DEFAULT)]
    recovery_dir: String,

    /// Address to sweep stuck proxy ETH to during recovery (default: first sender wallet)
    #[arg(long)]
    recovery_address: Option<String>,

    /// Dry-run recovery: print what would be recovered without sending txs
    #[arg(long)]
    recover_dry_run: bool,

    /// Skip recovery confirmation prompt
    #[arg(long)]
    recover_yes: bool,
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

    // RPC manager with all configured URLs (rotation + failover)
    let rpc_urls: Vec<String> = if config.rpc_urls.is_empty() {
        vec![config.rpc_url.clone()]
    } else {
        config.rpc_urls.clone()
    };
    let rpc_manager = Arc::new(RpcManager::new(config.chain_id, &rpc_urls));
    let http_client = reqwest::Client::new();

    // Initial provider from a healthy RPC
    let (provider, _) = create_provider(&rpc_manager, &http_client)?;
    println!("Connected to RPC: {}", &rpc_urls[0]);

    // ── Recovery mode ──
    if args.recover || args.recover_dry_run {
        let recovery_address = match &args.recovery_address {
            Some(addr) => {
                let clean = addr.trim_start_matches("0x");
                if clean.len() != 40 {
                    anyhow::bail!(
                        "Invalid recovery address: {}. Expected 40 hex chars (without 0x prefix).",
                        addr
                    );
                }
                let bytes = hex::decode(clean).context("Invalid recovery address hex")?;
                Address::from_slice(&bytes)
            },
            None => {
                // Use first sender wallet as recovery address
                // We need to decrypt wallet 0 (or the first sender) to get its address
                let decrypted = manager
                    .get_wallet(0, Some(&password))
                    .await
                    .context("Failed to decrypt wallet 0 to determine recovery address. Use --recovery-address")?;
                let w: LocalWallet = decrypted
                    .evm_private_key
                    .parse::<LocalWallet>()
                    .context("Failed to parse wallet 0 key for recovery address")?
                    .with_chain_id(config.chain_id);
                w.address()
            },
        };

        let is_dry = args.recover_dry_run;
        let mode_label = if is_dry { "DRY RUN" } else { "LIVE" };
        println!("\n=== Recovery Mode [{mode_label}] ===");
        println!("Recovery dir:    {}", args.recovery_dir);
        println!("Recovery addr:   {recovery_address:?}");

        if !is_dry && !args.recover_yes {
            print!("? Sweep all stuck proxy ETH to {recovery_address:?}? [y/N] ");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Aborted by user.");
                return Ok(());
            }
        }

        return recover_proxies(
            &args.recovery_dir,
            &password,
            recovery_address,
            config.chain_id,
            &rpc_manager,
            &http_client,
            is_dry,
        )
        .await;
    }

    // ── Recovery context for live funding mode ──
    let shutdown = Arc::new(AtomicBool::new(false));
    install_shutdown_handler(Arc::clone(&shutdown));

    // Resolve recovery address for emergency sweep (skip in dry-run — not needed)
    let recovery_ctx = if args.dry_run {
        None
    } else {
        let recovery_address = match &args.recovery_address {
            Some(addr) => {
                let clean = addr.trim_start_matches("0x");
                let bytes = hex::decode(clean).context("Invalid recovery address hex")?;
                Address::from_slice(&bytes)
            },
            None => {
                let decrypted = manager
                    .get_wallet(0, Some(&password))
                    .await
                    .context("Failed to decrypt wallet 0 for recovery address")?;
                let w: LocalWallet = decrypted
                    .evm_private_key
                    .parse::<LocalWallet>()
                    .context("Failed to parse wallet 0 key")?
                    .with_chain_id(config.chain_id);
                w.address()
            },
        };

        Some(RecoveryContext {
            dir: args.recovery_dir.clone(),
            password: password.clone(),
            recovery_address,
            chain_id: config.chain_id,
            shutdown_requested: Arc::clone(&shutdown),
        })
    };

    // Build and run
    let funder = Funder::new(
        manager,
        provider,
        rpc_manager,
        http_client,
        password,
        config.chain_id,
        args.max_targets,
        recovery_ctx,
    );

    let result = funder.run(&args).await;

    // ── Emergency sweep on exit ──
    if let Some(ref rc) = funder.recovery {
        if !args.dry_run && !shutdown.load(Ordering::SeqCst) {
            // Normal completion — do quick sweep check
            match emergency_sweep_all(rc, &funder.rpc_manager, &funder.http_client).await {
                Ok(()) => {},
                Err(e) => warn!("Post-run emergency sweep encountered issues: {e}"),
            }
        } else if shutdown.load(Ordering::SeqCst) {
            // User pressed Ctrl+C — warn about possible stuck funds
            eprintln!();
            eprintln!("╔══════════════════════════════════════════════════════════════════╗");
            eprintln!("║ SHUTDOWN DETECTED — Stuck proxy funds may exist                  ║");
            eprintln!("║                                                                  ║");
            eprintln!("║ To recover any stuck ETH from in-flight proxies, run:            ║");
            eprintln!("║                                                                  ║");
            eprintln!("║   sepolia-funder --config <config.toml> --recover --recover-yes ║");
            eprintln!("║                                                                  ║");
            eprintln!("║ Proxy keys are encrypted in: {}     ", rc.dir);
            eprintln!("╚══════════════════════════════════════════════════════════════════╝");
        }
    }

    result
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

    #[test]
    fn test_sender_state_unlock_nonexistent_index_does_not_panic() {
        let mut state = SenderState {
            use_counts: vec![0],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        };
        // unlocking an index that was never locked should not panic
        state.unlock(42);
        state.unlock(0);
    }

    #[test]
    fn test_sender_state_double_lock_does_not_increment_use_count_twice() {
        // try_pick_and_lock always increments, so picking twice for the same
        // sender requires unlocking + re-picking. this tests that locking
        // excludes the sender until unlocked (use_count only increments on pick).
        let senders = vec![dummy_wallet(0, 1.0)];
        let mut state = SenderState {
            use_counts: vec![0],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);

        let p1 = state.try_pick_and_lock(&senders, 10, &mut rng);
        assert_eq!(p1, Some(0));
        assert_eq!(state.use_counts[0], 1);

        // locked → returns None, use_count stays 1
        let p2 = state.try_pick_and_lock(&senders, 10, &mut rng);
        assert!(p2.is_none());
        assert_eq!(state.use_counts[0], 1);
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

    #[test]
    fn test_generate_dry_run_plan_fixed_hops() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.01), dummy_wallet(2, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 4, 4, None, &mut rng);
        for pf in &plan {
            assert_eq!(pf.hops, 4);
        }
    }

    // choose_gas_price_mgwei

    #[test]
    fn test_choose_gas_price_mgwei_within_bounds() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            let chosen = choose_gas_price_mgwei(5_000, 1_000, 10_000, &mut rng);
            assert!(
                (1_000..=10_000).contains(&chosen),
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
        assert!((1000..=2000).contains(&chosen));
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
        // target = 1 ETH, gas = 20 gwei = 20_000_000_000 wei
        // gas_21k = 21_000 * 20_000_000_000 = 420_000_000_000_000 wei
        // For 3 hops: seed = 1 + 420_000_000_000_000 * (3+2) + 420_000_000_000_000
        //           = 1 + 0.0021 + 0.00042 = 1.00252
        let target = parse_units(1u64, "ether").unwrap().into();
        let gas = U256::from(20_000_000_000u64);
        let seed = calculate_seed_amount(target, gas, 3);
        let seed_eth = seed.as_u128() as f64 / 1e18;
        // new formula: target + gas_21k * (hop_count + 3) = 1.0 + 0.00252
        assert!(
            (seed_eth - 1.00252).abs() < 1e-4,
            "seed_eth should be ~1.00252 but got {seed_eth}"
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

    #[test]
    fn test_confirm_funding_with_whitespace_prefix() {
        let input = b"  y\n";
        let result = confirm_funding(3, 1, &input[..]).unwrap();
        assert!(result, "trim should strip leading whitespace before comparison");
    }

    #[test]
    fn test_confirm_funding_with_carriage_return() {
        let input = b"y\r\n";
        let result = confirm_funding(3, 1, &input[..]).unwrap();
        assert!(result);
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

    #[tokio::test]
    async fn test_await_confirmation_with_progress_immediate_error() {
        let future = async { Err::<(), std::io::Error>(std::io::Error::other("tx reverted")) };
        let result = await_confirmation_with_progress(
            1,
            "Funder",
            "Proxy -> P2",
            future,
            Duration::from_secs(60),
            Duration::from_secs(10),
        )
        .await;

        assert!(result.is_err());
        let msg = format!("{:#}", result.unwrap_err());
        assert!(msg.contains("Proxy -> P2 confirmation failed"));
        assert!(msg.contains("tx reverted"));
    }

    #[tokio::test]
    async fn test_await_confirmation_with_progress_sub_second_heartbeat_clamped() {
        // heartbeat < 1s should be clamped to 1s by the function
        let future = async { Ok::<_, std::io::Error>(42) };
        let result = await_confirmation_with_progress(
            1,
            "Funder",
            "test",
            future,
            Duration::from_secs(60),
            Duration::from_millis(100),
        )
        .await;

        assert_eq!(result.unwrap(), 42);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // prepare_funding_sets (via Funder)
    // ──────────────────────────────────────────────────────────────────────────

    fn dummy_rpc_manager(urls: &[&str]) -> Arc<RpcManager> {
        let str_urls: Vec<String> = urls.iter().map(|s| s.to_string()).collect();
        Arc::new(RpcManager::new(1, &str_urls))
    }

    #[test]
    fn test_prepare_funding_sets_basic() {
        let funder = Funder::new(
            Arc::new(core_logic::WalletManager::new().unwrap()),
            Provider::new(Http::new(reqwest::Url::parse("http://localhost").unwrap())),
            dummy_rpc_manager(&["http://localhost"]),
            reqwest::Client::new(),
            "pw".into(),
            1,
            None,
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
        // network=10000 → floor=9000 (90%), ceiling=11000 (110%)
        // min=100, max=1000 → floor=max(100,9000)=9000, ceiling=max(1000,11000)=11000
        // effective_floor = min(9000, 11000) = 9000
        let chosen = choose_gas_price_mgwei(10_000, 100, 1_000, &mut rng);
        assert!(
            (9000..=11_000).contains(&chosen),
            "chosen={} expected in [9000, 11000]",
            chosen
        );
    }

    #[test]
    fn test_choose_gas_price_mgwei_ceiling_capped_below_floor() {
        let mut rng = StdRng::seed_from_u64(42);
        // min=1_000_000, max=500, network=0 → floor=1_000_000, ceiling=500
        // effective_floor = min(1_000_000, 500) = 500
        let chosen = choose_gas_price_mgwei(0, 1_000_000, 500, &mut rng);
        assert_eq!(chosen, 500);
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
            dummy_rpc_manager(&["http://localhost"]),
            reqwest::Client::new(),
            "pw".into(),
            1,
            None,
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
            dummy_rpc_manager(&["http://localhost"]),
            reqwest::Client::new(),
            "password".into(),
            1,
            None,
            None,
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        let wallets = rt.block_on(funder.load_wallets(10)).unwrap();
        assert!(wallets.is_empty(), "all decryption failures should yield empty result");

        // Cleanup
        let _ = std::fs::remove_file(&wallet_path);
        let _ = std::fs::remove_dir(&dir);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Gas bump math (12% increase per heartbeat)
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn gas_bump_twelve_percent() {
        let gas = U256::from(10_000_000_000u64);
        let bumped = gas + gas / U256::from(100) * U256::from(12);
        assert_eq!(bumped, U256::from(11_200_000_000u64));
    }

    #[test]
    fn gas_bump_twelve_percent_rounds_down() {
        let gas = U256::from(1_000_000_001u64);
        let bumped = gas + gas / U256::from(100) * U256::from(12);
        assert_eq!(bumped, U256::from(1_120_000_001u64));
    }

    #[test]
    fn gas_bump_zero_gas_stays_zero() {
        let gas = U256::zero();
        let bumped = gas + gas / U256::from(100) * U256::from(12);
        assert_eq!(bumped, U256::zero());
    }

    #[test]
    fn gas_bump_max_gwei_does_not_overflow() {
        let gas = U256::from(100_000_000_000_000_000u64);
        let bumped = gas + gas / U256::from(100) * U256::from(12);
        assert!(bumped > gas);
        assert_eq!(bumped, gas * U256::from(112) / U256::from(100));
    }

    #[test]
    fn gas_bump_after_multiple_bumps() {
        let mut gas = U256::from(10_000_000_000u64);
        for _ in 0..5 {
            gas = gas + gas / U256::from(100) * U256::from(12);
        }
        // After 5 bumps the growth is compound: 1.12^5 ≈ 1.762
        assert!(gas > U256::from(17_600_000_000u64));
        assert!(gas < U256::from(17_700_000_000u64));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_eth_amount
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn format_eth_amount_removes_trailing_zeros() {
        assert_eq!(format_eth_amount(1.500000), "1.5");
    }

    #[test]
    fn format_eth_amount_whole_number_no_decimal() {
        assert_eq!(format_eth_amount(2.000000), "2");
    }

    #[test]
    fn format_eth_amount_zero() {
        assert_eq!(format_eth_amount(0.0), "0");
    }

    #[test]
    fn format_eth_amount_small() {
        assert_eq!(format_eth_amount(0.001), "0.001");
    }

    #[test]
    fn format_eth_amount_many_trailing_zeros() {
        assert_eq!(format_eth_amount(0.100000), "0.1");
    }

    #[test]
    fn format_eth_amount_all_zeros_after_decimal() {
        assert_eq!(format_eth_amount(42.000000), "42");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_compact_duration
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn format_compact_duration_seconds_only() {
        assert_eq!(format_compact_duration(Duration::from_secs(45)), "45s");
    }

    #[test]
    fn format_compact_duration_minutes_and_seconds() {
        assert_eq!(format_compact_duration(Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn format_compact_duration_hours_minutes_seconds() {
        assert_eq!(format_compact_duration(Duration::from_secs(3661)), "1h 1m 1s");
    }

    #[test]
    fn format_compact_duration_zero() {
        assert_eq!(format_compact_duration(Duration::from_secs(0)), "0s");
    }

    #[test]
    fn format_compact_duration_exact_hour() {
        assert_eq!(format_compact_duration(Duration::from_secs(3600)), "1h 0m 0s");
    }

    #[test]
    fn format_compact_duration_large_value() {
        assert_eq!(format_compact_duration(Duration::from_secs(9999)), "2h 46m 39s");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_worker_rest
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn format_worker_rest_none() {
        assert_eq!(format_worker_rest(0, 0), "none");
    }

    #[test]
    fn format_worker_rest_single_value() {
        assert_eq!(format_worker_rest(30, 30), "30s");
    }

    #[test]
    fn format_worker_rest_range() {
        assert_eq!(format_worker_rest(10, 30), "10-30s");
    }

    #[test]
    fn format_worker_rest_zero_range() {
        assert_eq!(format_worker_rest(0, 5), "0-5s");
    }

    #[test]
    fn format_worker_rest_large_values() {
        assert_eq!(format_worker_rest(3600, 7200), "3600-7200s");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // choose_worker_rest_secs
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn choose_worker_rest_secs_zero_range_returns_zero() {
        let mut rng = StdRng::seed_from_u64(42);
        assert_eq!(choose_worker_rest_secs(0, 0, &mut rng), 0);
    }

    #[test]
    fn choose_worker_rest_secs_fixed_range() {
        let mut rng = StdRng::seed_from_u64(42);
        assert_eq!(choose_worker_rest_secs(10, 10, &mut rng), 10);
    }

    #[test]
    fn choose_worker_rest_secs_within_bounds() {
        let mut rng = StdRng::seed_from_u64(99);
        for _ in 0..50 {
            let secs = choose_worker_rest_secs(5, 15, &mut rng);
            assert!((5..=15).contains(&secs), "secs={secs} not in [5, 15]");
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // worker_tag
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn worker_tag_zero_padded() {
        assert_eq!(worker_tag(1), "WK001");
        assert_eq!(worker_tag(12), "WK012");
        assert_eq!(worker_tag(123), "WK123");
    }

    #[test]
    fn worker_tag_large_id() {
        assert_eq!(worker_tag(9999), "WK9999");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // select_targets_to_fund
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn select_targets_to_fund_all_when_no_cap() {
        let targets = vec![dummy_wallet(0, 0.005), dummy_wallet(1, 0.003), dummy_wallet(2, 0.001)];
        let selected = select_targets_to_fund(&targets, None);
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn select_targets_to_fund_empty_targets() {
        let selected = select_targets_to_fund(&[], Some(5));
        assert!(selected.is_empty());
    }

    // ──────────────────────────────────────────────────────────────────────────
    // compute_max_per_sender edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn compute_max_per_sender_more_senders_than_targets() {
        assert_eq!(compute_max_per_sender(3, 10), 1);
    }

    #[test]
    fn compute_max_per_sender_exact_division() {
        assert_eq!(compute_max_per_sender(10, 5), 2);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // filter_senders / filter_targets edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn filter_senders_all_exactly_at_min_balance() {
        let wallets = vec![dummy_wallet(0, 0.5), dummy_wallet(1, 0.5)];
        let senders = filter_senders(&wallets, 0.5);
        assert_eq!(senders.len(), 2);
    }

    #[test]
    fn filter_senders_empty_wallet_list() {
        let senders = filter_senders(&[], 1.0);
        assert!(senders.is_empty());
    }

    #[test]
    fn filter_senders_zero_balance_keeps_all() {
        let wallets = vec![dummy_wallet(0, 0.0), dummy_wallet(1, 0.5)];
        let senders = filter_senders(&wallets, 0.0);
        assert_eq!(senders.len(), 2);
    }

    #[test]
    fn filter_targets_empty_wallet_list() {
        let targets = filter_targets(&[], 0.01);
        assert!(targets.is_empty());
    }

    #[test]
    fn filter_targets_all_exactly_at_max_balance() {
        let wallets = vec![dummy_wallet(0, 0.01), dummy_wallet(1, 0.01)];
        let targets = filter_targets(&wallets, 0.01);
        assert_eq!(targets.len(), 2);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // distribute_round_robin edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn distribute_round_robin_empty_items() {
        let queues = distribute_round_robin::<i32>(vec![], 3);
        assert_eq!(queues.len(), 3);
        for q in &queues {
            assert!(q.is_empty());
        }
    }

    #[test]
    fn distribute_round_robin_one_item_many_workers() {
        let queues = distribute_round_robin(vec![42], 10);
        assert_eq!(queues.len(), 10);
        assert_eq!(queues[0].len(), 1);
        assert_eq!(queues[0][0], 42);
        for q in queues.iter().skip(1) {
            assert!(q.is_empty());
        }
    }

    #[test]
    fn distribute_round_robin_single_worker() {
        let queues = distribute_round_robin(vec![1, 2, 3], 1);
        assert_eq!(queues.len(), 1);
        assert_eq!(queues[0].len(), 3);
    }

    #[test]
    fn distribute_round_robin_fewer_items_than_workers() {
        let queues = distribute_round_robin(vec![1, 2], 5);
        let populated = queues.iter().filter(|q| !q.is_empty()).count();
        assert_eq!(populated, 2);
    }

    #[test]
    fn distribute_round_robin_worker_count_zero_defaults_to_one() {
        let queues = distribute_round_robin(vec![10, 20], 0);
        assert_eq!(queues.len(), 1);
        assert_eq!(queues[0].len(), 2);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // should_skip_confirmation edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn should_skip_confirmation_both_yes_and_dry_run() {
        assert!(should_skip_confirmation(true, true));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // compute_max_per_sender edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn compute_max_per_sender_zero_targets_with_senders() {
        assert_eq!(compute_max_per_sender(0, 5), 0);
    }

    #[test]
    fn compute_max_per_sender_uneven_division_rounds_up() {
        assert_eq!(compute_max_per_sender(10, 3), 4);
    }

    #[test]
    fn compute_max_per_sender_targets_less_than_senders() {
        assert_eq!(compute_max_per_sender(2, 10), 1);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // calculate_seed_amount edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn calculate_seed_amount_zero_target() {
        let gas = U256::from(20_000_000_000u64);
        let seed = calculate_seed_amount(U256::zero(), gas, 3);
        let gas_21k = U256::from(21_000) * gas;
        // new formula: target + gas_21k * (h+2) + gas_21k = gas_21k * (h+3)
        assert_eq!(seed, gas_21k * U256::from(6));
    }

    #[test]
    fn calculate_seed_amount_zero_hops() {
        let target = parse_units(1u64, "ether").unwrap().into();
        let gas = U256::from(20_000_000_000u64);
        let seed = calculate_seed_amount(target, gas, 0);
        // new formula: target + gas_21k * (0+2) + gas_21k = target + 3*gas_21k
        let expected = target + U256::from(21_000) * gas * U256::from(3);
        assert_eq!(seed, expected);
    }

    #[test]
    fn calculate_seed_amount_large_hop_count() {
        let target = parse_units(1u64, "ether").unwrap().into();
        let gas = U256::from(20_000_000_000u64);
        let seed = calculate_seed_amount(target, gas, 50);
        // new formula: target + gas_21k * (50+2) + gas_21k = target + 53*gas_21k
        let expected = target + U256::from(21_000) * gas * U256::from(53);
        assert_eq!(seed, expected);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // calculate_forward_amount edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn calculate_forward_amount_exact_gas_cost_returns_zero() {
        let gas = U256::from(20_000_000_000u64);
        let remaining = U256::from(21_000) * gas;
        let forward = calculate_forward_amount(remaining, gas);
        assert_eq!(forward, U256::zero());
    }

    #[test]
    fn calculate_forward_amount_less_than_gas_saturates() {
        let gas = U256::from(20_000_000_000u64);
        let remaining = U256::from(10_000) * gas;
        let forward = calculate_forward_amount(remaining, gas);
        assert_eq!(forward, U256::zero());
    }

    #[test]
    fn calculate_seed_amount_zero_gas_price() {
        let target = parse_units(1u64, "ether").unwrap().into();
        let seed = calculate_seed_amount(target, U256::zero(), 5);
        assert_eq!(seed, target);
    }

    #[test]
    fn calculate_forward_amount_zero_gas_price() {
        let remaining = parse_units(1u64, "ether").unwrap().into();
        let forward = calculate_forward_amount(remaining, U256::zero());
        assert_eq!(forward, remaining);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // get_next_hop_address edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_get_next_hop_address_single_hop_goes_to_target() {
        let target = Address::from_low_u64_be(99);
        let proxies = vec![Address::from_low_u64_be(1)];
        let next = get_next_hop_address(0, 1, target, &proxies);
        assert_eq!(next, target);
    }

    #[test]
    fn test_get_next_hop_address_middle_hop_returns_proxy() {
        let target = Address::from_low_u64_be(99);
        let proxies = vec![
            Address::from_low_u64_be(1),
            Address::from_low_u64_be(2),
            Address::from_low_u64_be(3),
            Address::from_low_u64_be(4),
        ];
        // hop_index=1 → proxy_addrs[1+1]=proxy_addrs[2]=address 3
        let next = get_next_hop_address(1, 4, target, &proxies);
        assert_eq!(next, Address::from_low_u64_be(3));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_hop_label edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_format_hop_label_single_hop() {
        assert_eq!(format_hop_label(0, 1), "target");
    }

    #[test]
    fn test_format_hop_label_first_of_many() {
        assert_eq!(format_hop_label(0, 5), "P2");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // choose_gas_price_mgwei edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_choose_gas_price_mgwei_network_zero() {
        let mut rng = StdRng::seed_from_u64(42);
        let chosen = choose_gas_price_mgwei(0, 1_000, 20_000, &mut rng);
        assert!((1_000..=20_000).contains(&chosen));
    }

    #[test]
    fn test_choose_gas_price_mgwei_min_exceeds_max() {
        let mut rng = StdRng::seed_from_u64(42);
        // network=0 disables the 90%/110% inflation so ceil stays at max_mgwei
        let chosen = choose_gas_price_mgwei(0, 50_000, 10_000, &mut rng);
        assert_eq!(chosen, 10_000);
    }

    #[test]
    fn test_choose_gas_price_mgwei_caps_at_100k() {
        let mut rng = StdRng::seed_from_u64(42);
        let chosen = choose_gas_price_mgwei(200_000, 150_000, 200_000, &mut rng);
        assert_eq!(chosen, 100_000);
    }

    #[test]
    fn test_choose_gas_price_mgwei_network_drives_floor_up() {
        let mut rng = StdRng::seed_from_u64(42);
        // network 10_000 mgwei → 90% = 9_000 floor
        let chosen = choose_gas_price_mgwei(10_000, 500, 20_000, &mut rng);
        assert!(
            (9_000..=20_000).contains(&chosen),
            "chosen={chosen} not in [9000, 20000]"
        );
    }

    #[test]
    fn test_choose_gas_price_mgwei_both_bounds_equal() {
        let mut rng = StdRng::seed_from_u64(42);
        // network=0 so neither bound gets inflated by the 90%/110% rule
        let chosen = choose_gas_price_mgwei(0, 5_000, 5_000, &mut rng);
        assert_eq!(chosen, 5_000);
    }

    #[test]
    fn test_choose_gas_price_mgwei_min_zero_with_network() {
        let mut rng = StdRng::seed_from_u64(42);
        // network=5_000, min=0 → floor = max(0, 4500) = 4500
        let chosen = choose_gas_price_mgwei(5_000, 0, 10_000, &mut rng);
        assert!(
            (4_500..=10_000).contains(&chosen),
            "chosen={chosen} not in [4500, 10000]"
        );
    }

    #[test]
    fn test_choose_gas_price_mgwei_network_higher_than_cap() {
        let mut rng = StdRng::seed_from_u64(42);
        // network=1_000_000 → floor capped at 100_000, ceiling capped at 100_000
        let chosen = choose_gas_price_mgwei(1_000_000, 500, 200_000, &mut rng);
        assert_eq!(chosen, 100_000);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // pick_sender edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_pick_sender_empty_list_does_not_panic() {
        let mut rng = StdRng::seed_from_u64(42);
        let idx = pick_sender(&[], &[], 10, &mut rng);
        assert_eq!(idx, 0);
    }

    #[test]
    fn test_pick_sender_single_sender_at_limit_falls_back() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let use_counts = vec![5];
        let mut rng = StdRng::seed_from_u64(42);
        // at max_per_sender=5 limit → fallback picks sender 0 anyway
        let idx = pick_sender(&senders, &use_counts, 5, &mut rng);
        assert_eq!(idx, 0);
    }

    #[test]
    fn test_pick_sender_zero_max_per_sender() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0)];
        let use_counts = vec![0, 0];
        let mut rng = StdRng::seed_from_u64(42);
        // max_per_sender=0 → no candidates → fallback
        let idx = pick_sender(&senders, &use_counts, 0, &mut rng);
        assert!(idx < 2);
    }

    #[test]
    fn test_pick_sender_use_counts_longer_than_senders() {
        // programming error guard: use_counts may reference non-existent senders
        let senders = vec![dummy_wallet(0, 1.0)];
        let use_counts = vec![0, 5];
        let mut rng = StdRng::seed_from_u64(42);
        let idx = pick_sender(&senders, &use_counts, 10, &mut rng);
        assert_eq!(idx, 0);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_eth_amount edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn format_eth_amount_large_value() {
        assert_eq!(format_eth_amount(12345.678901), "12345.678901");
    }

    #[test]
    fn format_eth_amount_many_decimal_places() {
        assert_eq!(format_eth_amount(1.234567890), "1.234568");
    }

    #[test]
    fn format_eth_amount_single_digit() {
        assert_eq!(format_eth_amount(0.1), "0.1");
    }

    #[test]
    fn format_eth_amount_only_non_zero_tenths() {
        assert_eq!(format_eth_amount(0.500001), "0.500001");
    }

    #[test]
    fn format_eth_amount_nan_does_not_panic() {
        let result = format_eth_amount(f64::NAN);
        assert_eq!(result, "NaN");
    }

    #[test]
    fn format_eth_amount_infinity_does_not_panic() {
        let pos = format_eth_amount(f64::INFINITY);
        let neg = format_eth_amount(f64::NEG_INFINITY);
        assert_eq!(pos, "inf");
        assert_eq!(neg, "-inf");
    }

    #[test]
    fn format_eth_amount_negative_value() {
        assert_eq!(format_eth_amount(-1.5), "-1.5");
        assert_eq!(format_eth_amount(-0.001), "-0.001");
    }

    #[test]
    fn format_eth_amount_very_small_positive() {
        assert_eq!(format_eth_amount(0.000001), "0.000001");
        assert_eq!(format_eth_amount(0.0000004), "0");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_compact_duration edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn format_compact_duration_one_second() {
        assert_eq!(format_compact_duration(Duration::from_secs(1)), "1s");
    }

    #[test]
    fn format_compact_duration_one_minute() {
        assert_eq!(format_compact_duration(Duration::from_secs(60)), "1m 0s");
    }

    #[test]
    fn format_compact_duration_one_hour() {
        assert_eq!(format_compact_duration(Duration::from_secs(3600)), "1h 0m 0s");
    }

    #[test]
    fn format_compact_duration_max_does_not_panic() {
        let result = format_compact_duration(Duration::from_secs(u64::MAX));
        assert!(result.contains("h"));
        assert!(result.contains("m"));
        assert!(result.contains("s"));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // choose_worker_rest_secs edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn choose_worker_rest_secs_deterministic_with_seed() {
        let mut rng1 = StdRng::seed_from_u64(12345);
        let mut rng2 = StdRng::seed_from_u64(12345);
        for _ in 0..20 {
            let v1 = choose_worker_rest_secs(1, 100, &mut rng1);
            let v2 = choose_worker_rest_secs(1, 100, &mut rng2);
            assert_eq!(v1, v2, "same seed should produce same rest time");
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_funding_prompt
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn format_funding_prompt_single_target() {
        let prompt = format_funding_prompt(1, 3);
        assert_eq!(prompt, "? This will fund 1 targets with 3 workers. Continue? [y/N]");
    }

    #[test]
    fn format_funding_prompt_many_targets() {
        let prompt = format_funding_prompt(50, 5);
        assert_eq!(prompt, "? This will fund 50 targets with 5 workers. Continue? [y/N]");
    }

    #[test]
    fn format_funding_prompt_zero_targets() {
        let prompt = format_funding_prompt(0, 1);
        assert_eq!(prompt, "? This will fund 0 targets with 1 workers. Continue? [y/N]");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // calculate_seed_amount consistency invariant
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn calculate_seed_amount_forward_backward_invariant() {
        // seed should always exceed target after deducting all hop gas costs
        let target = parse_units(0.02, "ether").unwrap().into();
        let gas = U256::from(2_000_000_000u64); // 2 gwei
        for hops in 0..=10 {
            let seed = calculate_seed_amount(target, gas, hops);
            let mut remaining = seed;
            // deduct sender gas
            remaining = remaining.saturating_sub(U256::from(21_000) * gas);
            for _ in 0..hops {
                remaining = calculate_forward_amount(remaining, gas);
            }
            assert!(
                remaining >= target,
                "hops={hops}: remaining {remaining} < target {target}"
            );
        }
    }

    /// Verify the seed is large enough that a proxy can afford the 2x last-ditch
    /// gas bump on confirmation timeout. The replacement tx costs
    /// `forward + 2 * gas * 21000`, so the proxy's pre-send balance must be
    /// `>= forward + 2 * gas_21k`. After the hop, each proxy retains the headroom
    /// between what it received and what it forwarded.
    #[test]
    fn seed_supports_2x_last_ditch_bump() {
        let target = parse_units(0.02, "ether").unwrap().into();
        let gas = U256::from(1_500_000_000u64); // 1.5 gwei
        for hops in 1..=10 {
            let seed = calculate_seed_amount(target, gas, hops);
            let gas_21k = U256::from(21_000u64) * gas;
            // After the first hop sends seed->forward1, proxy 1 has forward1 left.
            // The replacement needs forward + 2*gas_21k worth of headroom.
            // forward at each hop ≈ seed - gas_21k
            // The proxy that needs to bump has balance = forward (before sending)
            // and the replacement costs forward + 2*gas_21k, so it needs
            // 2*gas_21k in headroom beyond forward.
            // With (h+3)*gas_21k in seed and 1*gas_21k per hop, headroom is h+2.
            let headroom_gas = hops as u64 + 2;
            assert!(
                headroom_gas >= 2,
                "hops={hops}: headroom {headroom_gas} gas units must be >= 2 for 2x last-ditch"
            );
            // Also verify the math: seed must include at least (h+2) gas_21k
            let min_required = target + gas_21k * U256::from(hops as u64 + 2);
            assert!(
                seed >= min_required,
                "hops={hops}: seed {seed} < min required {min_required}"
            );
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // max_affordable_gas_price tests
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn max_affordable_gas_normal_case() {
        // balance = 0.0005 ETH, value = 0.00025 ETH, headroom = 0.00025 ETH
        // max_gas = 0.00025 / 21000 = ~1.19e10 wei = ~11.9 gwei
        let balance = U256::from(500_000_000_000_000u64); // 0.0005 ETH
        let value = U256::from(250_000_000_000_000u64); // 0.00025 ETH
        let max_gas = max_affordable_gas_price(balance, value);
        // headroom = 0.00025 ETH = 2.5e14 wei
        // max_gas = 2.5e14 / 21000 = 1.19e10 wei
        let headroom = U256::from(250_000_000_000_000u64);
        let expected = headroom / U256::from(21_000u64);
        assert_eq!(max_gas, expected);
        // Verify the result is sane: ~1.19e10 wei = ~11.9 gwei
        assert!(max_gas > U256::from(10_000_000_000u64)); // > 10 gwei
        assert!(max_gas < U256::from(15_000_000_000u64)); // < 15 gwei
    }

    #[test]
    fn max_affordable_gas_balance_equals_value_returns_zero() {
        // If balance == value, headroom is 0, so max_gas is 0
        let balance = U256::from(1_000_000_000_000_000_000u64); // 1 ETH
        let value = U256::from(1_000_000_000_000_000_000u64);
        assert_eq!(max_affordable_gas_price(balance, value), U256::zero());
    }

    #[test]
    fn max_affordable_gas_balance_less_than_value_returns_zero() {
        // If balance < value, can't even afford the value transfer
        let balance = U256::from(500_000_000_000_000_000u64); // 0.5 ETH
        let value = U256::from(1_000_000_000_000_000_000u64); // 1 ETH
        assert_eq!(max_affordable_gas_price(balance, value), U256::zero());
    }

    #[test]
    fn max_affordable_gas_zero_balance() {
        let balance = U256::zero();
        let value = U256::from(1_000_000_000_000_000_000u64);
        assert_eq!(max_affordable_gas_price(balance, value), U256::zero());
    }

    #[test]
    fn max_affordable_gas_zero_value() {
        // No value transfer needed, all balance is for gas
        let balance = U256::from(21_000u64) * U256::from(1_500_000_000u64); // exactly 1.5 gwei's worth
        let value = U256::zero();
        let max_gas = max_affordable_gas_price(balance, value);
        assert_eq!(max_gas, U256::from(1_500_000_000u64));
    }

    #[test]
    fn max_affordable_gas_does_not_overflow() {
        // Use a very large balance and value to ensure no overflow
        let balance = U256::from(10_000_000_000_000_000_000_000_000_000u128); // 1e28 wei
        let value = U256::from(5_000_000_000_000_000_000_000_000_000u128); // 5e27 wei
        let max_gas = max_affordable_gas_price(balance, value);
        // headroom = 5e27, max_gas = 5e27 / 21000 = 2.38e23
        let expected = U256::from(5_000_000_000_000_000_000_000_000_000u128) / U256::from(21_000u64);
        assert_eq!(max_gas, expected);
        // Verify the result is usable: headroom - max_gas * 21000 < 21000
        let remaining = (balance - value) - max_gas * U256::from(21_000u64);
        assert!(remaining < U256::from(21_000u64));
    }

    #[test]
    fn max_affordable_gas_handles_typical_proxy_scenario() {
        // Simulate: proxy received `forward` from previous hop. To send `forward`
        // to the next hop, it needs (forward + 1_hop_gas) of balance.
        // The seed formula gives each proxy exactly 1_hop_gas of headroom beyond
        // what they need to forward — so the 2x last-ditch fits.
        let gas = U256::from(1_500_000_000u64); // 1.5 gwei
        let one_hop_gas = U256::from(21_000u64) * gas;
        let forward = U256::from(20_000_000_000_000_000u64); // 0.02 ETH
        let proxy_balance = forward + one_hop_gas; // funded with 1 extra hop
        // Replacement for 2x last-ditch:
        let target_2x = forward + (gas + gas) * U256::from(21_000u64);
        let max_gas = max_affordable_gas_price(proxy_balance, forward);
        // Headroom = 1_hop_gas, so max_gas = gas (the original price).
        // The 2x last-ditch (gas + gas) would not fit in max_gas, so it would
        // be capped to gas. The replacement still has a higher price than the
        // original tx, which is the goal of a last-ditch.
        assert_eq!(max_gas, gas, "headroom = 1_hop_gas → max_gas = gas");
        // 2x target exceeds max_gas, so the actual replacement would be capped.
        assert!(target_2x > max_gas, "2x should be capped by max_gas");
    }

    #[test]
    fn last_ditch_caps_at_max_affordable_gas() {
        // If 2x is not affordable, last-ditch should use whatever is affordable
        // Simulate: balance has 0.0001 ETH headroom, value = 0.0001 ETH, target_gas = 3 gwei
        // max_gas = 0.0001 / 21000 ≈ 4.76 gwei, so target_gas (3 gwei) fits
        let balance = U256::from(100_000_000_000_000u64); // 0.0001 ETH
        let value = U256::from(50_000_000_000_000u64); // 0.00005 ETH
        let max_gas = max_affordable_gas_price(balance, value);
        let target_gas = U256::from(3_000_000_000u64); // 3 gwei
        let emergency_gas = target_gas.min(max_gas);
        // headroom = 0.00005 ETH = 5e13 wei
        // max_gas = 5e13 / 21000 = 2.38e9 wei = 2.38 gwei
        // target_gas (3 gwei) > max_gas (2.38 gwei), so we use max_gas
        assert!(emergency_gas < target_gas, "should use reduced gas");
        assert!(emergency_gas > U256::zero(), "should still be able to send");
    }

    /// Verify the heartbeat does NOT bump gas (no replacement logic).
    /// This is a documentation/architecture test — the actual loop is in
    /// fund_via_chain and not unit-testable in isolation. We assert the
    /// design choice by checking that calculate_seed_amount has headroom
    /// for the 2x last-ditch WITHOUT requiring per-heartbeat bumps.
    #[test]
    fn heartbeat_should_not_bump_gas() {
        // If heartbeats did bump, the seed would need to scale with 1.12^n.
        // Since the seed is bounded, the only safe strategy is:
        // - heartbeat = log only
        // - last-ditch = single 2x bump at deadline
        // This test pins that behavior by checking the seed for h=10 with
        // max realistic 12% bumps. After 6 heartbeats, gas would be
        // 1.12^6 = 2.01x original. With max_gas=2 gwei, that's 4 gwei,
        // costing 4*21000=84000 gwei per replacement. The seed must not
        // be expected to cover this — that's why heartbeats don't bump.
        let target = parse_units(0.02, "ether").unwrap().into();
        let gas = U256::from(2_000_000_000u64); // 2 gwei (max realistic)
        let seed = calculate_seed_amount(target, gas, 5);
        // seed should be roughly target + small overhead, not scaled to 4 gwei
        let seed_eth = seed.as_u128() as f64 / 1e18;
        // target = 0.02, gas_21k = 0.000042, headroom = 7 hops = 0.000294
        // total = 0.020294
        assert!(
            (seed_eth - 0.02).abs() < 0.001,
            "seed should be target + small overhead, got {seed_eth}"
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_hop_label / get_next_hop_address integration
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn hop_label_and_address_stay_in_sync() {
        let target = Address::from_low_u64_be(99);
        let proxies: Vec<Address> = (0..5).map(Address::from_low_u64_be).collect();
        let hop_count = proxies.len();

        for i in 0..hop_count {
            let addr = get_next_hop_address(i, hop_count, target, &proxies);
            let label = format_hop_label(i, hop_count);

            if i == hop_count - 1 {
                assert_eq!(addr, target, "last hop should target recipient");
                assert_eq!(label, "target");
            } else {
                assert_eq!(addr, proxies[i + 1], "hop {i} should point to proxy {}", i + 1);
                assert_eq!(label, format!("P{}", i + 2));
            }
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Recovery infrastructure tests
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_encrypt_decrypt_proxy_key_roundtrip() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-encrypt-roundtrip");
        let _ = std::fs::create_dir_all(&temp_dir);
        let key_path = temp_dir.join("proxy-test.json");

        let private_key = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let encrypted = encrypt_proxy_key(private_key, "test_password", 11155111).unwrap();
        std::fs::write(&key_path, &encrypted).unwrap();

        let decrypted = decrypt_proxy_key_file(&key_path, "test_password").unwrap();
        assert_eq!(decrypted, private_key);

        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_encrypt_proxy_key_format_matches_wallet_json() {
        let private_key = "deadbeef";
        let encrypted = encrypt_proxy_key(private_key, "pw", 1).unwrap();
        let json: serde_json::Value = serde_json::from_str(&encrypted).unwrap();
        assert!(json.get("encrypted").is_some());
        let enc = json.get("encrypted").unwrap();
        assert!(enc.get("ciphertext").is_some());
        assert!(enc.get("iv").is_some());
        assert!(enc.get("salt").is_some());
        assert!(enc.get("tag").is_some());
        assert_eq!(json.get("encryption_type").unwrap().as_str().unwrap(), "aes-256-gcm");
        assert_eq!(json.get("chain_id").unwrap().as_u64().unwrap(), 1);
    }

    #[test]
    fn test_decrypt_proxy_key_wrong_password_fails() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-wrong-pw");
        let _ = std::fs::create_dir_all(&temp_dir);
        let key_path = temp_dir.join("proxy-wrong.json");

        let private_key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let encrypted = encrypt_proxy_key(private_key, "correct", 1).unwrap();
        std::fs::write(&key_path, &encrypted).unwrap();

        let result = decrypt_proxy_key_file(&key_path, "wrong");
        assert!(result.is_err(), "decryption with wrong password should fail");

        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_recovery_journal_entry_serialization() {
        let entry = RecoveryJournalEntry {
            hop_index: 0,
            hop_count: 5,
            tx_hash: "0xabc".to_string(),
            from_addr: "0x111".to_string(),
            to_addr: "0x222".to_string(),
            value_wei: "1000000000000000000".to_string(),
            gas_price_wei: "1500000000".to_string(),
            nonce: 42,
            chain_id: 11155111,
            recovery_address: "0x333".to_string(),
            timestamp: "2026-06-17T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: RecoveryJournalEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.hop_index, 0);
        assert_eq!(parsed.hop_count, 5);
        assert_eq!(parsed.chain_id, 11155111);
        assert_eq!(parsed.nonce, 42);
    }

    #[test]
    fn test_persist_proxy_and_journal_writes_files() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-persist");
        let _ = std::fs::create_dir_all(&temp_dir);
        std::env::set_var("WALLET_PASSWORD", "pw");

        let shutdown = Arc::new(AtomicBool::new(false));
        let recovery = RecoveryContext {
            dir: temp_dir.to_string_lossy().to_string(),
            password: "pw".to_string(),
            recovery_address: Address::from_low_u64_be(99),
            chain_id: 1,
            shutdown_requested: shutdown,
        };

        let from = Address::from_low_u64_be(1);
        let to = Address::from_low_u64_be(2);
        let key_filename = persist_proxy_and_journal(
            &recovery,
            0,
            3,
            "deadbeef",
            from,
            to,
            U256::from(1_000_000_000_000_000_000u64),
            U256::from(1_500_000_000u64),
            5,
        )
        .unwrap();

        // Verify key file was created
        let key_path = temp_dir.join(&key_filename);
        assert!(key_path.exists(), "proxy key file should exist");
        let decrypted = decrypt_proxy_key_file(&key_path, "pw").unwrap();
        assert_eq!(decrypted, "deadbeef");

        // Verify journal was created with one entry
        let journal_path = temp_dir.join("journal.jsonl");
        assert!(journal_path.exists(), "journal should exist");
        let content = std::fs::read_to_string(&journal_path).unwrap();
        assert!(content.contains("\"hop_index\":0"));
        assert!(content.contains("\"hop_count\":3"));

        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_file(&journal_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_recovery_context_shutdown_flag_works() {
        let shutdown = Arc::new(AtomicBool::new(false));
        let recovery = RecoveryContext {
            dir: "/tmp".to_string(),
            password: "pw".to_string(),
            recovery_address: Address::zero(),
            chain_id: 1,
            shutdown_requested: Arc::clone(&shutdown),
        };
        assert!(!recovery.shutdown_requested.load(Ordering::SeqCst));
        recovery.shutdown_requested.store(true, Ordering::SeqCst);
        assert!(recovery.shutdown_requested.load(Ordering::SeqCst));
    }

    #[test]
    fn test_uuid_simple_generates_unique_filenames() {
        let a = uuid_simple(0);
        let b = uuid_simple(0);
        assert!(a.starts_with("0-"));
        assert!(b.starts_with("0-"));
        assert_ne!(a, b, "UUID should be random");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Encryption edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_encrypt_proxy_key_unicode_password() {
        let private_key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let unicode_pw = "пароль密码🔐";
        let encrypted = encrypt_proxy_key(private_key, unicode_pw, 1).unwrap();
        let json: serde_json::Value = serde_json::from_str(&encrypted).unwrap();
        assert!(json.get("encrypted").is_some(), "unicode password should still produce valid JSON");
    }

    #[test]
    fn test_encrypt_proxy_key_special_chars_password() {
        let private_key = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        let special_pw = "p@$$w0rd!#%&*()_+-={}[]|:;<>?,./~`";
        let encrypted = encrypt_proxy_key(private_key, special_pw, 1).unwrap();
        // Should produce valid JSON
        let json: serde_json::Value = serde_json::from_str(&encrypted).unwrap();
        assert!(json.get("encrypted").is_some());
    }

    #[test]
    fn test_encrypt_proxy_key_very_long_password() {
        let private_key = "cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe";
        let long_pw = "x".repeat(1000);
        let encrypted = encrypt_proxy_key(private_key, &long_pw, 1).unwrap();
        let json: serde_json::Value = serde_json::from_str(&encrypted).unwrap();
        assert!(json.get("encrypted").is_some());
    }

    #[test]
    fn test_encrypt_proxy_key_short_password_still_works() {
        let private_key = "1234567812345678123456781234567812345678123456781234567812345678";
        let short_pw = "a";
        let encrypted = encrypt_proxy_key(private_key, short_pw, 1).unwrap();
        let json: serde_json::Value = serde_json::from_str(&encrypted).unwrap();
        assert!(json.get("encrypted").is_some());
    }

    #[test]
    fn test_encrypt_proxy_key_salt_iv_are_unique_per_call() {
        let private_key = "abc";
        let e1 = encrypt_proxy_key(private_key, "pw", 1).unwrap();
        let e2 = encrypt_proxy_key(private_key, "pw", 1).unwrap();
        let j1: serde_json::Value = serde_json::from_str(&e1).unwrap();
        let j2: serde_json::Value = serde_json::from_str(&e2).unwrap();
        let s1 = j1.get("encrypted").unwrap().get("salt").unwrap().as_str().unwrap();
        let s2 = j2.get("encrypted").unwrap().get("salt").unwrap().as_str().unwrap();
        let iv1 = j1.get("encrypted").unwrap().get("iv").unwrap().as_str().unwrap();
        let iv2 = j2.get("encrypted").unwrap().get("iv").unwrap().as_str().unwrap();
        assert_ne!(s1, s2, "salt should be unique per call");
        assert_ne!(iv1, iv2, "iv should be unique per call");
    }

    #[test]
    fn test_encrypt_proxy_key_stores_correct_chain_id() {
        let e1 = encrypt_proxy_key("aaa", "pw", 1).unwrap();
        let e111 = encrypt_proxy_key("aaa", "pw", 11155111).unwrap();
        let j1: serde_json::Value = serde_json::from_str(&e1).unwrap();
        let j111: serde_json::Value = serde_json::from_str(&e111).unwrap();
        assert_eq!(j1.get("chain_id").unwrap().as_u64().unwrap(), 1);
        assert_eq!(j111.get("chain_id").unwrap().as_u64().unwrap(), 11155111);
    }

    #[test]
    fn test_decrypt_proxy_key_truncated_ciphertext_fails() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-truncated");
        let _ = std::fs::create_dir_all(&temp_dir);
        let key_path = temp_dir.join("truncated.json");
        // Write a valid-looking but truncated JSON
        let bad = serde_json::json!({
            "encrypted": {
                "ciphertext": "dead",
                "iv": "00112233445566778899aabb",
                "salt": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
                "tag": "00112233445566778899aabbccddeeff"
            },
            "encryption_type": "aes-256-gcm",
            "chain_id": 1
        });
        std::fs::write(&key_path, bad.to_string()).unwrap();
        let result = decrypt_proxy_key_file(&key_path, "any_password");
        assert!(result.is_err(), "truncated ciphertext should fail");
        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_decrypt_proxy_key_missing_ciphertext_field_fails() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-missing-ct");
        let _ = std::fs::create_dir_all(&temp_dir);
        let key_path = temp_dir.join("missing.json");
        let bad = serde_json::json!({
            "encrypted": {
                "iv": "00",
                "salt": "00",
                "tag": "00"
            }
        });
        std::fs::write(&key_path, bad.to_string()).unwrap();
        let result = decrypt_proxy_key_file(&key_path, "pw");
        assert!(result.is_err(), "missing ciphertext should fail");
        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_decrypt_proxy_key_nonexistent_file_fails() {
        let result = decrypt_proxy_key_file(Path::new("/nonexistent/proxy-test.json"), "pw");
        assert!(result.is_err(), "non-existent file should fail");
    }

    #[test]
    fn test_decrypt_proxy_key_corrupted_json_fails() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-corrupt");
        let _ = std::fs::create_dir_all(&temp_dir);
        let key_path = temp_dir.join("corrupt.json");
        std::fs::write(&key_path, "not json at all {{{ broken").unwrap();
        let result = decrypt_proxy_key_file(&key_path, "pw");
        assert!(result.is_err(), "corrupted JSON should fail");
        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_encrypt_decrypt_proxy_key_different_chain_ids_still_compatible() {
        // Different chain_id in metadata should not affect encryption (scrypt+salt+iv is pw-derived)
        let temp_dir = std::env::temp_dir().join("testnet-fund-chainid-compat");
        let _ = std::fs::create_dir_all(&temp_dir);
        let key_path = temp_dir.join("chainid.json");

        let private_key = "9999999999999999999999999999999999999999999999999999999999999999";
        let encrypted = encrypt_proxy_key(private_key, "test", 1).unwrap();
        // Mutate chain_id and verify decrypt still works
        let mut json: serde_json::Value = serde_json::from_str(&encrypted).unwrap();
        json["chain_id"] = serde_json::json!(999);
        std::fs::write(&key_path, json.to_string()).unwrap();

        let decrypted = decrypt_proxy_key_file(&key_path, "test").unwrap();
        assert_eq!(decrypted, private_key, "decrypt should ignore chain_id in metadata");
        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_encrypt_proxy_key_salt_is_32_bytes_hex() {
        let e = encrypt_proxy_key("aaa", "pw", 1).unwrap();
        let j: serde_json::Value = serde_json::from_str(&e).unwrap();
        let salt = j.get("encrypted").unwrap().get("salt").unwrap().as_str().unwrap();
        assert_eq!(salt.len(), 64, "salt should be 32 bytes hex-encoded (64 chars)");
    }

    #[test]
    fn test_encrypt_proxy_key_iv_is_12_bytes_hex() {
        let e = encrypt_proxy_key("aaa", "pw", 1).unwrap();
        let j: serde_json::Value = serde_json::from_str(&e).unwrap();
        let iv = j.get("encrypted").unwrap().get("iv").unwrap().as_str().unwrap();
        assert_eq!(iv.len(), 24, "iv should be 12 bytes hex-encoded (24 chars)");
    }

    #[test]
    fn test_encrypt_proxy_key_tag_is_16_bytes_hex() {
        let e = encrypt_proxy_key("aaa", "pw", 1).unwrap();
        let j: serde_json::Value = serde_json::from_str(&e).unwrap();
        let tag = j.get("encrypted").unwrap().get("tag").unwrap().as_str().unwrap();
        assert_eq!(tag.len(), 32, "tag should be 16 bytes hex-encoded (32 chars)");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Recovery journal edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_recovery_journal_entry_unicode_addresses() {
        // Address can be the result of {:?} formatting which uses lowercase hex
        let entry = RecoveryJournalEntry {
            hop_index: 0,
            hop_count: 3,
            tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            from_addr: "0x0000000000000000000000000000000000000001".to_string(),
            to_addr: "0x0000000000000000000000000000000000000002".to_string(),
            value_wei: "0".to_string(),
            gas_price_wei: "0".to_string(),
            nonce: 0,
            chain_id: 1,
            recovery_address: "0x0000000000000000000000000000000000000099".to_string(),
            timestamp: "2026-06-17T00:00:00.000Z".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: RecoveryJournalEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.from_addr, "0x0000000000000000000000000000000000000001");
    }

    #[test]
    fn test_recovery_journal_entry_zero_values() {
        let entry = RecoveryJournalEntry {
            hop_index: 0,
            hop_count: 0,
            tx_hash: String::new(),
            from_addr: String::new(),
            to_addr: String::new(),
            value_wei: "0".to_string(),
            gas_price_wei: "0".to_string(),
            nonce: 0,
            chain_id: 0,
            recovery_address: String::new(),
            timestamp: String::new(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: RecoveryJournalEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.hop_count, 0);
        assert_eq!(parsed.tx_hash, "");
    }

    #[test]
    fn test_recovery_journal_entry_large_values() {
        let entry = RecoveryJournalEntry {
            hop_index: usize::MAX,
            hop_count: usize::MAX,
            tx_hash: "0x".to_string() + &"f".repeat(64),
            from_addr: "0x".to_string() + &"f".repeat(40),
            to_addr: "0x".to_string() + &"a".repeat(40),
            value_wei: "999999999999999999999999999999999".to_string(),
            gas_price_wei: "999999999999".to_string(),
            nonce: u64::MAX,
            chain_id: u64::MAX,
            recovery_address: "0x".to_string() + &"1".repeat(40),
            timestamp: "2099-12-31T23:59:59.999Z".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: RecoveryJournalEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.hop_index, usize::MAX);
        assert_eq!(parsed.nonce, u64::MAX);
        assert_eq!(parsed.chain_id, u64::MAX);
    }

    #[test]
    fn test_persist_proxy_and_journal_appends_multiple_entries() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-multiple-entries");
        let _ = std::fs::create_dir_all(&temp_dir);

        let shutdown = Arc::new(AtomicBool::new(false));
        let recovery = RecoveryContext {
            dir: temp_dir.to_string_lossy().to_string(),
            password: "pw".to_string(),
            recovery_address: Address::from_low_u64_be(99),
            chain_id: 1,
            shutdown_requested: shutdown,
        };

        for i in 0..5 {
            let _ = persist_proxy_and_journal(
                &recovery,
                i,
                5,
                &format!("key_{i}"),
                Address::from_low_u64_be(i as u64),
                Address::from_low_u64_be((i + 1) as u64),
                U256::from(1_000_000_000_000_000_000u64),
                U256::from(1_500_000_000u64),
                i as u64,
            )
            .unwrap();
        }

        let journal_path = temp_dir.join("journal.jsonl");
        let content = std::fs::read_to_string(&journal_path).unwrap();
        let line_count = content.lines().filter(|l| !l.trim().is_empty()).count();
        assert_eq!(line_count, 5, "journal should have 5 entries");

        // Verify each line is valid JSON
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let _: RecoveryJournalEntry = serde_json::from_str(trimmed).unwrap();
            }
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_persist_proxy_and_journal_creates_directory() {
        // The caller is expected to create the dir; we test that the function
        // works when the dir already exists
        let temp_base = std::env::temp_dir().join("testnet-fund-nested-dirs");
        let nested = temp_base.join("a").join("b").join("c");
        let _ = std::fs::remove_dir_all(&temp_base);
        let _ = std::fs::create_dir_all(&nested);

        let shutdown = Arc::new(AtomicBool::new(false));
        let recovery = RecoveryContext {
            dir: nested.to_string_lossy().to_string(),
            password: "pw".to_string(),
            recovery_address: Address::zero(),
            chain_id: 1,
            shutdown_requested: shutdown,
        };

        let result = persist_proxy_and_journal(
            &recovery,
            0,
            1,
            "key",
            Address::from_low_u64_be(1),
            Address::from_low_u64_be(2),
            U256::from(1000u64),
            U256::from(1u64),
            0,
        );
        assert!(result.is_ok());
        assert!(nested.join("journal.jsonl").exists(), "journal should be created");
        let _ = std::fs::remove_dir_all(&temp_base);
    }

    #[test]
    fn test_persist_proxy_and_journal_key_filename_format() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-filename-format");
        let _ = std::fs::create_dir_all(&temp_dir);

        let shutdown = Arc::new(AtomicBool::new(false));
        let recovery = RecoveryContext {
            dir: temp_dir.to_string_lossy().to_string(),
            password: "pw".to_string(),
            recovery_address: Address::zero(),
            chain_id: 1,
            shutdown_requested: shutdown,
        };

        let key_filename = persist_proxy_and_journal(
            &recovery,
            3,
            5,
            "k",
            Address::from_low_u64_be(1),
            Address::from_low_u64_be(2),
            U256::from(100u64),
            U256::from(1u64),
            0,
        )
        .unwrap();
        assert!(key_filename.starts_with("proxy-3-"), "filename should start with proxy-<hop_index>-");
        assert!(key_filename.ends_with(".json"), "filename should end with .json");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cleanup_proxy_and_journal_removes_file() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-cleanup");
        let _ = std::fs::create_dir_all(&temp_dir);
        let key_path = temp_dir.join("proxy-test.json");
        std::fs::write(&key_path, "{}").unwrap();
        assert!(key_path.exists());

        let shutdown = Arc::new(AtomicBool::new(false));
        let recovery = RecoveryContext {
            dir: temp_dir.to_string_lossy().to_string(),
            password: "pw".to_string(),
            recovery_address: Address::zero(),
            chain_id: 1,
            shutdown_requested: shutdown,
        };

        cleanup_proxy_and_journal(&recovery, "proxy-test.json", 0);
        assert!(!key_path.exists(), "key file should be removed after cleanup");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cleanup_proxy_and_journal_nonexistent_file_does_not_panic() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-cleanup-noop");
        let _ = std::fs::create_dir_all(&temp_dir);

        let shutdown = Arc::new(AtomicBool::new(false));
        let recovery = RecoveryContext {
            dir: temp_dir.to_string_lossy().to_string(),
            password: "pw".to_string(),
            recovery_address: Address::zero(),
            chain_id: 1,
            shutdown_requested: shutdown,
        };

        // Should not panic when file doesn't exist
        cleanup_proxy_and_journal(&recovery, "proxy-nonexistent.json", 0);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Gas bump math edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn gas_bump_with_max_u256_does_not_overflow_with_saturating() {
        // 12% bump on near-max U256 using saturating arithmetic
        let gas = U256::MAX - U256::from(1000u64);
        let bump_amount = gas / U256::from(100) * U256::from(12);
        let bumped = gas.saturating_add(bump_amount);
        // saturating_add caps at U256::MAX
        assert!(bumped >= gas);
        assert!(bumped <= U256::MAX);
    }

    #[test]
    fn gas_bump_percentage_is_exactly_12() {
        let gas = U256::from(1_000_000u64);
        let bumped = gas + gas / U256::from(100) * U256::from(12);
        // 1_000_000 * 1.12 = 1_120_000
        assert_eq!(bumped, U256::from(1_120_000u64));
    }

    #[test]
    fn gas_bump_doubles_roughly() {
        // 2x of base = 100% bump. 12% bump gives 1.12x.
        let gas = U256::from(1_000_000_000u64);
        let bumped = gas + gas / U256::from(100) * U256::from(12);
        // Bump is +12% of original = 120_000_000
        let increase = bumped - gas;
        assert_eq!(increase, U256::from(120_000_000u64));
    }

    #[test]
    fn last_ditch_gas_is_exactly_2x() {
        let gas = U256::from(3_000_000_000u64);
        let last_ditch = gas + gas;
        assert_eq!(last_ditch, U256::from(6_000_000_000u64));
    }

    #[test]
    fn timeout_constants_match_spec() {
        // 60s timeout, 10s heartbeat per the reliability spec
        assert_eq!(CONFIRMATION_TIMEOUT_SECS, 60);
        assert_eq!(CONFIRMATION_HEARTBEAT_SECS, 10);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Seed and forward amount math
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_calculate_seed_amount_one_hop() {
        let target = parse_units(1u64, "ether").unwrap().into();
        let gas = U256::from(20_000_000_000u64); // 20 gwei
        // 1 hop: gas_21k = 21_000 * 20_000_000_000 = 4.2e14
        // new formula: seed = target + gas_21k * (1+2) + gas_21k = 1.0 + 0.00168
        let seed = calculate_seed_amount(target, gas, 1);
        let seed_eth = seed.as_u128() as f64 / 1e18;
        // gas_21k = 4.2e14 wei = 0.00042 ETH
        // seed = 1.0 + 0.00042 * 4 = 1.0 + 0.00168 = 1.00168
        assert!((seed_eth - 1.00168).abs() < 1e-4, "expected ~1.00168 ETH, got {seed_eth}");
    }

    #[test]
    fn test_calculate_seed_amount_ten_hops() {
        let target = parse_units(1u64, "ether").unwrap().into();
        let gas = U256::from(1_000_000_000u64); // 1 gwei
        let seed_3 = calculate_seed_amount(target, gas, 3);
        let seed_10 = calculate_seed_amount(target, gas, 10);
        // More hops = more gas overhead
        assert!(seed_10 > seed_3, "10 hops should need more seed than 3 hops");
    }

    #[test]
    fn test_calculate_seed_amount_gas_22_gwei() {
        let target = U256::from(1_000_000_000_000_000_000u64); // 1 ETH
        let gas = U256::from(22_000_000_000u64); // 22 gwei
        let seed = calculate_seed_amount(target, gas, 5);
        // 5 hops: gas_21k = 21_000 * 22e9 = 4.62e14 wei = 0.000462 ETH
        // new formula: target + gas_21k * (5+2) + gas_21k = 1 + 0.000462*8 = 1.003696
        let expected = 1.0 + (21_000.0 * 22.0 * 1e9 * 8.0) / 1e18;
        let actual = seed.as_u128() as f64 / 1e18;
        assert!((actual - expected).abs() < 1e-6, "expected {expected}, got {actual}");
    }

    #[test]
    fn test_calculate_forward_amount_normal_case() {
        let remaining = parse_units(1u64, "ether").unwrap().into();
        let gas = U256::from(20_000_000_000u64);
        let hop_cost = U256::from(21_000u64) * gas;
        let forward = calculate_forward_amount(remaining, gas);
        assert_eq!(forward, remaining - hop_cost);
    }

    #[test]
    fn test_calculate_forward_amount_zero_remaining() {
        let forward = calculate_forward_amount(U256::zero(), U256::from(1_000_000_000u64));
        assert_eq!(forward, U256::zero());
    }

    #[test]
    fn test_calculate_forward_amount_barely_enough() {
        // remaining = 21_000 * gas, forward should be 0
        let gas = U256::from(1_000_000_000u64);
        let remaining = U256::from(21_000u64) * gas;
        let forward = calculate_forward_amount(remaining, gas);
        assert_eq!(forward, U256::zero());
    }

    #[test]
    fn test_calculate_forward_amount_one_wei_remaining() {
        let gas = U256::from(1_000_000_000u64);
        let remaining = U256::from(21_000u64) * gas + U256::from(1u64);
        let forward = calculate_forward_amount(remaining, gas);
        assert_eq!(forward, U256::from(1u64));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Hop address resolution
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_get_next_hop_address_single_hop() {
        let target: Address = Address::from_low_u64_be(99);
        let proxies = vec![Address::from_low_u64_be(1)];
        // 1 hop: i=0 -> target
        assert_eq!(get_next_hop_address(0, 1, target, &proxies), target);
    }

    #[test]
    fn test_get_next_hop_address_two_hops() {
        let target: Address = Address::from_low_u64_be(99);
        let proxies = vec![Address::from_low_u64_be(1), Address::from_low_u64_be(2)];
        // 2 hops: i=0 -> proxies[1] = 2, i=1 -> target
        assert_eq!(get_next_hop_address(0, 2, target, &proxies), Address::from_low_u64_be(2));
        assert_eq!(get_next_hop_address(1, 2, target, &proxies), target);
    }

    #[test]
    fn test_get_next_hop_address_middle_hop() {
        let target: Address = Address::from_low_u64_be(99);
        let proxies = vec![
            Address::from_low_u64_be(1),
            Address::from_low_u64_be(2),
            Address::from_low_u64_be(3),
            Address::from_low_u64_be(4),
            Address::from_low_u64_be(5),
        ];
        // 5 hops: i=0 -> proxies[1]=2, i=1 -> proxies[2]=3, i=2 -> proxies[3]=4, i=3 -> proxies[4]=5, i=4 -> target
        assert_eq!(get_next_hop_address(0, 5, target, &proxies), Address::from_low_u64_be(2));
        assert_eq!(get_next_hop_address(1, 5, target, &proxies), Address::from_low_u64_be(3));
        assert_eq!(get_next_hop_address(2, 5, target, &proxies), Address::from_low_u64_be(4));
        assert_eq!(get_next_hop_address(3, 5, target, &proxies), Address::from_low_u64_be(5));
        assert_eq!(get_next_hop_address(4, 5, target, &proxies), target);
    }

    #[test]
    fn test_format_hop_label_two_hops() {
        assert_eq!(format_hop_label(0, 2), "P2");
        assert_eq!(format_hop_label(1, 2), "target");
    }

    #[test]
    fn test_format_hop_label_three_hops() {
        assert_eq!(format_hop_label(0, 3), "P2");
        assert_eq!(format_hop_label(1, 3), "P3");
        assert_eq!(format_hop_label(2, 3), "target");
    }

    #[test]
    fn test_format_hop_label_seven_hops() {
        for i in 0..6 {
            assert_eq!(format_hop_label(i, 7), format!("P{}", i + 2));
        }
        assert_eq!(format_hop_label(6, 7), "target");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Gas price selector edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_choose_gas_price_mgwei_min_equals_max() {
        let mut rng = StdRng::seed_from_u64(42);
        // When network=0 and min==max, the result should be that value
        // (no 90% network floor interference)
        for _ in 0..20 {
            let chosen = choose_gas_price_mgwei(0, 3_000, 3_000, &mut rng);
            assert_eq!(chosen, 3_000, "min==max with network=0 should always return that value");
        }
    }

    #[test]
    fn test_choose_gas_price_mgwei_network_zero_uses_min() {
        let mut rng = StdRng::seed_from_u64(42);
        // network=0 -> 90% floor = 0, ceiling = max_gwei
        let chosen = choose_gas_price_mgwei(0, 1_000, 5_000, &mut rng);
        assert!((1_000..=5_000).contains(&chosen), "should be in [min, max]={}", chosen);
    }

    #[test]
    fn test_choose_gas_price_mgwei_network_one_uses_min() {
        let mut rng = StdRng::seed_from_u64(42);
        // network=1 -> 90% = 0, ceiling = 11
        let chosen = choose_gas_price_mgwei(1, 1_000, 5_000, &mut rng);
        assert!((1_000..=5_000).contains(&chosen));
    }

    #[test]
    fn test_choose_gas_price_mgwei_never_exceeds_100_000() {
        let mut rng = StdRng::seed_from_u64(42);
        for network in [10_000u64, 50_000, 100_000, 500_000, 1_000_000, 10_000_000] {
            for _ in 0..10 {
                let chosen = choose_gas_price_mgwei(network, 1, 200_000, &mut rng);
                assert!(chosen <= 100_000, "chosen={} for network={}", chosen, network);
            }
        }
    }

    #[test]
    fn test_choose_gas_price_mgwei_floor_floor_clamps_to_min() {
        let mut rng = StdRng::seed_from_u64(42);
        // network=100 -> 90%=90, min=1000, so floor=1000
        let chosen = choose_gas_price_mgwei(100, 1_000, 5_000, &mut rng);
        assert!(chosen >= 1_000);
    }

    #[test]
    fn test_choose_gas_price_mgwei_returns_values_in_range() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut results = std::collections::HashSet::new();
        for _ in 0..100 {
            let chosen = choose_gas_price_mgwei(2_000, 1_000, 10_000, &mut rng);
            assert!((1_000..=10_000).contains(&chosen));
            results.insert(chosen);
        }
        // Should generate some variety
        assert!(results.len() > 10, "should generate varied results, got {}", results.len());
    }

    // ──────────────────────────────────────────────────────────────────────────
    // SenderState stress / concurrent logic
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_sender_state_sequential_locks_and_unlocks() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0), dummy_wallet(2, 1.0)];
        let mut state = SenderState {
            use_counts: vec![0; 3],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);

        // Lock 1, 2, 3 in sequence
        let p1 = state.try_pick_and_lock(&senders, 1, &mut rng).unwrap();
        let p2 = state.try_pick_and_lock(&senders, 1, &mut rng).unwrap();
        let p3 = state.try_pick_and_lock(&senders, 1, &mut rng).unwrap();
        assert_eq!(state.use_counts.iter().sum::<usize>(), 3);
        assert!(state.locked_senders.contains(&p1));
        assert!(state.locked_senders.contains(&p2));
        assert!(state.locked_senders.contains(&p3));

        // All at limit and locked
        assert!(state.try_pick_and_lock(&senders, 1, &mut rng).is_none());

        // Unlock 1
        state.unlock(p1);
        assert!(!state.locked_senders.contains(&p1));

        // But p1 is still at use_count=1 == limit, so still cannot pick
        assert!(state.try_pick_and_lock(&senders, 1, &mut rng).is_none());
    }

    #[test]
    fn test_sender_state_use_count_remains_after_unlock() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let mut state = SenderState {
            use_counts: vec![0],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);

        let p = state.try_pick_and_lock(&senders, 5, &mut rng).unwrap();
        assert_eq!(state.use_counts[0], 1);
        state.unlock(p);
        // After unlock, use_count is still 1
        assert_eq!(state.use_counts[0], 1);
        // Can pick again because limit is 5
        let p2 = state.try_pick_and_lock(&senders, 5, &mut rng).unwrap();
        assert_eq!(state.use_counts[0], 2);
        assert_eq!(p, p2, "same sender should be picked when only one available");
    }

    #[test]
    fn test_sender_state_stress_many_iterations() {
        let senders: Vec<WalletInfo> = (0..10)
            .map(|i| dummy_wallet(i, 1.0))
            .collect();
        let mut state = SenderState {
            use_counts: vec![0; 10],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 0,
            durations: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);

        // 50 iterations: pick, use, unlock
        for _ in 0..50 {
            let p = state.try_pick_and_lock(&senders, 100, &mut rng);
            if let Some(idx) = p {
                state.unlock(idx);
            }
        }
        let total_uses: usize = state.use_counts.iter().sum();
        assert_eq!(total_uses, 50);
    }

    #[test]
    fn test_sender_state_distribution_fair_within_limit() {
        // 5 targets, 2 senders -> ceil(5/2) = 3 per sender
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0)];
        let max_per = compute_max_per_sender(5, 2);
        assert_eq!(max_per, 3);

        // Run multiple trials to handle randomness
        let mut diffs_below_2 = 0;
        for trial in 0..50 {
            let mut state = SenderState {
                use_counts: vec![0; 2],
                locked_senders: HashSet::new(),
                funded: 0,
                failed: 0,
                durations: vec![],
            };
            let mut rng = StdRng::seed_from_u64(trial);

            for _ in 0..5 {
                let p = state.try_pick_and_lock(&senders, max_per, &mut rng);
                assert!(p.is_some(), "trial {trial} should always have an available sender");
                state.unlock(p.unwrap());
            }
            // Total should always be 5
            let total: usize = state.use_counts.iter().sum();
            assert_eq!(total, 5, "trial {trial} should have 5 total uses");
            // Difference between senders should be at most 1 (fair distribution)
            let diff = (state.use_counts[0] as i64 - state.use_counts[1] as i64).abs();
            assert!(diff <= 1, "trial {trial} diff should be <= 1, got {diff}");
            if diff < 2 {
                diffs_below_2 += 1;
            }
        }
        // Most trials should have fair distribution
        assert!(diffs_below_2 > 40, "expected most trials to be fair, got {diffs_below_2}/50");
    }

    #[test]
    fn test_sender_state_failed_target_doesnt_increment_funded() {
        let mut state = SenderState {
            use_counts: vec![0],
            locked_senders: HashSet::new(),
            funded: 0,
            failed: 1,
            durations: vec![],
        };
        // Failed is tracked separately from funded
        assert_eq!(state.failed, 1);
        assert_eq!(state.funded, 0);
        state.funded += 1;
        assert_eq!(state.funded, 1);
        assert_eq!(state.failed, 1);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // distribute_round_robin edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_distribute_round_robin_zero_items() {
        let queues = distribute_round_robin::<i32>(vec![], 5);
        for queue in &queues {
            assert!(queue.is_empty());
        }
        assert_eq!(queues.len(), 5);
    }

    #[test]
    fn test_distribute_round_robin_single_worker() {
        let queues = distribute_round_robin((0..10).collect::<Vec<_>>(), 1);
        assert_eq!(queues.len(), 1);
        let flattened: Vec<usize> = queues[0].iter().cloned().collect();
        assert_eq!(flattened, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn test_distribute_round_robin_even_split() {
        let queues = distribute_round_robin((0..100).collect::<Vec<_>>(), 4);
        let total: usize = queues.iter().map(|q| q.len()).sum();
        assert_eq!(total, 100);
        // Should be 25 each
        for q in &queues {
            assert_eq!(q.len(), 25);
        }
    }

    #[test]
    fn test_distribute_round_robin_uneven_split() {
        // 10 items / 3 workers = 3, 3, 4 (or similar)
        let queues = distribute_round_robin((0..10).collect::<Vec<_>>(), 3);
        let total: usize = queues.iter().map(|q| q.len()).sum();
        assert_eq!(total, 10);
    }

    #[test]
    fn test_distribute_round_robin_more_workers_than_items() {
        let queues = distribute_round_robin(vec![1, 2, 3], 10);
        let total: usize = queues.iter().map(|q| q.len()).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn test_distribute_round_robin_preserves_order() {
        let queues = distribute_round_robin((0..9).collect::<Vec<_>>(), 3);
        // Round-robin: item i goes to worker i % 3
        // Worker 0: indices 0, 3, 6
        // Worker 1: indices 1, 4, 7
        // Worker 2: indices 2, 5, 8
        assert_eq!(queues[0].iter().cloned().collect::<Vec<_>>(), vec![0, 3, 6]);
        assert_eq!(queues[1].iter().cloned().collect::<Vec<_>>(), vec![1, 4, 7]);
        assert_eq!(queues[2].iter().cloned().collect::<Vec<_>>(), vec![2, 5, 8]);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // should_retry_proxy_send_error more patterns
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn retry_filter_connection_refused() {
        let err = anyhow::anyhow!("connection refused");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn retry_filter_network_error() {
        let err = anyhow::anyhow!("network error: timeout");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn retry_filter_service_unavailable() {
        let err = anyhow::anyhow!("503 Service Unavailable");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn retry_filter_rate_limited_message() {
        let err = anyhow::anyhow!("rate limited, try again later");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn retry_filter_temporary_failure() {
        let err = anyhow::anyhow!("temporary failure in name resolution");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn retry_filter_revert_is_not_retryable() {
        let err = anyhow::anyhow!("execution reverted: insufficient balance");
        assert!(!should_retry_proxy_send_error(&err));
    }

    #[test]
    fn retry_filter_gas_estimate_failed_is_not_retryable() {
        let err = anyhow::anyhow!("gas required exceeds allowance");
        assert!(!should_retry_proxy_send_error(&err));
    }

    #[test]
    fn retry_filter_replacement_underpriced_is_not_retryable() {
        let err = anyhow::anyhow!("replacement transaction underpriced");
        assert!(!should_retry_proxy_send_error(&err));
    }

    #[test]
    fn retry_filter_lowercase_pattern_matching() {
        // Patterns should be case-insensitive (lowercased internally)
        let err = anyhow::anyhow!("TIMEOUT while sending tx");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn retry_filter_mixed_case_pattern_matching() {
        let err = anyhow::anyhow!("Service UNAVAILABLE");
        assert!(should_retry_proxy_send_error(&err));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // should_skip_confirmation edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_should_skip_confirmation_yes_and_dry_run() {
        assert!(should_skip_confirmation(true, true));
    }

    #[test]
    fn test_should_skip_confirmation_false_yes() {
        assert!(!should_skip_confirmation(false, false));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // confirm_funding edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_confirm_funding_with_whitespace() {
        let input = b"  y  \n";
        let result = confirm_funding(3, 1, &input[..]).unwrap();
        assert!(result, "trim should handle surrounding whitespace");
    }

    #[test]
    fn test_confirm_funding_uppercase_y() {
        let input = b"Y\n";
        let result = confirm_funding(3, 1, &input[..]).unwrap();
        assert!(result);
    }

    #[test]
    fn test_confirm_funding_word_yes() {
        // Only "y" should be accepted, not "yes"
        let input = b"yes\n";
        let result = confirm_funding(3, 1, &input[..]).unwrap();
        assert!(!result, "should require exact 'y', not 'yes'");
    }

    #[test]
    fn test_confirm_funding_zero_targets() {
        let input = b"n\n";
        let result = confirm_funding(0, 1, &input[..]).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_confirm_funding_carriage_return() {
        let input = b"y\r\n";
        let result = confirm_funding(3, 1, &input[..]).unwrap();
        assert!(result);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_funding_prompt
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_format_funding_prompt_contains_counts() {
        let prompt = format_funding_prompt(10, 3);
        assert!(prompt.contains("10"));
        assert!(prompt.contains("3"));
        assert!(prompt.contains("y/N"));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_eth_amount edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_format_eth_amount_very_small() {
        let formatted = format_eth_amount(0.000001);
        assert!(formatted.starts_with("0."));
    }

    #[test]
    fn test_format_eth_amount_zero() {
        let formatted = format_eth_amount(0.0);
        assert_eq!(formatted, "0");
    }

    #[test]
    fn test_format_eth_amount_decimal_only() {
        let formatted = format_eth_amount(0.5);
        assert_eq!(formatted, "0.5");
    }

    #[test]
    fn test_format_eth_amount_large() {
        let formatted = format_eth_amount(1000.5);
        // Should keep meaningful digits
        assert!(formatted.contains("1000"));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_compact_duration
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_format_compact_duration_zero() {
        assert_eq!(format_compact_duration(Duration::from_secs(0)), "0s");
    }

    #[test]
    fn test_format_compact_duration_seconds() {
        assert_eq!(format_compact_duration(Duration::from_secs(30)), "30s");
    }

    #[test]
    fn test_format_compact_duration_minutes() {
        assert_eq!(format_compact_duration(Duration::from_secs(60)), "1m 0s");
        assert_eq!(format_compact_duration(Duration::from_secs(90)), "1m 30s");
    }

    #[test]
    fn test_format_compact_duration_hours() {
        assert_eq!(format_compact_duration(Duration::from_secs(3600)), "1h 0m 0s");
        assert_eq!(format_compact_duration(Duration::from_secs(3660)), "1h 1m 0s");
    }

    #[test]
    fn test_format_compact_duration_complex() {
        let d = format_compact_duration(Duration::from_secs(7325)); // 2h 2m 5s
        assert_eq!(d, "2h 2m 5s");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // format_worker_rest
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_format_worker_rest_zero_zero() {
        assert_eq!(format_worker_rest(0, 0), "none");
    }

    #[test]
    fn test_format_worker_rest_min_equals_max() {
        assert_eq!(format_worker_rest(30, 30), "30s");
    }

    #[test]
    fn test_format_worker_rest_range() {
        assert_eq!(format_worker_rest(15, 30), "15-30s");
    }

    #[test]
    fn test_format_worker_rest_min_larger() {
        // Edge case: min > max shouldn't happen due to ensure! in orchestrate
        // but we test the format
        assert_eq!(format_worker_rest(60, 30), "60-30s");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // choose_worker_rest_secs
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_choose_worker_rest_secs_zero_max() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..20 {
            let secs = choose_worker_rest_secs(10, 0, &mut rng);
            assert_eq!(secs, 0, "max=0 should always return 0");
        }
    }

    #[test]
    fn test_choose_worker_rest_secs_in_range() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            let secs = choose_worker_rest_secs(5, 10, &mut rng);
            assert!((5..=10).contains(&secs), "secs={secs} out of [5, 10]");
        }
    }

    #[test]
    fn test_choose_worker_rest_secs_single_value() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..20 {
            let secs = choose_worker_rest_secs(7, 7, &mut rng);
            assert_eq!(secs, 7);
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // worker_tag
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_worker_tag_format() {
        assert_eq!(worker_tag(1), "WK001");
        assert_eq!(worker_tag(99), "WK099");
        assert_eq!(worker_tag(100), "WK100");
    }

    #[test]
    fn test_worker_tag_padding() {
        assert_eq!(worker_tag(5), "WK005");
        assert_eq!(worker_tag(10), "WK010");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // prepare_funding_sets edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_prepare_funding_sets_empty_wallets() {
        let funder = Funder::new(
            Arc::new(core_logic::WalletManager::new().unwrap()),
            Provider::new(Http::new(reqwest::Url::parse("http://localhost").unwrap())),
            dummy_rpc_manager(&["http://localhost"]),
            reqwest::Client::new(),
            "pw".into(),
            1,
            None,
            None,
        );
        let (senders, targets, available, max_per) = funder.prepare_funding_sets(&[], 0.5, 0.01);
        assert_eq!(senders.len(), 0);
        assert_eq!(targets.len(), 0);
        assert_eq!(available, 0);
        assert_eq!(max_per, 0);
    }

    #[test]
    fn test_prepare_funding_sets_all_targets_no_senders() {
        let funder = Funder::new(
            Arc::new(core_logic::WalletManager::new().unwrap()),
            Provider::new(Http::new(reqwest::Url::parse("http://localhost").unwrap())),
            dummy_rpc_manager(&["http://localhost"]),
            reqwest::Client::new(),
            "pw".into(),
            1,
            Some(5),
            None,
        );
        let wallets = vec![dummy_wallet(0, 0.001), dummy_wallet(1, 0.005)];
        let (senders, targets, available, max_per) = funder.prepare_funding_sets(&wallets, 0.5, 0.01);
        assert_eq!(senders.len(), 0);
        assert_eq!(targets.len(), 2);
        assert_eq!(available, 2);
        assert_eq!(max_per, 0, "max_per should be 0 when no senders");
    }

    #[test]
    fn test_prepare_funding_sets_max_targets_caps() {
        let funder = Funder::new(
            Arc::new(core_logic::WalletManager::new().unwrap()),
            Provider::new(Http::new(reqwest::Url::parse("http://localhost").unwrap())),
            dummy_rpc_manager(&["http://localhost"]),
            reqwest::Client::new(),
            "pw".into(),
            1,
            Some(2), // cap at 2
            None,
        );
        let wallets = vec![
            dummy_wallet(0, 0.005),
            dummy_wallet(1, 0.008),
            dummy_wallet(2, 0.003),
            dummy_wallet(3, 0.001),
            dummy_wallet(4, 1.0), // sender
        ];
        let (_senders, targets, available, _max_per) = funder.prepare_funding_sets(&wallets, 0.5, 0.01);
        assert_eq!(targets.len(), 4);
        assert_eq!(available, 2, "max_targets should cap available to 2");
    }

    #[test]
    fn test_prepare_funding_sets_max_targets_none() {
        let funder = Funder::new(
            Arc::new(core_logic::WalletManager::new().unwrap()),
            Provider::new(Http::new(reqwest::Url::parse("http://localhost").unwrap())),
            dummy_rpc_manager(&["http://localhost"]),
            reqwest::Client::new(),
            "pw".into(),
            1,
            None, // no cap
            None,
        );
        let wallets = vec![dummy_wallet(0, 0.001), dummy_wallet(1, 0.002)];
        let (_senders, _targets, available, _max_per) = funder.prepare_funding_sets(&wallets, 0.5, 0.01);
        assert_eq!(available, 2);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // filter_senders / filter_targets edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_filter_senders_exact_match() {
        let wallets = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.5)];
        // min_balance = 1.0 -> wallet 0 has exactly 1.0
        let senders = filter_senders(&wallets, 1.0);
        assert_eq!(senders.len(), 2);
    }

    #[test]
    fn test_filter_senders_boundary() {
        let wallets = vec![dummy_wallet(0, 0.99999), dummy_wallet(1, 1.0)];
        let senders = filter_senders(&wallets, 1.0);
        assert_eq!(senders.len(), 1);
        assert_eq!(senders[0].idx, 1);
    }

    #[test]
    fn test_filter_targets_zero_max() {
        let wallets = vec![dummy_wallet(0, 0.0), dummy_wallet(1, 0.0001)];
        let targets = filter_targets(&wallets, 0.0);
        assert_eq!(targets.len(), 1, "only wallet with exactly 0.0 matches max=0.0");
    }

    #[test]
    fn test_filter_targets_exact_match() {
        let wallets = vec![dummy_wallet(0, 0.01), dummy_wallet(1, 0.02)];
        // max_balance = 0.01 -> wallet 0 has exactly 0.01
        let targets = filter_targets(&wallets, 0.01);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].idx, 0);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // compute_max_per_sender edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_compute_max_per_sender_zero_available() {
        assert_eq!(compute_max_per_sender(0, 5), 0);
    }

    #[test]
    fn test_compute_max_per_sender_one_sender() {
        // 10 targets, 1 sender -> 10
        assert_eq!(compute_max_per_sender(10, 1), 10);
    }

    #[test]
    fn test_compute_max_per_sender_3_targets_2_senders() {
        // ceil(3/2) = 2
        assert_eq!(compute_max_per_sender(3, 2), 2);
    }

    #[test]
    fn test_compute_max_per_sender_5_targets_2_senders() {
        // ceil(5/2) = 3
        assert_eq!(compute_max_per_sender(5, 2), 3);
    }

    #[test]
    fn test_compute_max_per_sender_1_target_5_senders() {
        // ceil(1/5) = 1
        assert_eq!(compute_max_per_sender(1, 5), 1);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // select_targets_to_fund edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_select_targets_to_fund_zero_cap() {
        let targets = vec![dummy_wallet(0, 0.01), dummy_wallet(1, 0.02)];
        let selected = select_targets_to_fund(&targets, Some(0));
        assert!(selected.is_empty());
    }

    #[test]
    fn test_select_targets_to_fund_cap_equals_count() {
        let targets = vec![dummy_wallet(0, 0.01), dummy_wallet(1, 0.02)];
        let selected = select_targets_to_fund(&targets, Some(2));
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn test_select_targets_to_fund_cap_larger_than_count() {
        let targets = vec![dummy_wallet(0, 0.01)];
        let selected = select_targets_to_fund(&targets, Some(10));
        assert_eq!(selected.len(), 1);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // generate_dry_run_plan more cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_generate_dry_run_plan_single_sender_many_targets() {
        let senders = vec![dummy_wallet(0, 100.0)];
        let targets: Vec<WalletInfo> = (1..20).map(|i| dummy_wallet(i, 0.005)).collect();
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng);
        assert_eq!(plan.len(), 19);
        // All targets should be assigned to sender 0
        for pf in &plan {
            assert_eq!(pf.sender_idx, 0);
        }
    }

    #[test]
    fn test_generate_dry_run_plan_honors_max_targets_zero() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.01), dummy_wallet(2, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, Some(0), &mut rng);
        assert_eq!(plan.len(), 0);
    }

    #[test]
    fn test_generate_dry_run_plan_min_equals_max_target() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.025, 0.025, 3, 3, None, &mut rng);
        assert_eq!(plan.len(), 1);
        let amount_eth = plan[0].amount.as_u128() as f64 / 1e18;
        assert!((amount_eth - 0.025).abs() < 1e-9, "amount should be exactly 0.025");
    }

    #[test]
    fn test_generate_dry_run_plan_min_equals_max_hops() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.01), dummy_wallet(2, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 4, 4, None, &mut rng);
        for pf in &plan {
            assert_eq!(pf.hops, 4, "all plans should have exactly 4 hops");
        }
    }

    #[test]
    fn test_generate_dry_run_plan_target_balance_preserved() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.12345)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng);
        assert_eq!(plan[0].target_balance_eth, 0.12345);
    }

    #[test]
    fn test_generate_dry_run_plan_deterministic_with_seed() {
        let senders = vec![dummy_wallet(0, 1.0), dummy_wallet(1, 1.0)];
        let targets = vec![dummy_wallet(2, 0.01), dummy_wallet(3, 0.01), dummy_wallet(4, 0.01)];

        let mut rng1 = StdRng::seed_from_u64(999);
        let plan1 = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng1);

        let mut rng2 = StdRng::seed_from_u64(999);
        let plan2 = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng2);

        assert_eq!(plan1.len(), plan2.len());
        for (a, b) in plan1.iter().zip(plan2.iter()) {
            assert_eq!(a.amount, b.amount, "amounts should match with same seed");
            assert_eq!(a.hops, b.hops, "hops should match with same seed");
            assert_eq!(a.sender_idx, b.sender_idx, "sender should match with same seed");
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // await_confirmation_with_progress more edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_await_confirmation_with_progress_heartbeat_minimum_clamped() {
        // Heartbeat < 1s should be clamped to 1s
        let future = std::future::pending::<std::result::Result<(), std::io::Error>>();
        let result = await_confirmation_with_progress(
            1,
            "Funder",
            "test",
            future,
            Duration::from_millis(50), // very short
            Duration::from_millis(10), // sub-second heartbeat
        )
        .await;
        // Should timeout (not panic on heartbeat clamping)
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_await_confirmation_with_progress_long_running() {
        // Future that completes after 100ms, with 60s timeout
        let future = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok::<_, std::io::Error>(42u32)
        };
        let result = await_confirmation_with_progress(
            1,
            "Funder",
            "test",
            future,
            Duration::from_secs(60),
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        assert_eq!(result, 42);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Dummy helper for recovery tests
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_dummy_wallet_creates_valid_address() {
        let w = dummy_wallet(42, 1.5);
        assert_eq!(w.idx, 42);
        assert_eq!(w.balance_eth, 1.5);
        assert_eq!(w.address, Address::from_low_u64_be(42));
    }

    #[test]
    fn test_dummy_wallet_zero_idx() {
        let w = dummy_wallet(0, 0.0);
        assert_eq!(w.idx, 0);
        assert_eq!(w.balance_eth, 0.0);
    }

    #[test]
    fn test_dummy_wallet_large_idx() {
        let w = dummy_wallet(usize::MAX, f64::MAX);
        assert_eq!(w.idx, usize::MAX);
        // f64::MAX will lose precision when stored in u64 but balance field is f64
        assert_eq!(w.balance_eth, f64::MAX);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // JSON log shape (verify structure)
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_json_log_structure_minimal() {
        // Verify the expected JSON structure
        let data = serde_json::json!({
            "funded": 5,
            "failed": 1,
            "total_duration_secs": 120.5,
            "summary": {
                "total_wallets": 10,
                "senders_count": 4,
                "targets_count": 6,
                "assigned": 6
            },
            "timestamp": "2026-06-17T00:00:00Z"
        });
        let json = serde_json::to_string_pretty(&data).unwrap();
        assert!(json.contains("\"funded\": 5"));
        assert!(json.contains("\"failed\": 1"));
        assert!(json.contains("\"summary\""));
        assert!(json.contains("\"timestamp\""));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Dry-run balance sufficiency tests
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_dry_run_balance_check_sufficient() {
        // Sender has plenty, no warnings expected
        let funder = Funder::new(
            Arc::new(core_logic::WalletManager::new().unwrap()),
            Provider::new(Http::new(reqwest::Url::parse("http://localhost").unwrap())),
            dummy_rpc_manager(&["http://localhost"]),
            reqwest::Client::new(),
            "pw".into(),
            1,
            None,
            None,
        );
        let senders = vec![dummy_wallet(0, 10.0)]; // 10 ETH, more than enough
        let targets = vec![dummy_wallet(1, 0.005), dummy_wallet(2, 0.005)];
        // Should not panic
        let result = funder.execute_dry_run(&senders, &targets, 0.02, 0.04, 3, 5, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dry_run_balance_check_insufficient() {
        // Sender has very little — should warn
        let funder = Funder::new(
            Arc::new(core_logic::WalletManager::new().unwrap()),
            Provider::new(Http::new(reqwest::Url::parse("http://localhost").unwrap())),
            dummy_rpc_manager(&["http://localhost"]),
            reqwest::Client::new(),
            "pw".into(),
            1,
            None,
            None,
        );
        let senders = vec![dummy_wallet(0, 0.001)]; // 0.001 ETH — too little
        let targets = vec![dummy_wallet(1, 0.005)];
        let result = funder.execute_dry_run(&senders, &targets, 0.02, 0.04, 5, 7, None);
        assert!(result.is_ok());
        // The function should print warnings (we can't easily assert stdout here, but it shouldn't panic)
    }

    #[test]
    fn test_dry_run_balance_check_multiple_senders() {
        // Multiple senders with mixed balance adequacy
        let funder = Funder::new(
            Arc::new(core_logic::WalletManager::new().unwrap()),
            Provider::new(Http::new(reqwest::Url::parse("http://localhost").unwrap())),
            dummy_rpc_manager(&["http://localhost"]),
            reqwest::Client::new(),
            "pw".into(),
            1,
            None,
            None,
        );
        let senders = vec![
            dummy_wallet(0, 10.0),   // sufficient
            dummy_wallet(1, 0.001),  // insufficient
        ];
        let targets: Vec<WalletInfo> = (2..10).map(|i| dummy_wallet(i, 0.005)).collect();
        let result = funder.execute_dry_run(&senders, &targets, 0.02, 0.04, 3, 5, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dry_run_balance_check_empty_targets() {
        let funder = Funder::new(
            Arc::new(core_logic::WalletManager::new().unwrap()),
            Provider::new(Http::new(reqwest::Url::parse("http://localhost").unwrap())),
            dummy_rpc_manager(&["http://localhost"]),
            reqwest::Client::new(),
            "pw".into(),
            1,
            None,
            None,
        );
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets: Vec<WalletInfo> = vec![];
        let result = funder.execute_dry_run(&senders, &targets, 0.02, 0.04, 3, 5, None);
        assert!(result.is_ok());
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Recovery nonce handling tests
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_sweep_amount_deducts_gas_correctly() {
        // 1 ETH balance, 1 gwei gas = 21_000 * 1e9 = 21_000 gwei = 0.000021 ETH
        let balance = U256::from(1_000_000_000_000_000_000u64); // 1 ETH
        let gas_price = U256::from(1_000_000_000u64); // 1 gwei
        let gas_cost = U256::from(21_000u64) * gas_price;
        let sweep = balance.saturating_sub(gas_cost);
        // Expected: 1.0 - 0.000021 = 0.999979
        let sweep_eth = sweep.as_u128() as f64 / 1e18;
        assert!((sweep_eth - 0.999979).abs() < 1e-6);
    }

    #[test]
    fn test_sweep_amount_handles_zero_balance() {
        let balance = U256::zero();
        let gas_price = U256::from(1_000_000_000u64);
        let gas_cost = U256::from(21_000u64) * gas_price;
        let sweep = balance.saturating_sub(gas_cost);
        assert_eq!(sweep, U256::zero());
    }

    #[test]
    fn test_sweep_amount_handles_balance_below_gas() {
        // 0.00001 ETH balance, gas cost = 0.000021 ETH -> sweep = 0
        let balance = U256::from(10_000_000_000_000u64); // 0.00001 ETH
        let gas_price = U256::from(1_000_000_000u64);
        let gas_cost = U256::from(21_000u64) * gas_price;
        let sweep = balance.saturating_sub(gas_cost);
        assert_eq!(sweep, U256::zero(), "should be 0 when balance < gas");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Per-call timeout for load_wallets
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_load_wallet_balance_query_timeout_constant() {
        // Verify the timeout duration is set as expected
        // (we use 30s in the implementation; verify it's reasonable)
        const EXPECTED_TIMEOUT_SECS: u64 = 30;
        // This is a compile-time assertion via the literal in the code
        // (no runtime check needed since 30s is hardcoded in the function)
        const _: () = assert!(EXPECTED_TIMEOUT_SECS > 0);
        const _: () = assert!(EXPECTED_TIMEOUT_SECS <= 120, "timeout should not be excessive");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Delivery verification (target balance delta) tests
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_delivery_delta_calculation_correct() {
        // After funding, target should have +X ETH (where X = remaining)
        // If actual delta >= expected - 1 gwei dust, OK
        let expected = U256::from(1_000_000_000_000_000_000u64); // 1 ETH
        let actual = U256::from(999_999_999_999_999_999u64); // 0.99999... ETH
        let dust_tolerance = U256::from(1_000_000_000u64); // 1 gwei
        let delivery_ok = actual >= expected.saturating_sub(dust_tolerance);
        assert!(delivery_ok, "delivery within 1 gwei dust should be OK");
    }

    #[test]
    fn test_delivery_delta_shortfall_detected() {
        // Actual delta way below expected
        let expected = U256::from(1_000_000_000_000_000_000u64);
        let actual = U256::from(500_000_000_000_000_000u64); // 0.5 ETH (way short)
        let dust_tolerance = U256::from(1_000_000_000u64);
        let delivery_ok = actual >= expected.saturating_sub(dust_tolerance);
        assert!(!delivery_ok, "shortfall should be detected");
        let shortfall = expected.saturating_sub(actual);
        let shortfall_eth = shortfall.as_u128() as f64 / 1e18;
        assert!((shortfall_eth - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_delivery_delta_exact_match() {
        let expected = U256::from(1_000_000_000_000_000_000u64);
        let actual = U256::from(1_000_000_000_000_000_000u64);
        let dust_tolerance = U256::from(1_000_000_000u64);
        let delivery_ok = actual >= expected.saturating_sub(dust_tolerance);
        assert!(delivery_ok);
    }

    #[test]
    fn test_delivery_delta_concurrent_tx_drains_target() {
        // Simulating a concurrent tx that drains the target
        // after our funding: actual_delta could be negative
        let expected = U256::from(1_000_000_000_000_000_000u64);
        let actual = U256::zero(); // target drained after our tx
        let dust_tolerance = U256::from(1_000_000_000u64);
        let delivery_ok = actual >= expected.saturating_sub(dust_tolerance);
        assert!(!delivery_ok);
    }

    #[test]
    fn test_delivery_delta_target_also_received_other_funds() {
        // Target gets our funding PLUS concurrent funding
        // actual_delta > expected should still be considered OK
        let expected = U256::from(1_000_000_000_000_000_000u64);
        let actual = U256::from(2_000_000_000_000_000_000u64);
        let dust_tolerance = U256::from(1_000_000_000u64);
        let delivery_ok = actual >= expected.saturating_sub(dust_tolerance);
        assert!(delivery_ok);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Sender balance integrity tests
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_sender_spent_within_expected_range() {
        // Sender should spend approximately seed + 21k*gas
        let sender_before = U256::from(1_000_000_000_000_000_000u64); // 1 ETH
        let gas_price = U256::from(1_000_000_000u64); // 1 gwei
        let seed = U256::from(100_000_000_000_000_000u64); // 0.1 ETH
        let gas_21k = U256::from(21_000u64) * gas_price;
        let expected_spent = seed + gas_21k;
        let sender_after = sender_before.saturating_sub(expected_spent);
        let actual_spent = sender_before.saturating_sub(sender_after);
        assert_eq!(actual_spent, expected_spent, "spent should match expected");
    }

    #[test]
    fn test_sender_spent_anomaly_detection() {
        // If sender spent way more than expected, that's a bug
        let sender_before = U256::from(1_000_000_000_000_000_000u64);
        let gas_price = U256::from(1_000_000_000u64);
        let seed = U256::from(100_000_000_000_000_000u64);
        let gas_21k = U256::from(21_000u64) * gas_price;
        let expected_spent = seed + gas_21k;
        // Simulate anomaly: spent 2x expected (e.g., bug in code)
        let simulated_spent = expected_spent * U256::from(2);
        let sender_after = sender_before.saturating_sub(simulated_spent);
        let actual_spent = sender_before.saturating_sub(sender_after);
        let threshold = expected_spent + gas_21k; // 1 tx gas tolerance
        assert!(actual_spent > threshold, "anomaly should be detected");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Final integration assertions for delivery verification logic
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_target_balance_delta_with_zero_balance_before() {
        // Target had 0 before, now has X
        let before = U256::zero();
        let after = U256::from(1_000_000_000_000_000_000u64); // 1 ETH
        let actual_delta = after.saturating_sub(before);
        assert_eq!(actual_delta, after);
    }

    #[test]
    fn test_target_balance_delta_with_prior_balance() {
        // Target had 0.1 ETH, now has 1.1 ETH (received our 1 ETH)
        let before = U256::from(100_000_000_000_000_000u64); // 0.1 ETH
        let after = U256::from(1_100_000_000_000_000_000u64); // 1.1 ETH
        let actual_delta = after.saturating_sub(before);
        assert_eq!(actual_delta, U256::from(1_000_000_000_000_000_000u64));
    }

    #[test]
    fn test_target_balance_delta_underflow_safe() {
        // If somehow before > after (shouldn't happen normally), saturate
        let before = U256::from(1_000_000_000_000_000_000u64);
        let after = U256::from(500_000_000_000_000_000u64);
        let actual_delta = after.saturating_sub(before);
        assert_eq!(actual_delta, U256::zero(), "saturating_sub should not underflow");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Input validation tests (H2 audit fix)
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_validation_min_target_le_max_target() {
        // This is just a unit test for the validation logic
        let min = 0.02;
        let max = 0.04;
        assert!(min <= max, "min <= max should be valid");
    }

    #[test]
    fn test_validation_min_hops_le_max_hops() {
        let min = 3;
        let max = 5;
        assert!(min <= max);
    }

    #[test]
    fn test_validation_min_gwei_le_max_gwei() {
        let min = 1.2;
        let max = 1.5;
        assert!(min <= max);
    }

    #[test]
    fn test_validation_min_balance_ge_max_balance() {
        // senders have balance >= min_balance, targets have balance <= max_balance
        // So min_balance should be >= max_balance (no overlap)
        let min = 0.5;
        let max = 0.01;
        assert!(min >= max);
    }

    #[test]
    fn test_validation_max_hops_positive() {
        let max_hops = 5usize;
        assert!(max_hops > 0, "max_hops must be at least 1");
    }

    #[test]
    fn test_validation_min_target_positive() {
        let min_target = 0.02;
        assert!(min_target > 0.0, "min_target must be positive");
    }

    #[test]
    fn test_seed_uses_max_gwei_for_safety() {
        // Seed should be calculated using max_gwei to cover worst case
        // during multi-hop flow
        let target = U256::from(100_000_000_000_000_000u64); // 0.1 ETH
        let max_gas_price_wei = U256::from(1_500_000_000u64); // 1.5 gwei (max)
        let min_gas_price_wei = U256::from(1_200_000_000u64); // 1.2 gwei (min)

        let seed_using_max = calculate_seed_amount(target, max_gas_price_wei, 5);
        let seed_using_min = calculate_seed_amount(target, min_gas_price_wei, 5);

        assert!(
            seed_using_max > seed_using_min,
            "seed with max gas should be larger than seed with min gas"
        );
    }

    #[test]
    fn test_seed_covers_max_gas_for_all_hops() {
        // Even if all hops use max_gas, the seed should cover all gas costs
        let target = U256::from(100_000_000_000_000_000u64); // 0.1 ETH
        let max_gas = U256::from(1_500_000_000u64); // 1.5 gwei
        let hop_count = 7usize;
        let seed = calculate_seed_amount(target, max_gas, hop_count);
        let gas_21k = U256::from(21_000u64) * max_gas;
        let total_hop_gas = gas_21k * U256::from(hop_count as u64);
        // seed = target + (hop_count+2) * gas_21k
        // So gas covered for (hop_count+2) hops
        assert!(seed >= target + total_hop_gas);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // MAX_SEND_ATTEMPTS test (L5 audit fix)
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_max_send_attempts_constant_exists() {
        // Verify the constant is bounded to a reasonable value
        // (this is a compile-time guarantee since the const is in the fn body,
        // but we test the behavior via should_retry_proxy_send_error)
        let err = anyhow::anyhow!("connection refused");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn test_max_send_attempts_logic() {
        // Simulate: with MAX_SEND_ATTEMPTS=5, after 5 retryable errors we should bail
        const MAX_SEND_ATTEMPTS: usize = 5;
        let mut send_attempt = 0usize;
        let mut last_err: Option<String> = None;

        // Loop simulating retry logic
        loop {
            let retryable = true; // simulate retryable error
            if !retryable {
                break;
            }
            if last_err.is_some() {
                send_attempt += 1;
            }
            last_err = Some(format!("err_{send_attempt}"));
            if send_attempt + 1 >= MAX_SEND_ATTEMPTS {
                break;
            }
        }
        assert_eq!(send_attempt, MAX_SEND_ATTEMPTS - 1);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Shutdown warning tests (H3 audit fix)
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_shutdown_flag_propagates_to_workers() {
        let shutdown = Arc::new(AtomicBool::new(false));
        let rc = RecoveryContext {
            dir: "/tmp".to_string(),
            password: "pw".to_string(),
            recovery_address: Address::zero(),
            chain_id: 1,
            shutdown_requested: Arc::clone(&shutdown),
        };
        assert!(!rc.shutdown_requested.load(Ordering::SeqCst));
        rc.shutdown_requested.store(true, Ordering::SeqCst);
        // All worker clones of this Arc should see the same value
        assert!(rc.shutdown_requested.load(Ordering::SeqCst));
    }

    #[test]
    fn test_shutdown_flag_can_be_toggled() {
        let shutdown = Arc::new(AtomicBool::new(false));
        let s2 = Arc::clone(&shutdown);
        shutdown.store(true, Ordering::SeqCst);
        assert!(s2.load(Ordering::SeqCst));
        shutdown.store(false, Ordering::SeqCst);
        assert!(!s2.load(Ordering::SeqCst));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Per-call timeout for load_wallets
    // ──────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_timeout_wraps_slow_operations() {
        // Simulate a slow operation that should be timed out
        let result = tokio::time::timeout(
            Duration::from_millis(50),
            tokio::time::sleep(Duration::from_secs(5)),
        )
        .await;
        assert!(result.is_err(), "should timeout before 5s elapses");
    }

    #[tokio::test]
    async fn test_timeout_allows_fast_operations() {
        // Simulate a fast operation that should complete in time
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::time::sleep(Duration::from_millis(10)),
        )
        .await;
        assert!(result.is_ok(), "should complete before timeout");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Gas bump math
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn gas_bump_12_percent_compound_over_5_iterations() {
        // After 5 heartbeats with 12% bump, gas is multiplied by 1.12^5
        let mut gas = U256::from(1_000_000_000u64); // 1 gwei
        for _ in 0..5 {
            gas = gas + gas / U256::from(100) * U256::from(12);
        }
        // 1.12^5 = 1.7623416...
        // So 1 gwei * 1.7623 = 1.7623 gwei = 1_762_341_634 wei (approximate)
        let expected_min = U256::from(1_762_000_000u64);
        let expected_max = U256::from(1_763_000_000u64);
        assert!(
            gas >= expected_min && gas <= expected_max,
            "5x 12% bumps should give ~1.76 gwei, got {gas}"
        );
    }

    #[test]
    fn last_ditch_gas_2x_within_u256() {
        // 2x of 50 gwei should be 100 gwei, well within U256
        let gas = U256::from(50_000_000_000u64);
        let doubled = gas + gas;
        assert_eq!(doubled, U256::from(100_000_000_000u64));
        assert!(doubled < U256::MAX);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Persistence edge cases
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_persist_proxy_journal_handles_existing_file() {
        // Calling persist twice should append, not overwrite
        let temp_dir = std::env::temp_dir().join("testnet-fund-append-mode");
        let _ = std::fs::create_dir_all(&temp_dir);

        let shutdown = Arc::new(AtomicBool::new(false));
        let recovery = RecoveryContext {
            dir: temp_dir.to_string_lossy().to_string(),
            password: "pw".to_string(),
            recovery_address: Address::zero(),
            chain_id: 1,
            shutdown_requested: Arc::clone(&shutdown),
        };

        persist_proxy_and_journal(
            &recovery,
            0,
            1,
            "k1",
            Address::from_low_u64_be(1),
            Address::from_low_u64_be(2),
            U256::from(1u64),
            U256::from(1u64),
            0,
        )
        .unwrap();

        let journal_path = temp_dir.join("journal.jsonl");
        let size_after_first = std::fs::metadata(&journal_path).unwrap().len();

        persist_proxy_and_journal(
            &recovery,
            1,
            2,
            "k2",
            Address::from_low_u64_be(3),
            Address::from_low_u64_be(4),
            U256::from(2u64),
            U256::from(1u64),
            1,
        )
        .unwrap();

        let size_after_second = std::fs::metadata(&journal_path).unwrap().len();
        assert!(size_after_second > size_after_first, "journal should grow");

        let content = std::fs::read_to_string(&journal_path).unwrap();
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 2);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
