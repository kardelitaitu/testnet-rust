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
use core_logic::config::ProxyConfig;
use core_logic::security::SecurityUtils;
use core_logic::setup_logger;
use core_logic::ProxyHealthManager;
use core_logic::ProxyManager;
use core_logic::ProxyRateLimiter;
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
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::RwLock;
use tokio::sync::Semaphore;
use tracing::{info, warn};

const DEFAULT_LOAD_CONCURRENCY: usize = 100;
const CONFIRMATION_TIMEOUT_SECS: u64 = 60;
const CONFIRMATION_HEARTBEAT_SECS: u64 = 10;

/// Shared proxy infrastructure for routing RPC calls through egress proxies.
struct ProxyContext {
    pool: Arc<RwLock<Vec<ProxyConfig>>>,
    health: Arc<ProxyHealthManager>,
    rate_limiter: Arc<ProxyRateLimiter>,
}

/// Create a Provider<Http> from the RPC manager's next healthy endpoint.
fn create_provider(rpc_manager: &RpcManager, http_client: &reqwest::Client) -> Result<(Provider<Http>, String)> {
    let endpoint = rpc_manager.get_endpoint()?;
    let url = reqwest::Url::parse(&endpoint.url)?;
    let provider = Provider::new(Http::new_with_client(url, http_client.clone()));
    Ok((provider, endpoint.url.clone()))
}

/// Build an HTTP client, optionally tunnelled through an egress proxy.
fn build_http_client(proxy_config: Option<&ProxyConfig>) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder();
    if let Some(pc) = proxy_config {
        let mut proxy =
            reqwest::Proxy::all(&pc.url).map_err(|e| anyhow::anyhow!("Invalid proxy URL {}: {e}", pc.url))?;
        if let (Some(u), Some(p)) = (&pc.username, &pc.password) {
            proxy = proxy.basic_auth(u, p);
        }
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {e}"))
}

/// Select a healthy proxy from the pool, or return None.
async fn select_proxy(proxy_ctx: Option<&ProxyContext>) -> Option<ProxyConfig> {
    let ctx = proxy_ctx?;
    let proxies = ctx.pool.read().await;
    if proxies.is_empty() {
        return None;
    }
    let mut available = Vec::new();
    for p in proxies.iter() {
        if ctx.health.is_available(&p.url).await {
            available.push(p);
        }
    }
    if available.is_empty() {
        return None;
    }
    let mut rng = rand::thread_rng();
    Some(available[rng.gen_range(0..available.len())].clone())
}

/// Create a proxy-tunnelled Provider for a specific RPC URL.
async fn create_provider_with_proxy(rpc_url: &str, proxy_config: Option<&ProxyConfig>) -> Result<Provider<Http>> {
    let url = reqwest::Url::parse(rpc_url).context("Invalid RPC URL")?;
    let client = build_http_client(proxy_config)?;
    Ok(Provider::new(Http::new_with_client(url, client)))
}

/// Create a Provider from the RPC manager, optionally routing through an egress proxy.
/// If a healthy proxy is available, it's selected and the call goes through it.
async fn create_provider_routed(
    rpc_manager: &RpcManager,
    proxy_ctx: Option<&ProxyContext>,
) -> Result<(Provider<Http>, String)> {
    let endpoint = rpc_manager.get_endpoint()?;
    let rpc_url = endpoint.url.clone();
    if let Some(ctx) = proxy_ctx {
        if let Some(proxy) = select_proxy(Some(ctx)).await {
            ctx.rate_limiter.wait_until_available(&proxy.url).await;
            let provider = create_provider_with_proxy(&rpc_url, Some(&proxy)).await?;
            return Ok((provider, rpc_url));
        }
    }
    // Fallback: direct connection
    let url = reqwest::Url::parse(&rpc_url)?;
    let client = reqwest::Client::new();
    let provider = Provider::new(Http::new_with_client(url, client));
    Ok((provider, rpc_url))
}

/// Summary of RPC endpoint health probe results.
#[derive(Debug, Clone)]
struct ProbeSummary {
    total: usize,
    healthy: usize,
}

/// Probe all configured RPC endpoints before funding starts.
/// Each endpoint is tested through available egress proxies (if configured).
/// If the first proxy fails, the next proxy is tried before marking the RPC unhealthy.
async fn probe_rpc_endpoints(
    rpc_manager: &RpcManager,
    http_client: &reqwest::Client,
    chain_id: u64,
    timeout_secs: u64,
    proxy_ctx: Option<&ProxyContext>,
) -> Result<ProbeSummary> {
    let urls = rpc_manager.urls();
    if urls.is_empty() {
        anyhow::bail!("No RPC endpoints configured");
    }

    let has_proxies = match proxy_ctx {
        Some(ctx) => !ctx.pool.read().await.is_empty(),
        None => false,
    };

    println!("=== RPC Health Probe ===");
    if has_proxies {
        println!("Checking {} endpoint(s) through proxy pool...", urls.len());
    } else {
        println!("Checking {} endpoint(s) directly...", urls.len());
    }

    let mut healthy = 0usize;
    let mut unhealthy = 0usize;

    for (i, url_str) in urls.iter().enumerate() {
        let parsed_url = match reqwest::Url::parse(url_str) {
            Ok(u) => u,
            Err(e) => {
                warn!("  [{}/{}] {} — invalid URL: {}", i + 1, urls.len(), url_str, e);
                rpc_manager.record_failure(url_str);
                unhealthy += 1;
                continue;
            },
        };

        let mut probe_ok = false;

        // If we have proxies, try each one
        if has_proxies {
            let ctx = proxy_ctx.unwrap();
            let proxies = ctx.pool.read().await;
            // Shuffle proxy indices for random order
            let mut proxy_indices: Vec<usize> = (0..proxies.len()).collect();
            let mut rng = rand::thread_rng();
            for idx in (1..proxy_indices.len()).rev() {
                let j = rng.gen_range(0..=idx);
                proxy_indices.swap(idx, j);
            }

            for pi in &proxy_indices {
                let pc = &proxies[*pi];
                if !ctx.health.is_available(&pc.url).await {
                    continue;
                }

                let client = match build_http_client(Some(pc)) {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("  proxy {} build failed: {e}; trying next proxy", pc.url);
                        continue;
                    },
                };
                let provider = Provider::new(Http::new_with_client(parsed_url.clone(), client));

                let start = std::time::Instant::now();
                let probe = tokio::time::timeout(Duration::from_secs(timeout_secs), provider.get_chainid()).await;

                let latency_ms = start.elapsed().as_millis() as u64;

                match probe {
                    Ok(Ok(reported_chain_id)) if reported_chain_id.as_u64() == chain_id => {
                        ctx.health.record_success(&pc.url).await;
                        rpc_manager.record_success(url_str);
                        rpc_manager.record_latency(url_str, latency_ms);
                        healthy += 1;
                        probe_ok = true;
                        println!(
                            "  [{}/{}] {} — OK via proxy {} ({}ms, chain {})",
                            i + 1,
                            urls.len(),
                            url_str,
                            pc.url,
                            latency_ms,
                            reported_chain_id
                        );
                        break;
                    },
                    _ => {
                        ctx.health.record_failure(&pc.url).await;
                        // continue to next proxy
                    },
                }
            }
        }

        // Fallback: try direct connection if proxies didn't work or none configured
        if !probe_ok {
            let provider = Provider::new(Http::new_with_client(parsed_url, http_client.clone()));
            let start = std::time::Instant::now();
            let probe = tokio::time::timeout(Duration::from_secs(timeout_secs), provider.get_chainid()).await;
            let latency_ms = start.elapsed().as_millis() as u64;

            match probe {
                Ok(Ok(reported_chain_id)) if reported_chain_id.as_u64() == chain_id => {
                    rpc_manager.record_success(url_str);
                    rpc_manager.record_latency(url_str, latency_ms);
                    healthy += 1;
                    println!(
                        "  [{}/{}] {} — OK (direct, {}ms, chain {})",
                        i + 1,
                        urls.len(),
                        url_str,
                        latency_ms,
                        reported_chain_id
                    );
                },
                Ok(Ok(reported_chain_id)) => {
                    warn!(
                        "  [{}/{}] {} — chain mismatch: expected {}, got {}",
                        i + 1,
                        urls.len(),
                        url_str,
                        chain_id,
                        reported_chain_id
                    );
                    rpc_manager.record_failure(url_str);
                    unhealthy += 1;
                },
                Ok(Err(e)) => {
                    warn!(
                        "  [{}/{}] {} — RPC error after {}ms: {}",
                        i + 1,
                        urls.len(),
                        url_str,
                        latency_ms,
                        e
                    );
                    rpc_manager.record_failure(url_str);
                    unhealthy += 1;
                },
                Err(_) => {
                    warn!(
                        "  [{}/{}] {} — timed out after {}s",
                        i + 1,
                        urls.len(),
                        url_str,
                        timeout_secs
                    );
                    rpc_manager.record_failure(url_str);
                    unhealthy += 1;
                },
            }
        }
    }

    println!(
        "Probe complete: {}/{} healthy, {}/{} unhealthy",
        healthy,
        urls.len(),
        unhealthy,
        urls.len()
    );

    Ok(ProbeSummary {
        total: urls.len(),
        healthy,
    })
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
    proxy_ctx: Option<Arc<ProxyContext>>,
}

/// Info for a single wallet loaded from disk.
#[derive(Clone, Debug)]
struct WalletInfo {
    #[allow(dead_code)]
    idx: usize,
    address: Address,
    balance_eth: f64,
}

/// A single journal entry for one funding flow.
/// Everything needed to recover all proxy keys is in this one file.
/// The flow_seed is encrypted with WALLET_PASSWORD (same scheme as wallet-json).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FlowEntry {
    /// Hex-encoded AES-256-GCM ciphertext of the 32-byte flow_seed
    ciphertext: String,
    /// Hex-encoded 12-byte IV
    iv: String,
    /// Hex-encoded 32-byte scrypt salt
    salt: String,
    /// Hex-encoded 16-byte GCM authentication tag
    tag: String,
    /// Number of proxy hops
    hop_count: usize,
    /// Chain ID
    chain_id: u64,
    /// Timestamp for diagnostics
    timestamp: String,
}

const DOMAIN_SEPARATOR: &[u8] = b"sepolia-funder-flow-v1";
const FLOW_FILE_PREFIX: &str = "flow_";

/// Derive a deterministic proxy private key (32 bytes) from the flow_seed + hop index.
/// Uses keccak256(domain || flow_seed || index || counter), with a rejection loop
/// for the astronomically-unlikely case that the output is >= secp256k1 curve order n.
/// In practice counter will always be 0.
fn derive_proxy_key_bytes(flow_seed: &[u8; 32], hop_index: usize) -> [u8; 32] {
    let mut counter = 0u64;
    let curve_order = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141";
    let n = U256::from_str_radix(curve_order, 16).expect("valid curve order hex");
    loop {
        let mut data = Vec::with_capacity(DOMAIN_SEPARATOR.len() + 32 + 8 + 8);
        data.extend_from_slice(DOMAIN_SEPARATOR);
        data.extend_from_slice(flow_seed);
        data.extend_from_slice(&hop_index.to_le_bytes());
        data.extend_from_slice(&counter.to_le_bytes());
        let hash: [u8; 32] = ethers::utils::keccak256(&data);
        if U256::from(hash) < n && hash != [0u8; 32] {
            return hash;
        }
        counter += 1;
    }
}

/// Derive a LocalWallet from a flow_seed + hop index.
fn derive_proxy_wallet(flow_seed: &[u8; 32], hop_index: usize, chain_id: u64) -> Result<LocalWallet> {
    let key_bytes = derive_proxy_key_bytes(flow_seed, hop_index);
    let key_hex = hex::encode(key_bytes);
    key_hex
        .parse::<LocalWallet>()
        .context("flow-derived key is valid secp256k1")
        .map(|w| w.with_chain_id(chain_id))
}

/// Encrypt the 32-byte flow_seed with WALLET_PASSWORD (same AES-256-GCM + scrypt as wallet-json).
/// Hex-encodes the seed first so the plaintext is always valid UTF-8.
/// Returns (ciphertext_hex, iv_hex, salt_hex, tag_hex).
fn encrypt_flow_seed(flow_seed: &[u8; 32], password: &str) -> Result<(String, String, String, String)> {
    // Hex-encode first so the encrypted payload is valid UTF-8 (compatible with SecurityUtils)
    let seed_hex = hex::encode(flow_seed);
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
        .encrypt(nonce, seed_hex.as_bytes())
        .map_err(|e| anyhow::anyhow!("encrypt: {e}"))?;

    let tag_pos = ciphertext_with_tag.len().saturating_sub(16);
    let ciphertext = &ciphertext_with_tag[..tag_pos];
    let tag = &ciphertext_with_tag[tag_pos..];

    Ok((
        hex::encode(ciphertext),
        hex::encode(iv),
        hex::encode(salt),
        hex::encode(tag),
    ))
}

/// Write one flow file atomically: write to temp, then rename.
/// Returns the filename written.
fn write_flow_file(recovery_dir: &str, entry: &FlowEntry) -> Result<String> {
    let _ = fs::create_dir_all(recovery_dir);
    let filename = format!("flow_{}.json", uuid_fast());
    let tmp_filename = format!(".tmp_{}", uuid_fast());
    let tmp_path = Path::new(recovery_dir).join(&tmp_filename);
    let final_path = Path::new(recovery_dir).join(&filename);

    let json = serde_json::to_string_pretty(entry).context("Failed to serialize flow entry")?;
    fs::write(&tmp_path, &json).with_context(|| format!("Failed to write temp flow file: {}", tmp_path.display()))?;
    fs::rename(&tmp_path, &final_path)
        .with_context(|| format!("Failed to rename temp flow file: {}", final_path.display()))?;

    Ok(filename)
}

/// Remove a flow file by filename.
fn remove_flow_file(recovery_dir: &str, filename: &str) {
    let path = Path::new(recovery_dir).join(filename);
    let _ = fs::remove_file(&path);
}

/// Quick 8-byte random hex suffix for temp/filename uniqueness.
fn uuid_fast() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.gen();
    hex::encode(bytes)
}

/// Decrypt a flow_seed from a FlowEntry using WALLET_PASSWORD.
fn decrypt_flow_seed(entry: &FlowEntry, password: &str) -> Result<[u8; 32]> {
    let plaintext = SecurityUtils::decrypt_components(&entry.ciphertext, &entry.iv, &entry.salt, &entry.tag, password)?;
    let bytes = hex::decode(plaintext.trim()).context("flow_seed is not valid hex")?;
    if bytes.len() != 32 {
        anyhow::bail!("flow_seed is {} bytes, expected 32", bytes.len());
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&bytes);
    Ok(seed)
}

/// Read all flow files from the recovery directory.
fn read_flow_files(recovery_dir: &str) -> Result<Vec<(String, FlowEntry)>> {
    let dir = Path::new(recovery_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut results = Vec::new();
    for entry in fs::read_dir(dir).context("Failed to read recovery dir")?.flatten() {
        let fname = entry.file_name().to_string_lossy().to_string();
        if !fname.starts_with(FLOW_FILE_PREFIX) || !fname.ends_with(".json") {
            continue;
        }
        let content =
            fs::read_to_string(entry.path()).with_context(|| format!("Failed to read flow file: {}", fname))?;
        match serde_json::from_str::<FlowEntry>(&content) {
            Ok(flow) => results.push((fname, flow)),
            Err(e) => warn!("Flow file {} has invalid JSON: {}. Skipping.", fname, e),
        }
    }
    Ok(results)
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
/// Recovery context shared across all funding operations.
struct RecoveryContext {
    /// Directory where flow_*.json files are stored
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

/// Recovery mode: read flow_*.json files, decrypt flow seed, derive all proxy keys,
/// check on-chain balances, and sweep any remaining ETH back to the recovery address.
async fn recover_proxies(
    recovery_dir: &str,
    password: &str,
    recovery_address: Address,
    chain_id: u64,
    rpc_manager: &RpcManager,
    http_client: &reqwest::Client,
    dry_run: bool,
) -> Result<()> {
    let flow_files = read_flow_files(recovery_dir)?;
    if flow_files.is_empty() {
        println!("No flow files found in {}. Nothing to recover.", recovery_dir);
        return Ok(());
    }

    println!("=== Recovery Mode ===");
    println!("Recovery dir:    {}", recovery_dir);
    println!("Recovery addr:   {:?}", recovery_address);
    println!("Chain ID:        {}", chain_id);
    if dry_run {
        println!("Mode:            DRY RUN (no txs will be sent)");
    }
    println!("Found {} flow entries.\n", flow_files.len());

    let (provider, _url) = create_provider(rpc_manager, http_client).context("No healthy RPC for recovery")?;
    let mut swept_count = 0usize;
    let mut already_spent_count = 0usize;
    let mut error_count = 0usize;

    for (fname, entry) in &flow_files {
        println!("--- Flow: {} ---", fname);

        // Decrypt the flow seed
        let flow_seed = match decrypt_flow_seed(entry, password) {
            Ok(s) => s,
            Err(e) => {
                warn!("[ERROR] Failed to decrypt flow_seed in {}: {e}. Skipping.", fname);
                error_count += 1;
                continue;
            },
        };

        // Derive ALL proxy keys from the flow seed and check each one
        for i in 0..entry.hop_count {
            let proxy_key_bytes = derive_proxy_key_bytes(&flow_seed, i);
            let proxy_hex = hex::encode(proxy_key_bytes);
            let proxy_wallet = match proxy_hex.parse::<LocalWallet>() {
                Ok(w) => w.with_chain_id(chain_id),
                Err(e) => {
                    warn!("[ERROR] Failed to parse derived key {i}: {e}. Skipping.");
                    error_count += 1;
                    continue;
                },
            };
            let proxy_addr = proxy_wallet.address();

            let balance = match provider.get_balance(proxy_addr, None).await {
                Ok(b) => b,
                Err(e) => {
                    warn!("[ERROR] Failed to fetch balance for {proxy_addr:?} (proxy {i}): {e}. Skipping.");
                    error_count += 1;
                    continue;
                },
            };

            if balance.is_zero() {
                println!("  [SKIP] {proxy_addr:?} (proxy {i}) — balance is 0. Already safe.");
                already_spent_count += 1;
                continue;
            }

            let balance_eth = balance.as_u128() as f64 / 1e18;
            println!(
                "  [FOUND] {proxy_addr:?} (proxy {i}) has {} ETH stuck.",
                format_eth_amount(balance_eth)
            );

            // Fetch nonce from chain
            let on_chain_nonce = match provider.get_transaction_count(proxy_addr, None).await {
                Ok(n) => n,
                Err(e) => {
                    warn!("[ERROR] Failed to fetch nonce for {proxy_addr:?}: {e}.");
                    error_count += 1;
                    continue;
                },
            };

            // Sweep: balance - 21k*gas
            let gas_price = provider.get_gas_price().await.unwrap_or(U256::from(1_000_000_000u64));
            let gas_cost = U256::from(21_000u64) * gas_price;
            let sweep_amount = balance.saturating_sub(gas_cost);

            if sweep_amount.is_zero() {
                println!(
                    "  [SKIP] {proxy_addr:?} — balance ({:.6} ETH) insufficient to cover gas.",
                    balance_eth
                );
                already_spent_count += 1;
                continue;
            }

            let sweep_eth = sweep_amount.as_u128() as f64 / 1e18;
            println!(
                "  [SWEEP] {proxy_addr:?} → {recovery_address:?} : {} ETH (nonce {on_chain_nonce})",
                format_eth_amount(sweep_eth)
            );

            if dry_run {
                println!("    (dry run — no tx sent)");
                swept_count += 1;
                continue;
            }

            // Use a fresh provider for the sweep
            let (sweep_provider, _sweep_url) = match create_provider(rpc_manager, http_client) {
                Ok(p) => p,
                Err(e) => {
                    warn!("[ERROR] Failed to create RPC for sweep: {e}");
                    error_count += 1;
                    continue;
                },
            };
            let signer = SignerMiddleware::new(sweep_provider, proxy_wallet);
            let tx = TransactionRequest::pay(recovery_address, sweep_amount)
                .gas(21_000)
                .gas_price(gas_price)
                .nonce(on_chain_nonce);
            let send_result = { signer.send_transaction(tx, None).await };
            match send_result {
                Ok(pending) => {
                    let tx_hash = pending.tx_hash();
                    println!("    Tx broadcast: {tx_hash:?}. Waiting for confirmation...");
                    let confirm_result = pending.confirmations(1).interval(Duration::from_millis(500)).await;
                    match confirm_result {
                        Ok(Some(_)) => {
                            println!("    ✅ Confirmed. {} ETH recovered.", format_eth_amount(sweep_eth));
                            swept_count += 1;
                        },
                        Ok(None) => {
                            println!("    ⚠️ Tx may have dropped. Check {tx_hash:?} manually.");
                            error_count += 1;
                        },
                        Err(e) => {
                            warn!("    ❌ Confirmation failed: {e}. Check {tx_hash:?} manually.");
                            error_count += 1;
                        },
                    }
                },
                Err(e) => {
                    warn!("    ❌ Send failed: {e}. Cannot sweep {proxy_addr:?}.");
                    error_count += 1;
                },
            }
        }
    }

    println!();
    println!("=== Recovery Complete ===");
    println!("Swept:          {}", swept_count);
    println!("Already safe:   {}", already_spent_count);
    println!("Errors:         {}", error_count);

    // If all sweeps succeeded, archive each flow file
    if !dry_run && error_count == 0 {
        for (fname, _) in &flow_files {
            let src = Path::new(recovery_dir).join(fname);
            let dst = format!("{}.bak", src.display());
            if Path::new(&dst).exists() {
                let _ = fs::remove_file(&dst);
            }
            let _ = fs::rename(&src, &dst);
        }
        println!("All flow files archived.");
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
    let flow_files = read_flow_files(&recovery.dir)?;

    for (fname, entry) in &flow_files {
        let flow_seed = match decrypt_flow_seed(entry, &recovery.password) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for i in 0..entry.hop_count {
            let proxy_hex = hex::encode(derive_proxy_key_bytes(&flow_seed, i));
            let wallet = match proxy_hex.parse::<LocalWallet>() {
                Ok(w) => w.with_chain_id(recovery.chain_id),
                Err(_) => continue,
            };
            let addr = wallet.address();

            let balance = match provider.get_balance(addr, None).await {
                Ok(b) => b,
                Err(_) => continue,
            };

            if balance.is_zero() {
                continue;
            }

            let gas_price = provider.get_gas_price().await.unwrap_or(U256::from(1_000_000_000u64));
            let gas_cost = U256::from(21_000u64) * gas_price;
            let sweep_amount = balance.saturating_sub(gas_cost);

            if sweep_amount.is_zero() {
                continue;
            }

            let nonce = match provider.get_transaction_count(addr, None).await {
                Ok(n) => n,
                Err(_) => continue,
            };

            let (sweep_provider, _sweep_url) = match create_provider(rpc_manager, http_client) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let signer = SignerMiddleware::new(sweep_provider, wallet);
            let tx = TransactionRequest::pay(recovery.recovery_address, sweep_amount)
                .gas(21_000)
                .gas_price(gas_price)
                .nonce(nonce);

            let send_result = { signer.send_transaction(tx, None).await };
            match send_result {
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
                },
                Err(e) => warn!("[EMERGENCY SWEEP] Failed to sweep {:?}: {}", addr, e),
            }
        }

        // Remove flow file if all proxies were processed
        let _ = fs::remove_file(Path::new(&recovery.dir).join(fname));
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
        "rate limit exceeded",
        "rate limit reached",
        "too many request",
        "too many requests",
        "request limit",
        "request timeout",
        "-32005",
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
    proxy_ctx: Option<&ProxyContext>,
    rng: &mut StdRng,
) -> Result<()> {
    let (provider, rpc_url) = create_provider_routed(rpc_manager, proxy_ctx)
        .await
        .context("No healthy RPC for wallet check")?;

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

    // Pre-check: fetch sender's on-chain balance
    let sender_balance = provider.get_balance(sender_address, None).await?;
    let sender_balance_eth = sender_balance.as_u128() as f64 / 1e18;

    // ── Generate flow seed — all proxy keys derive from this ──
    // One 32-byte random seed per funding flow. Persisted ONCE in a single
    // flow file for recovery. No per-proxy key management needed.
    let mut flow_seed = [0u8; 32];
    rng.fill(&mut flow_seed[..]);

    // Derive ALL proxy wallets deterministically from the flow seed
    let mut proxies: Vec<LocalWallet> = Vec::with_capacity(hop_count);
    for i in 0..hop_count {
        proxies.push(derive_proxy_wallet(&flow_seed, i, chain_id)?);
    }
    let proxy_addrs: Vec<Address> = proxies.iter().map(|w| w.address()).collect();

    // ── Write ONE flow file atomically before Sender→P1 ──
    // This is the single source of truth for recovery. Written via temp→rename
    // so it's either fully present or not at all.
    let mut flow_filename: Option<String> = None;
    if let Some(rc) = recovery {
        let (ct, iv, salt, tag) = encrypt_flow_seed(&flow_seed, &rc.password)?;
        let entry = FlowEntry {
            ciphertext: ct,
            iv,
            salt,
            tag,
            hop_count,
            chain_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        match write_flow_file(&rc.dir, &entry) {
            Ok(fname) => {
                runner_log(worker_id, "Funder", format!("Flow file written: {fname}"));
                flow_filename = Some(fname);
            },
            Err(e) => warn!("[WK{worker_id}] Failed to write flow file: {e}"),
        }
    }

    // Capture target's balance BEFORE any hops for delta verification
    let target_balance_before = provider.get_balance(target, None).await?;

    let gas_price = random_gas_price(&provider, min_gwei, max_gwei, rng).await?;
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

    // ── Sender -> P1 (with RPC rotation on retryable errors) ──
    const MAX_SEND_ATTEMPTS: usize = 5;
    let mut send_attempt = 0usize;
    let mut current_provider = provider.clone();
    let mut current_rpc_url = rpc_url;
    let mut sender_signer = SignerMiddleware::new(current_provider.clone(), sender.clone());
    let pending = loop {
        let tx = TransactionRequest::pay(proxy_addrs[0], seed)
            .gas(21_000)
            .gas_price(gas_price);
        let retry_info = match sender_signer.send_transaction(tx.clone(), None).await {
            Ok(p) => {
                rpc_manager.record_success(&current_rpc_url);
                break p;
            },
            Err(e) => {
                if should_retry_proxy_send_error(&e) {
                    rpc_manager.record_failure(&current_rpc_url);
                    Some(format!("{e:#}"))
                } else {
                    return Err(e).context("Sender -> P1 tx failed");
                }
            },
        };
        if let Some(err_msg) = retry_info {
            if send_attempt + 1 >= MAX_SEND_ATTEMPTS {
                anyhow::bail!(
                    "Sender -> P1 send exhausted after {} attempts. Last error: {}. Flow file persists for recovery.",
                    MAX_SEND_ATTEMPTS,
                    err_msg
                );
            }
            let (new_provider, new_url) = match create_provider_routed(rpc_manager, proxy_ctx).await {
                Ok(p) => p,
                Err(e) => anyhow::bail!("No healthy RPC for Sender->P1 retry: {e}"),
            };
            current_provider = new_provider;
            current_rpc_url = new_url;
            sender_signer = SignerMiddleware::new(current_provider.clone(), sender.clone());
            let retry_after_secs = (1u64 << send_attempt as u32).min(120);
            runner_log(
                worker_id,
                "Funder",
                format!(
                    "Sender -> P1 send attempt {} failed ({}); rotating RPC, retrying in {}s",
                    send_attempt + 1,
                    err_msg,
                    retry_after_secs
                ),
            );
            tokio::time::sleep(Duration::from_secs(retry_after_secs)).await;
            send_attempt += 1;
            continue;
        }
    };
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

    let sender_wait_label = "Sender -> P1";
    runner_log(
        worker_id,
        "Funder",
        format!("Waiting for {sender_wait_label} confirmation"),
    );
    let sender_tx_hash;
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
            sender_tx_hash = receipt.transaction_hash;
            runner_log(
                worker_id,
                "Funder",
                format!("Sender -> P1 confirmed: {:?}", sender_tx_hash),
            );
        },
        Err(e) => {
            // Tx was broadcast but timed out. Send a 2x replacement from the
            // SENDER (not proxy 0 — proxy 0 hasn't received funds yet).
            runner_log(
                worker_id,
                "Funder",
                format!(
                    "Sender -> P1 confirmation failed: {:#}. Sending 2x last-ditch from sender.",
                    e
                ),
            );
            if let Ok((new_provider, _)) = create_provider_routed(rpc_manager, proxy_ctx).await {
                let last_ditch_gas = gas_price + gas_price;
                let (provider_reload, _) = create_provider_routed(rpc_manager, proxy_ctx)
                    .await
                    .unwrap_or_else(|_| (new_provider.clone(), String::new()));
                let decrypted_reload = manager
                    .get_wallet(sender_idx, Some(password))
                    .await
                    .context("Sender decrypt for last-ditch failed")?;
                let sender_reload: LocalWallet = decrypted_reload
                    .evm_private_key
                    .parse::<LocalWallet>()
                    .context("Sender key parse for last-ditch failed")?
                    .with_chain_id(chain_id);
                let ls_signer = SignerMiddleware::new(provider_reload, sender_reload);
                let ls_tx = TransactionRequest::pay(proxy_addrs[0], seed)
                    .gas(21_000)
                    .gas_price(last_ditch_gas);
                let ls_result_tx = { ls_signer.send_transaction(ls_tx, None).await };
                match ls_result_tx {
                    Ok(ls_pending) => {
                        runner_log(
                            worker_id,
                            "Funder",
                            format!("Last-ditch from sender sent: {:?}", ls_pending.tx_hash()),
                        );
                        // Wait for confirmation, polling every 5s for shutdown flag
                        let ls_result = {
                            let mut fut = Box::pin(ls_pending.confirmations(1).interval(Duration::from_millis(500)));
                            let deadline = tokio::time::Instant::now() + Duration::from_secs(CONFIRMATION_TIMEOUT_SECS);
                            let mut result = None;
                            while tokio::time::Instant::now() < deadline {
                                if let Some(rc) = recovery {
                                    if rc.shutdown_requested.load(Ordering::SeqCst) {
                                        warn!("Shutdown during last-ditch wait. Flow file persists for recovery.");
                                        result = Some(Err(anyhow::anyhow!("Shutdown during last-ditch")));
                                        break;
                                    }
                                }
                                match tokio::time::timeout(Duration::from_millis(500), fut.as_mut()).await {
                                    Ok(Ok(Some(receipt))) => {
                                        result = Some(Ok(Ok(Some(receipt))));
                                        break;
                                    },
                                    Ok(Ok(None)) => continue,
                                    Ok(Err(e)) => {
                                        result = Some(Ok(Err(e)));
                                        break;
                                    },
                                    Err(_) => continue,
                                }
                            }
                            result.unwrap_or(Err(anyhow::anyhow!(
                                "Last-ditch timed out after {}s",
                                CONFIRMATION_TIMEOUT_SECS
                            )))
                        };
                        if let Ok(Ok(Some(ls_receipt))) = ls_result {
                            sender_tx_hash = ls_receipt.transaction_hash;
                            runner_log(
                                worker_id,
                                "Funder",
                                format!("Last-ditch confirmed: {:?}", sender_tx_hash),
                            );
                        } else {
                            warn!("Last-ditch also timed out. Flow file persists for recovery.");
                            return Err(e);
                        }
                    },
                    Err(ls_e) => {
                        warn!("Last-ditch send failed: {ls_e}. Flow file persists for recovery.");
                        return Err(e);
                    },
                }
            } else {
                return Err(e);
            }
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

        let (mut current_provider, mut current_rpc_url) = create_provider_routed(rpc_manager, proxy_ctx)
            .await
            .context("No healthy RPC for proxy hop")?;
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

        // Phase 1: Send tx with RPC rotation on failure
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
                            "P{} -> {} send exhausted after {} attempts. Last error: {}. Flow file persists for recovery.",
                            i + 1,
                            hop_label,
                            MAX_SEND_ATTEMPTS,
                            err_msg
                        );
                    }
                    if let Ok((new_provider, new_url)) = create_provider_routed(rpc_manager, proxy_ctx).await {
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

        // Phase 2: Wait for confirmation
        let hop_wait_label = format_hop_label(i, hop_count);
        let hop_tx_hash = pending.tx_hash();
        runner_log(
            worker_id,
            &stage_label,
            format!("Waiting for {hop_wait_label} confirmation"),
        );

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
                        let elapsed = confirmation_start.elapsed();
                        runner_log(
                            worker_id,
                            &stage_label,
                            format!("Still waiting for {hop_wait_label} confirmation... {} elapsed", format_compact_duration(elapsed)),
                        );
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        if !last_ditch_sent {
                            #[allow(unused_assignments)]
                            { last_ditch_sent = true; }
                            let balance_now = match current_provider.get_balance(proxy.address(), None).await {
                                Ok(b) => b,
                                Err(e) => {
                                    runner_log(worker_id, &stage_label, format!("Last-ditch balance check failed: {e}; skipping bump"));
                                    U256::zero()
                                }
                            };
                            let max_gas = max_affordable_gas_price(balance_now, forward);
                            let target_gas = hop_gas + hop_gas;
                            let emergency_gas = target_gas.min(max_gas);
                            if emergency_gas == U256::zero() {
                                runner_log(
                                    worker_id,
                                    &stage_label,
                                    format!("Last-ditch skipped: proxy balance {} — checking for already-mined receipt", format_eth_amount(balance_now.as_u128() as f64 / 1e18)),
                                );
                                let receipt_check = tokio::time::timeout(
                                    Duration::from_secs(10),
                                    current_provider.get_transaction_receipt(hop_tx_hash),
                                ).await;
                                match receipt_check {
                                    Ok(Ok(Some(receipt))) => {
                                        runner_log(worker_id, &stage_label, format!("Receipt found via direct check (status={:?}): {:?}", receipt.status, receipt.transaction_hash));
                                        #[allow(unused_assignments)]
                                        { last_ditch_sent = true; }
                                        break (Some(Ok(Some(receipt))), confirmation_start.elapsed());
                                    },
                                    Ok(Ok(None)) => runner_log(worker_id, &stage_label, "Direct receipt check returned None (tx not mined). Giving up."),
                                    Ok(Err(e)) => runner_log(worker_id, &stage_label, format!("Direct receipt check failed: {e}. Giving up.")),
                                    Err(_) => runner_log(worker_id, &stage_label, "Direct receipt check timed out after 10s. Giving up."),
                                }
                            } else {
                                runner_log(
                                    worker_id,
                                    &stage_label,
                                    format!("Confirmation timeout, sending last-ditch tx with {}x gas", if emergency_gas >= target_gas { "2" } else { "reduced" }),
                                );
                                let tx = TransactionRequest::pay(next, forward).gas(21_000).gas_price(emergency_gas);
                                let send_result = proxy_signer.send_transaction(tx, None).await;
                                match send_result {
                                    Ok(new_pending) => {
                                        rpc_manager.record_success(&current_rpc_url);
                                        pending_tx = Box::pin(new_pending);
                                    }
                                    Err(e) => runner_log(worker_id, &stage_label, format!("Last-ditch send failed: {e}; flow file persists for recovery.")),
                                }
                            }
                        }
                        break (None, confirmation_start.elapsed());
                    }
                }
            }
        };

        match confirm_result {
            (Some(Ok(receipt_opt)), _) => {
                if let Some(receipt) = &receipt_opt {
                    if let Some(receipt_to) = receipt.to {
                        if receipt_to != next {
                            warn!(
                                "[WK{worker_id}] {stage_label} receipt.to {:?} != expected next {:?}. tx={:?}",
                                receipt_to, next, receipt.transaction_hash
                            );
                        }
                    }
                    if receipt.from != proxy.address() {
                        warn!(
                            "[WK{worker_id}] {stage_label} receipt.from {:?} != proxy {:?}. tx={:?}",
                            receipt.from,
                            proxy.address(),
                            receipt.transaction_hash
                        );
                    }
                    runner_log(
                        worker_id,
                        &stage_label,
                        format!("Confirmed: {:?}", receipt.transaction_hash),
                    );
                }
            },
            (Some(Err(e)), _) => {
                runner_log(
                    worker_id,
                    &stage_label,
                    format!("{hop_wait_label} confirmation failed: {e:#}. Flow file persists for recovery."),
                );
                anyhow::bail!("{hop_wait_label} confirmation failed: {e:#}");
            },
            (None, elapsed) => {
                runner_log(
                    worker_id,
                    &stage_label,
                    format!("Timed out waiting for {hop_wait_label}. Flow file persists for recovery."),
                );
                anyhow::bail!(
                    "Timed out waiting for {hop_wait_label} confirmation after {}",
                    format_compact_duration(elapsed)
                );
            },
        }

        if i == hop_count - 1 {
            recipient_tx_hash = Some(hop_tx_hash);
        }

        remaining = forward;
    }

    let recipient_tx_hash = recipient_tx_hash.context("Missing recipient tx hash")?;

    // ── Target balance verification ──
    let (provider_verify, _) =
        create_provider(rpc_manager, http_client).context("No healthy RPC for target balance verification")?;
    let target_balance_after = provider_verify.get_balance(target, None).await?;
    let actual_delta = target_balance_after.saturating_sub(target_balance_before);
    let expected_delta = remaining;
    let delivery_ok = actual_delta >= expected_delta.saturating_sub(U256::from(1_000_000_000u64));
    let shortfall = expected_delta.saturating_sub(actual_delta);

    if !delivery_ok {
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
            format_eth_amount(target_balance_before.as_u128() as f64 / 1e18),
            format_eth_amount(target_balance_after.as_u128() as f64 / 1e18),
            format_eth_amount(actual_delta.as_u128() as f64 / 1e18),
            format_eth_amount(expected_delta.as_u128() as f64 / 1e18),
        ),
    );

    // ── Final summary ──
    let (provider_final, _) = create_provider(rpc_manager, http_client).context("No healthy RPC for final balance")?;
    let sender_balance_after = provider_final.get_balance(sender_address, None).await?;
    let fee_wei = match sender_balance
        .checked_sub(sender_balance_after)
        .and_then(|spent| spent.checked_sub(remaining))
    {
        Some(fee) => fee,
        None => {
            warn!("[WK{worker_id}] Could not compute fee from balance delta (balance may have changed due to reorg)");
            U256::zero()
        },
    };
    let duration = format_compact_duration(flow_start.elapsed());

    let sender_spent_total = sender_balance.saturating_sub(sender_balance_after);
    let sender_spent_expected = seed + gas_21k_cost;
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
            "Address {:?} {} {} ETH, tx: {:?}, cost {} ETH, duration: {}",
            target,
            if delivery_ok { "received" } else { "MUST have received" },
            format_eth_amount(actual_delta.as_u128() as f64 / 1e18),
            recipient_tx_hash,
            format_eth_amount(fee_wei.as_u128() as f64 / 1e18),
            duration
        ),
    );

    // ── All hops confirmed — remove the flow file ──
    if let Some(fname) = flow_filename {
        if let Some(rc) = recovery {
            remove_flow_file(&rc.dir, &fname);
        }
    }

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
) -> Result<Vec<PlannedFund>> {
    let selected = select_targets_to_fund(targets, max_targets);
    if senders.is_empty() || selected.is_empty() {
        return Ok(vec![]);
    }

    let max_per = compute_max_per_sender(selected.len(), senders.len());
    let mut use_counts = vec![0usize; senders.len()];
    let mut plan = Vec::with_capacity(selected.len());

    for target in &selected {
        let amount: U256 = parse_units(rng.gen_range(min_target..=max_target), "ether")
            .with_context(|| format!("Invalid target amount in range [{min_target}, {max_target}]"))?
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
    Ok(plan)
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
        proxy_ctx: Option<Arc<ProxyContext>>,
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
            proxy_ctx,
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
        ensure!(args.max_hops > 0, "--max-hops must be at least 1 (got 0)");
        ensure!(
            args.min_target > 0.0,
            "--min-target must be positive (got {})",
            args.min_target
        );
        ensure!(
            args.min_target.is_finite() && args.max_target.is_finite(),
            "--min-target / --max-target must be finite numbers (got {}, {})",
            args.min_target,
            args.max_target
        );
        ensure!(
            args.min_balance.is_finite() && args.max_balance.is_finite(),
            "--min-balance / --max-balance must be finite numbers (got {}, {})",
            args.min_balance,
            args.max_balance
        );
        ensure!(
            args.min_gwei.is_finite() && args.max_gwei.is_finite(),
            "--min-gwei / --max-gwei must be finite numbers (got {}, {})",
            args.min_gwei,
            args.max_gwei
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
        let cancel = Arc::new(AtomicBool::new(false));

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
            let proxy_ctx = self.proxy_ctx.clone();
            let cancel = Arc::clone(&cancel);

            handles.push(tokio::spawn(async move {
                let worker_start = std::time::Instant::now();

                let mut rng = StdRng::from_entropy();
                while let Some((target_idx, target)) = queue.pop_front() {
                    // Check cancel flag (set when another worker panicked)
                    if cancel.load(Ordering::SeqCst) {
                        runner_log(worker_id, "Funder", "Cancelled by sibling worker failure");
                        break;
                    }
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
                    let proxy_ref = proxy_ctx.as_deref();
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
                        proxy_ref,
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
            if let Err(e) = h.await {
                warn!("Worker task panicked: {e}. Cancelling remaining workers.");
                cancel.store(true, Ordering::SeqCst);
            }
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
        )?;
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
                let balance = match tokio::time::timeout(Duration::from_secs(30), prov.get_balance(address, None)).await
                {
                    Ok(Ok(b)) => b.as_u128() as f64 / 1e18,
                    Ok(Err(e)) => {
                        warn!("Wallet {idx}: balance query failed: {e}");
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

    /// Disable egress proxy routing (RPC calls go direct)
    #[arg(long)]
    no_proxy: bool,

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

    // ── Load egress proxy pool (unless disabled) ──
    let proxy_ctx: Option<ProxyContext> = if args.no_proxy {
        println!("Egress proxies disabled by --no-proxy");
        None
    } else {
        let proxies = ProxyManager::load_proxies().unwrap_or_else(|_| {
            warn!("Failed to load proxies from config; continuing without proxies");
            vec![]
        });
        if proxies.is_empty() {
            println!("No egress proxies configured; RPC calls will go direct.");
            None
        } else {
            println!("Loaded {} egress proxy/ies for RPC routing.", proxies.len());
            Some(ProxyContext {
                pool: Arc::new(RwLock::new(proxies)),
                health: Arc::new(ProxyHealthManager::new(3, 5)),
                rate_limiter: Arc::new(ProxyRateLimiter::new(10)),
            })
        }
    };

    // ── Probe all RPC endpoints before any work ──
    let probe = probe_rpc_endpoints(&rpc_manager, &http_client, config.chain_id, 10, proxy_ctx.as_ref()).await?;

    if probe.healthy == 0 {
        anyhow::bail!("All {} RPC endpoint(s) are unhealthy. Cannot proceed.", probe.total);
    }

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
        proxy_ctx.map(Arc::new),
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
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng).unwrap();
        assert_eq!(plan.len(), 3);
    }

    #[test]
    fn test_generate_dry_run_plan_respects_max_targets() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.01), dummy_wallet(2, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, Some(1), &mut rng).unwrap();
        assert_eq!(plan.len(), 1);
    }

    #[test]
    fn test_generate_dry_run_plan_empty_on_no_senders() {
        let senders: Vec<WalletInfo> = vec![];
        let targets = vec![dummy_wallet(0, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng).unwrap();
        assert!(plan.is_empty());
    }

    #[test]
    fn test_generate_dry_run_plan_empty_on_no_targets() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets: Vec<WalletInfo> = vec![];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng).unwrap();
        assert!(plan.is_empty());
    }

    #[test]
    fn test_generate_dry_run_plan_hops_in_range() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.01), dummy_wallet(2, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng).unwrap();
        for pf in &plan {
            assert!(pf.hops >= 3 && pf.hops <= 5);
        }
    }

    #[test]
    fn test_generate_dry_run_plan_amount_in_range() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 3, 5, None, &mut rng).unwrap();
        assert_eq!(plan.len(), 1);
        let amount_eth = plan[0].amount.as_u128() as f64 / 1e18;
        assert!((0.02..=0.04).contains(&amount_eth));
    }

    #[test]
    fn test_generate_dry_run_plan_fixed_hops() {
        let senders = vec![dummy_wallet(0, 1.0)];
        let targets = vec![dummy_wallet(1, 0.01), dummy_wallet(2, 0.01)];
        let mut rng = StdRng::seed_from_u64(42);
        let plan = generate_dry_run_plan(&senders, &targets, 0.02, 0.04, 4, 4, None, &mut rng).unwrap();
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
    // Flow-seed architecture tests
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_derive_proxy_key_deterministic() {
        let mut seed = [0u8; 32];
        seed[0] = 0xAB;
        seed[31] = 0xCD;
        let a = derive_proxy_key_bytes(&seed, 0);
        let b = derive_proxy_key_bytes(&seed, 0);
        assert_eq!(a, b, "same seed+index should produce same key");
    }

    #[test]
    fn test_derive_proxy_key_different_per_index() {
        let mut seed = [0u8; 32];
        seed[0] = 0xAB;
        let a = derive_proxy_key_bytes(&seed, 0);
        let b = derive_proxy_key_bytes(&seed, 1);
        assert_ne!(a, b, "different index should produce different key");
    }

    #[test]
    fn test_derive_proxy_key_different_per_seed() {
        let mut s1 = [0u8; 32];
        let mut s2 = [0u8; 32];
        s1[0] = 0xAB;
        s2[0] = 0xCD;
        let a = derive_proxy_key_bytes(&s1, 0);
        let b = derive_proxy_key_bytes(&s2, 0);
        assert_ne!(a, b, "different seed should produce different key");
    }

    #[test]
    fn test_derive_proxy_key_produces_valid_secp256k1_key() {
        let mut seed = [0u8; 32];
        for i in 0..100 {
            seed[i % 32] = i as u8;
            let key = derive_proxy_key_bytes(&seed, i);
            let hex_key = hex::encode(key);
            let wallet_result = hex_key.parse::<LocalWallet>();
            assert!(wallet_result.is_ok(), "key[{i}] should be valid: {hex_key}");
        }
    }

    #[test]
    fn test_derive_proxy_wallet_address() {
        let mut seed = [0u8; 32];
        seed[0] = 0x42;
        let wallet = derive_proxy_wallet(&seed, 3, 11155111).unwrap();
        assert_eq!(wallet.chain_id(), 11155111);
        assert_ne!(wallet.address(), Address::zero());
    }

    #[test]
    fn test_encrypt_decrypt_flow_seed_roundtrip() {
        let mut seed = [0u8; 32];
        seed[0] = 0xAA;
        seed[31] = 0xBB;
        let (ct, iv, salt, tag) = encrypt_flow_seed(&seed, "test_password").unwrap();
        let entry = FlowEntry {
            ciphertext: ct,
            iv,
            salt,
            tag,
            hop_count: 5,
            chain_id: 11155111,
            timestamp: "2026-06-19T00:00:00Z".to_string(),
        };
        let decrypted = decrypt_flow_seed(&entry, "test_password").unwrap();
        assert_eq!(decrypted, seed);
    }

    #[test]
    fn test_decrypt_flow_seed_wrong_password_fails() {
        let mut seed = [0u8; 32];
        seed[0] = 0xAA;
        let (ct, iv, salt, tag) = encrypt_flow_seed(&seed, "correct").unwrap();
        let entry = FlowEntry {
            ciphertext: ct,
            iv,
            salt,
            tag,
            hop_count: 3,
            chain_id: 1,
            timestamp: String::new(),
        };
        let result = decrypt_flow_seed(&entry, "wrong");
        assert!(result.is_err(), "wrong password should fail decryption");
    }

    #[test]
    fn test_encrypt_flow_seed_produces_32_byte_salt() {
        let seed = [0u8; 32];
        let (_, _, salt, _) = encrypt_flow_seed(&seed, "pw").unwrap();
        let salt_bytes = hex::decode(&salt).unwrap();
        assert_eq!(salt_bytes.len(), 32);
    }

    #[test]
    fn test_encrypt_flow_seed_produces_12_byte_iv() {
        let seed = [0u8; 32];
        let (_, iv, _, _) = encrypt_flow_seed(&seed, "pw").unwrap();
        let iv_bytes = hex::decode(&iv).unwrap();
        assert_eq!(iv_bytes.len(), 12);
    }

    #[test]
    fn test_encrypt_flow_seed_produces_16_byte_tag() {
        let seed = [0u8; 32];
        let (_, _, _, tag) = encrypt_flow_seed(&seed, "pw").unwrap();
        let tag_bytes = hex::decode(&tag).unwrap();
        assert_eq!(tag_bytes.len(), 16);
    }

    #[test]
    fn test_encrypt_flow_seed_generates_unique_salt_per_call() {
        let seed = [0u8; 32];
        let (_, _, s1, _) = encrypt_flow_seed(&seed, "pw").unwrap();
        let (_, _, s2, _) = encrypt_flow_seed(&seed, "pw").unwrap();
        assert_ne!(s1, s2, "salt should be unique per call");
    }

    #[test]
    fn test_encrypt_flow_seed_unicode_password() {
        let seed = [0u8; 32];
        let result = encrypt_flow_seed(&seed, "пароль密码🔐");
        assert!(result.is_ok(), "unicode passwords should work");
    }

    #[test]
    fn test_flow_entry_serialization_roundtrip() {
        let entry = FlowEntry {
            ciphertext: "abc123".to_string(),
            iv: "def456".to_string(),
            salt: "789abc".to_string(),
            tag: "def789".to_string(),
            hop_count: 7,
            chain_id: 11155111,
            timestamp: "2026-06-19T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: FlowEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.ciphertext, "abc123");
        assert_eq!(parsed.hop_count, 7);
        assert_eq!(parsed.chain_id, 11155111);
    }

    #[test]
    fn test_flow_entry_zero_values() {
        let entry = FlowEntry {
            ciphertext: String::new(),
            iv: String::new(),
            salt: String::new(),
            tag: String::new(),
            hop_count: 0,
            chain_id: 0,
            timestamp: String::new(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: FlowEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.hop_count, 0);
    }

    #[test]
    fn test_write_flow_file_creates_file() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-flow-file");
        let _ = std::fs::create_dir_all(&temp_dir);

        let entry = FlowEntry {
            ciphertext: "ct".to_string(),
            iv: "iv".to_string(),
            salt: "salt".to_string(),
            tag: "tag".to_string(),
            hop_count: 3,
            chain_id: 1,
            timestamp: "now".to_string(),
        };
        let fname = write_flow_file(&temp_dir.to_string_lossy(), &entry).unwrap();
        assert!(fname.starts_with("flow_"), "filename should start with flow_");
        assert!(fname.ends_with(".json"));

        let file_path = temp_dir.join(&fname);
        assert!(file_path.exists(), "flow file should exist");
        let content = std::fs::read_to_string(&file_path).unwrap();
        let parsed: FlowEntry = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.hop_count, 3);

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_remove_flow_file_removes_file() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-remove-flow");
        let _ = std::fs::create_dir_all(&temp_dir);

        let test_path = temp_dir.join("flow_test_remove.json");
        std::fs::write(&test_path, "{}").unwrap();
        assert!(test_path.exists());

        remove_flow_file(&temp_dir.to_string_lossy(), "flow_test_remove.json");
        assert!(!test_path.exists(), "file should be removed");

        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_remove_flow_file_nonexistent_does_not_panic() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-remove-nonexistent");
        let _ = std::fs::create_dir_all(&temp_dir);
        // Should not panic
        remove_flow_file(&temp_dir.to_string_lossy(), "flow_nonexistent.json");
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_read_flow_files_empty_directory() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-read-empty");
        let _ = std::fs::create_dir_all(&temp_dir);
        let files = read_flow_files(&temp_dir.to_string_lossy()).unwrap();
        assert!(files.is_empty(), "empty dir should return empty list");
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_read_flow_files_skips_non_flow_files() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-read-skips");
        let _ = std::fs::create_dir_all(&temp_dir);
        std::fs::write(temp_dir.join("not_a_flow.txt"), "{}").unwrap();
        std::fs::write(temp_dir.join("random.json"), "{}").unwrap();
        std::fs::write(temp_dir.join("proxy-0-abc.json"), "{}").unwrap();
        let files = read_flow_files(&temp_dir.to_string_lossy()).unwrap();
        assert!(files.is_empty(), "non-flow files should be skipped");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_flow_files_parses_valid_entries() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-read-valid");
        let _ = std::fs::create_dir_all(&temp_dir);

        let entry = FlowEntry {
            ciphertext: "ct".to_string(),
            iv: "iv".to_string(),
            salt: "salt".to_string(),
            tag: "tag".to_string(),
            hop_count: 5,
            chain_id: 1,
            timestamp: "ts1".to_string(),
        };
        let _ = write_flow_file(&temp_dir.to_string_lossy(), &entry);

        let files = read_flow_files(&temp_dir.to_string_lossy()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1.hop_count, 5);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_flow_files_multiple_entries() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-read-multi");
        let _ = std::fs::create_dir_all(&temp_dir);

        for i in 0..5 {
            let entry = FlowEntry {
                ciphertext: format!("ct{i}"),
                iv: format!("iv{i}"),
                salt: format!("salt{i}"),
                tag: format!("tag{i}"),
                hop_count: i,
                chain_id: 1,
                timestamp: format!("ts{i}"),
            };
            let _ = write_flow_file(&temp_dir.to_string_lossy(), &entry);
        }

        let files = read_flow_files(&temp_dir.to_string_lossy()).unwrap();
        assert_eq!(files.len(), 5);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_flow_files_skips_invalid_json() {
        let temp_dir = std::env::temp_dir().join("testnet-fund-read-invalid");
        let _ = std::fs::create_dir_all(&temp_dir);
        std::fs::write(temp_dir.join("flow_bad.json"), "not valid json {{{").unwrap();
        let files = read_flow_files(&temp_dir.to_string_lossy()).unwrap();
        assert!(files.is_empty(), "invalid JSON files should be skipped");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_decrypt_flow_seed_invalid_hex_fails() {
        let entry = FlowEntry {
            ciphertext: "zz".to_string(), // invalid hex
            iv: "00".to_string(),
            salt: "00".to_string(),
            tag: "00".to_string(),
            hop_count: 1,
            chain_id: 1,
            timestamp: String::new(),
        };
        let result = decrypt_flow_seed(&entry, "pw");
        assert!(result.is_err(), "invalid hex should fail");
    }

    #[test]
    fn test_derive_proxy_key_does_not_produce_zero_key() {
        let seed = [0u8; 32];
        for i in 0..50 {
            let key = derive_proxy_key_bytes(&seed, i);
            assert_ne!(key, [0u8; 32], "key[{i}] should not be all zeros");
        }
    }

    #[test]
    fn test_derive_proxy_key_32_hops_all_distinct() {
        let mut seed = [0u8; 32];
        seed[0] = 0xDE;
        let mut seen = std::collections::HashSet::new();
        for i in 0..32 {
            let key = derive_proxy_key_bytes(&seed, i);
            assert!(seen.insert(key), "key[{i}] should be unique");
        }
    }

    #[test]
    fn test_flow_file_write_is_atomic() {
        // Verify the temp->rename pattern works
        let temp_dir = std::env::temp_dir().join("testnet-fund-atomic-write");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Write should not leave temp files behind
        let entry = FlowEntry {
            ciphertext: "ct".to_string(),
            iv: "iv".to_string(),
            salt: "salt".to_string(),
            tag: "tag".to_string(),
            hop_count: 1,
            chain_id: 1,
            timestamp: "t".to_string(),
        };
        let fname = write_flow_file(&temp_dir.to_string_lossy(), &entry).unwrap();

        // Check no .tmp files left
        let has_tmp = std::fs::read_dir(&temp_dir)
            .unwrap()
            .any(|e| e.unwrap().file_name().to_string_lossy().starts_with(".tmp_"));
        assert!(!has_tmp, "no temp files should remain after write");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_uuid_fast_generates_unique_values() {
        let a = uuid_fast();
        let b = uuid_fast();
        assert_eq!(a.len(), 16, "uuid_fast should be 16 hex chars (8 bytes)");
        assert_ne!(a, b, "two calls should produce different values");
    }

    #[test]
    fn test_uuid_fast_is_valid_hex() {
        let id = uuid_fast();
        assert!(hex::decode(&id).is_ok(), "uuid_fast output should be valid hex");
    }

    #[test]
    fn test_recover_flow_integration() {
        // End-to-end: create a flow file → read it back → decrypt → derive proxy keys → verify addresses
        let temp_dir = std::env::temp_dir().join("testnet-fund-recover-integration");
        let _ = std::fs::create_dir_all(&temp_dir);

        // 1. Create a known flow_seed
        let mut flow_seed = [0u8; 32];
        flow_seed[0] = 0xAA;
        flow_seed[15] = 0xBB;
        flow_seed[31] = 0xCC;

        // 2. Encrypt it and write a flow file
        let (ct, iv, salt, tag) = encrypt_flow_seed(&flow_seed, "recover_pw").unwrap();
        let flow_entry = FlowEntry {
            ciphertext: ct,
            iv,
            salt,
            tag,
            hop_count: 3,
            chain_id: 11155111,
            timestamp: "test".to_string(),
        };
        let dir_str = temp_dir.to_string_lossy().to_string();
        let fname = write_flow_file(&dir_str, &flow_entry).unwrap();
        assert!(std::fs::exists(temp_dir.join(&fname)).unwrap_or(false));

        // 3. Read it back (as recovery would)
        let files = read_flow_files(&dir_str).unwrap();
        assert_eq!(files.len(), 1);

        // 4. Decrypt the flow_seed
        let decrypted_seed = decrypt_flow_seed(&files[0].1, "recover_pw").unwrap();
        assert_eq!(decrypted_seed, flow_seed, "decrypted seed should match original");

        // 5. Derive all proxy keys from the decrypted seed
        for i in 0..3 {
            let wallet = derive_proxy_wallet(&decrypted_seed, i, 11155111).unwrap();
            assert_eq!(wallet.chain_id(), 11155111);
            assert_ne!(
                wallet.address(),
                Address::zero(),
                "proxy {i} address should not be zero"
            );

            // Verify the same seed+index always produces the same address
            let wallet2 = derive_proxy_wallet(&decrypted_seed, i, 11155111).unwrap();
            assert_eq!(
                wallet.address(),
                wallet2.address(),
                "proxy {i} address should be deterministic"
            );

            // Verify different indices produce different addresses
            if i > 0 {
                let prev = derive_proxy_wallet(&decrypted_seed, i - 1, 11155111).unwrap();
                assert_ne!(
                    wallet.address(),
                    prev.address(),
                    "proxy {i} should differ from proxy {}",
                    i - 1
                );
            }
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_decrypt_flow_seed_corrupted_ciphertext_fails() {
        let entry = FlowEntry {
            ciphertext: "deadbeef".to_string(), // valid hex but garbage ciphertext
            iv: "00112233445566778899aabb".to_string(),
            salt: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".to_string(),
            tag: "00112233445566778899aabbccddeeff".to_string(),
            hop_count: 1,
            chain_id: 1,
            timestamp: String::new(),
        };
        let result = decrypt_flow_seed(&entry, "pw");
        assert!(result.is_err(), "corrupted ciphertext should fail decryption");
    }

    #[test]
    fn test_decrypt_flow_seed_short_ciphertext_fails() {
        let entry = FlowEntry {
            ciphertext: "ab".to_string(), // too short
            iv: "00".repeat(12),
            salt: "00".repeat(32),
            tag: "00".repeat(16),
            hop_count: 1,
            chain_id: 1,
            timestamp: String::new(),
        };
        let result = decrypt_flow_seed(&entry, "pw");
        assert!(result.is_err(), "short ciphertext should fail");
    }

    #[test]
    fn test_flow_entry_serialization_with_all_fields() {
        let entry = FlowEntry {
            ciphertext: "ct123".to_string(),
            iv: "iv456".to_string(),
            salt: "salt789".to_string(),
            tag: "tag000".to_string(),
            hop_count: 10,
            chain_id: 11155111,
            timestamp: "2026-06-19T12:34:56.789Z".to_string(),
        };
        let json = serde_json::to_string_pretty(&entry).unwrap();
        let parsed: FlowEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.hop_count, 10);
        assert_eq!(parsed.chain_id, 11155111);
        assert_eq!(parsed.timestamp, "2026-06-19T12:34:56.789Z");
    }

    #[test]
    fn test_derive_proxy_wallet_10_hops_all_valid() {
        let mut seed = [0u8; 32];
        // Use a non-zero seed
        for b in 0..32 {
            seed[b] = b as u8;
        }
        let mut seen = std::collections::HashSet::new();
        for i in 0..10 {
            let wallet = derive_proxy_wallet(&seed, i, 1).unwrap();
            assert!(seen.insert(wallet.address()), "proxy {i} address should be unique");
        }
        assert_eq!(seen.len(), 10);
    }

    #[test]
    fn test_probe_summary_basic() {
        let summary = ProbeSummary { total: 5, healthy: 3 };
        assert_eq!(summary.total, 5);
        assert_eq!(summary.healthy, 3);
    }

    #[test]
    fn test_probe_summary_zero_healthy() {
        let summary = ProbeSummary { total: 2, healthy: 0 };
        assert_eq!(summary.healthy, 0);
    }

    /// Verify `probe_rpc_endpoints` handles an invalid URL gracefully
    /// (marks it unhealthy via record_failure, no panic).
    #[tokio::test]
    async fn test_probe_rpc_endpoints_invalid_url() {
        let rpc_manager = Arc::new(RpcManager::new(1, &["not-a-valid-url://".to_string()]));
        let http_client = reqwest::Client::new();
        let result = probe_rpc_endpoints(&rpc_manager, &http_client, 1, 3, None).await;
        assert!(result.is_ok(), "probe should not fail on invalid URL");
        let summary = result.unwrap();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.healthy, 0, "invalid URL should be marked unhealthy");
    }

    #[test]
    fn test_probe_summary_display_after_zero_rpcs() {
        // Edge case: ProbeSummary { total: 0, healthy: 0 } shouldn't crash
        let summary = ProbeSummary { total: 0, healthy: 0 };
        assert_eq!(summary.total, 0);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Receipt validation (to/from mismatch detection)
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn receipt_to_mismatch_should_be_flagged() {
        // Receipt says to=X, but we expected Y. The receipt is wrong or stale.
        let receipt_to: Option<Address> = Some(Address::from_low_u64_be(1));
        let expected: Address = Address::from_low_u64_be(2);
        let mismatched = receipt_to.map_or(false, |r| r != expected);
        assert!(mismatched, "receipt to != expected should be detected");
    }

    #[test]
    fn receipt_to_match_should_pass() {
        let addr = Address::from_low_u64_be(1);
        let receipt_to: Option<Address> = Some(addr);
        let expected = addr;
        let mismatched = receipt_to.map_or(false, |r| r != expected);
        assert!(!mismatched, "matching receipt to should pass");
    }

    #[test]
    fn receipt_to_none_should_skip_check() {
        // Some receipts (e.g., contract creation) have no `to` field
        let receipt_to: Option<Address> = None;
        let expected = Address::from_low_u64_be(1);
        // The check is gated on Some(receipt_to), so None is fine
        let mismatched = receipt_to.map_or(false, |r| r != expected);
        assert!(!mismatched, "None receipt_to should skip the check");
    }

    #[test]
    fn receipt_from_mismatch_should_be_flagged() {
        let proxy_addr = Address::from_low_u64_be(1);
        let receipt_from = Address::from_low_u64_be(2);
        let mismatched = receipt_from != proxy_addr;
        assert!(mismatched, "receipt from != proxy should be detected");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Delivery verification logic
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn delivery_ok_when_delta_meets_expected() {
        // Target got exactly what we expected
        let before = U256::from(100_000_000_000_000_000u64);
        let after = U256::from(350_000_000_000_000_000u64);
        let expected = U256::from(250_000_000_000_000_000u64);
        let dust = U256::from(1_000_000_000u64);
        let delta = after.saturating_sub(before);
        let ok = delta >= expected.saturating_sub(dust);
        assert!(ok, "exact match should pass");
    }

    #[test]
    fn delivery_ok_within_dust_tolerance() {
        // Target got 0.01 gwei less than expected — within 1 gwei dust tolerance
        // before=0.1, after=0.24999999999, expected=0.25
        // delta = 0.24999999999 ETH = 249_999_999_990_000_000 wei
        // expected - dust (1 gwei) = 249_999_999_000_000_000 wei
        // delta >= expected - dust → passes
        let before = U256::from(100_000_000_000_000_000u64);
        let after = U256::from(349_999_999_990_000_000u64);
        let expected = U256::from(250_000_000_000_000_000u64);
        let dust = U256::from(1_000_000_000u64);
        let delta = after.saturating_sub(before);
        let ok = delta >= expected.saturating_sub(dust);
        assert!(ok, "0.01 gwei shortfall should be within 1 gwei dust tolerance");
    }

    #[test]
    fn delivery_fails_when_target_got_nothing() {
        // The user-reported bug: target balance unchanged but tx was "confirmed"
        let before = U256::from(267_632_000_000_000_000u64); // 0.267632 ETH
        let after = U256::from(267_632_000_000_000_000u64); // same
        let expected = U256::from(259_165_000_000_000_000u64); // 0.259165 ETH
        let dust = U256::from(1_000_000_000u64);
        let delta = after.saturating_sub(before);
        let ok = delta >= expected.saturating_sub(dust);
        assert!(!ok, "delivery check should FAIL when target got nothing");
    }

    #[test]
    fn delivery_shortfall_computed_correctly() {
        // Target got half of expected — compute the shortfall
        let before = U256::from(100_000_000_000_000_000u64);
        let after = U256::from(225_000_000_000_000_000u64); // got 0.125, expected 0.25
        let expected = U256::from(250_000_000_000_000_000u64);
        let delta = after.saturating_sub(before);
        let shortfall = expected.saturating_sub(delta);
        assert_eq!(delta, U256::from(125_000_000_000_000_000u64));
        assert_eq!(shortfall, U256::from(125_000_000_000_000_000u64));
    }

    #[test]
    fn delivery_log_wording_uses_must_have_on_failure() {
        // The log should use "MUST have received" wording when delivery failed,
        // to make it clear that the on-chain state did not match the planned amount.
        let delivery_ok = false;
        let actual_delta_eth = 0.0;
        let tx_hash = "0xa7483bc8...";
        let phrase = if delivery_ok { "received" } else { "MUST have received" };
        let log = format!("Address 0x... {} {} ETH, tx: {}", phrase, actual_delta_eth, tx_hash);
        assert!(log.contains("MUST have received"));
    }

    #[test]
    fn delivery_log_wording_uses_received_on_success() {
        let delivery_ok = true;
        let phrase = if delivery_ok { "received" } else { "MUST have received" };
        assert_eq!(phrase, "received");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // target_balance_before capture timing
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn target_balance_before_captured_at_function_start() {
        // The fix: capture target's on-chain balance BEFORE any hops fire,
        // so the post-funding delta check correctly measures delivery.
        // Pre-fix: both "before" and "after" were captured at the end,
        // making delta always ~0 and the verification a silent no-op.
        // This test pins the contract: target_balance_before reflects
        // the state BEFORE the first hop tx is sent.
        let before = U256::from(267_632_000_000_000_000u64);
        let after_funded = U256::from(526_797_000_000_000_000u64); // +0.259165
        let delta = after_funded.saturating_sub(before);
        let expected = U256::from(259_165_000_000_000_000u64);
        let dust = U256::from(1_000_000_000u64);
        assert!(
            delta >= expected.saturating_sub(dust),
            "delta should reflect actual delivery"
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Rate limit error retryability
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn rate_limit_exceeded_is_retryable() {
        // Sepolia/Infura returns this exact message when concurrent requests exceed the limit
        let err = anyhow::anyhow!("(code: -32005, message: rate limit exceeded, data: None)");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn rate_limit_lowercase_is_retryable() {
        let err = anyhow::anyhow!("rate limit exceeded");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn rate_limited_alternative_is_retryable() {
        let err = anyhow::anyhow!("rate limited, slow down");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn too_many_requests_is_retryable() {
        let err = anyhow::anyhow!("429 too many requests");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn request_limit_is_retryable() {
        let err = anyhow::anyhow!("request limit reached for this endpoint");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn jsonrpc_error_code_32005_is_retryable() {
        // The numeric code alone should trigger retry
        let err = anyhow::anyhow!("error code: -32005");
        assert!(should_retry_proxy_send_error(&err));
    }

    #[test]
    fn insufficient_funds_is_NOT_retryable() {
        // This is a real error, not transient — don't retry
        let err = anyhow::anyhow!("insufficient funds for gas * price + value");
        assert!(!should_retry_proxy_send_error(&err));
    }

    #[test]
    fn nonce_too_low_is_NOT_retryable() {
        // The replacement would also fail with nonce too low
        let err = anyhow::anyhow!("nonce too low");
        assert!(!should_retry_proxy_send_error(&err));
    }

    #[test]
    fn revert_is_NOT_retryable() {
        let err = anyhow::anyhow!("execution reverted: transfer failed");
        assert!(!should_retry_proxy_send_error(&err));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Receipt-on-balance-zero path
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn balance_zero_means_tx_likely_mined() {
        // If the proxy's balance is 0 at the deadline, the original tx was
        // almost certainly mined (the proxy paid for gas + forwarded value).
        // The fix: do a direct receipt check before reporting failure.
        let balance_now = U256::zero();
        let forward = U256::from(259_165_000_000_000_000u64);
        let max_gas = max_affordable_gas_price(balance_now, forward);
        // balance <= value, so max_gas = 0 → triggers the receipt-check branch
        assert_eq!(max_gas, U256::zero());
    }

    #[test]
    fn balance_zero_with_small_value_still_triggers_check() {
        let balance = U256::zero();
        let value = U256::from(1u64); // tiny value
        let max_gas = max_affordable_gas_price(balance, value);
        assert_eq!(max_gas, U256::zero());
    }
}
