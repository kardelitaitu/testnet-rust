//! Proxy Health - Health checking and banlist management for proxies
//!
//! This module provides health monitoring for HTTP proxies used in transaction
//! spamming. It implements a banlist pattern where unhealthy proxies are temporarily
//! banned and automatically rechecked after a cooldown period.
//!
//! # Architecture
//!
//! The health system consists of two main components:
//!
//! 1. **ProxyBanlist**: Tracks banned proxies with automatic expiration
//! 2. **Health Checker**: Tests proxies by making HTTP requests to the RPC endpoint
//!
//! # Health Check Flow
//!
//! 1. **Startup Scan**: All proxies are checked concurrently on startup
//! 2. **Banning**: Unhealthy proxies are banned for a configurable duration (default 10 min)
//! 3. **Client Filtering**: ClientPool automatically excludes banned proxies
//! 4. **Background Recheck**: Banned proxies are periodically retested
//! 5. **Auto-Unban**: Healthy proxies are automatically unbanned
//!
//! # Integration with ClientPool
//!
//! The [`ClientPool`] integrates with ProxyBanlist:
//!
//! ```rust,no_run
//! use tempo_spammer::{ClientPool, proxy_health::ProxyBanlist};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let banlist = ProxyBanlist::new(10); // 10 minute ban duration
//!
//! let pool = ClientPool::new(
//!     "config/config.toml",
//!     Some("password".to_string()),
//!     None,
//! ).await?;
//!
//! // Pool will now check banlist before assigning proxies
//! # Ok(())
//! # }
//! ```
//!
//! # Concurrent Health Checks
//!
//! The `scan_proxies` function checks all proxies concurrently with a configurable
//! limit to avoid overwhelming the network:
//!
//! ```rust,no_run
//! use tempo_spammer::proxy_health::{scan_proxies, ProxyBanlist};
//! use tempo_spammer::tasks::load_proxies;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let proxies = load_proxies("config/proxies.txt")?;
//! let banlist = ProxyBanlist::new(30);
//!
//! // Scan all proxies, 50 at a time
//! let (healthy, banned) = scan_proxies(
//!     &proxies,
//!     "https://rpc.moderato.tempo.xyz",
//!     &banlist,
//!     50,
//! ).await;
//!
//! println!("Healthy: {}, Banned: {}", healthy, banned);
//! # Ok(())
//! # }
//! ```

use crate::tasks::ProxyConfig;
use anyhow::Result;
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Tracks banned proxies with automatic expiration
///
/// Implements a temporary banlist for unhealthy proxies. Banned proxies are
/// automatically excluded from rotation until the ban duration expires or
/// they are manually unbanned after passing a health check.
///
/// # Thread Safety
///
/// This struct is thread-safe and can be shared across multiple async tasks.
/// All operations use appropriate locking for concurrent access.
///
/// # Clone Behavior
///
/// Cloning creates a new reference to the same underlying banlist. All clones
/// share the same banned proxy state.
#[derive(Clone)]
pub struct ProxyBanlist {
    /// Map of banned proxy indices to ban start time
    banned: Arc<RwLock<HashMap<usize, Instant>>>,
    /// Duration for which proxies remain banned
    ban_duration: Duration,
}

impl ProxyBanlist {
    /// Creates a new banlist with specified ban duration
    ///
    /// # Arguments
    ///
    /// * `ban_duration_minutes` - Duration in minutes for which proxies remain banned
    ///
    /// # Example
    ///
    /// ```rust
    /// use tempo_spammer::proxy_health::ProxyBanlist;
    ///
    /// // Ban proxies for 10 minutes
    /// let banlist = ProxyBanlist::new(10);
    /// ```
    pub fn new(ban_duration_minutes: u64) -> Self {
        Self {
            banned: Arc::new(RwLock::new(HashMap::new())),
            ban_duration: Duration::from_secs(ban_duration_minutes * 60),
        }
    }

    /// Checks if a proxy is currently banned
    ///
    /// Returns true if the proxy index is in the banlist and the ban duration
    /// has not yet expired.
    ///
    /// # Arguments
    ///
    /// * `proxy_index` - The index of the proxy to check
    ///
    /// # Returns
    ///
    /// `true` if the proxy is banned, `false` otherwise
    pub async fn is_banned(&self, proxy_index: usize) -> bool {
        let banned = self.banned.read().await;
        if let Some(&ban_time) = banned.get(&proxy_index) {
            ban_time.elapsed() < self.ban_duration
        } else {
            false
        }
    }

    /// Bans a proxy temporarily
    ///
    /// Adds the proxy to the banlist with the current timestamp. The proxy will
    /// remain banned until the ban duration expires or it is manually unbanned.
    ///
    /// # Arguments
    ///
    /// * `proxy_index` - The index of the proxy to ban
    pub async fn ban(&self, proxy_index: usize) {
        let mut banned = self.banned.write().await;
        banned.insert(proxy_index, Instant::now());
    }

    /// Unbans a proxy manually
    ///
    /// Removes the proxy from the banlist immediately, making it available
    /// for use again. Typically called when a health check passes.
    ///
    /// # Arguments
    ///
    /// * `proxy_index` - The index of the proxy to unban
    pub async fn unban(&self, proxy_index: usize) {
        let mut banned = self.banned.write().await;
        banned.remove(&proxy_index);
    }

    /// Gets list of currently banned proxy indices
    ///
    /// Returns all proxy indices that are currently banned and whose ban
    /// duration has not yet expired.
    ///
    /// # Returns
    ///
    /// Vector of banned proxy indices
    pub async fn get_banned_indices(&self) -> Vec<usize> {
        let banned = self.banned.read().await;
        let now = Instant::now();
        banned
            .iter()
            .filter(|(_, ban_time)| (now - **ban_time) < self.ban_duration)
            .map(|(&idx, _)| idx)
            .collect()
    }

    /// Removes expired bans from the list
    ///
    /// Cleans up the banlist by removing entries where the ban duration has
    /// expired. This is called automatically by the background recheck task.
    pub async fn cleanup_expired(&self) {
        let mut banned = self.banned.write().await;
        let now = Instant::now();
        banned.retain(|_, ban_time| (now - *ban_time) < self.ban_duration);
    }
}
use std::sync::OnceLock;

static CLIENT_CACHE: OnceLock<tokio::sync::RwLock<HashMap<String, reqwest::Client>>> =
    OnceLock::new();

/// Test if a proxy is healthy using cached clients
async fn check_proxy_health(proxy: &ProxyConfig, rpc_url: &str) -> bool {
    let proxy_url_full = if let (Some(user), Some(pass)) = (&proxy.username, &proxy.password) {
        // Formatted for reqwest::Proxy
        let host_port = proxy
            .url
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        format!("http://{}:{}@{}", user, pass, host_port)
    } else {
        proxy.url.clone()
    };

    // Get cache
    let cache = CLIENT_CACHE.get_or_init(|| tokio::sync::RwLock::new(HashMap::new()));

    // Try to get existing client
    let client = {
        let read = cache.read().await;
        read.get(&proxy_url_full).cloned()
    };

    let client = if let Some(c) = client {
        c
    } else {
        // Build new client if missing
        let proxy_config = match reqwest::Proxy::all(&proxy_url_full) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("Bad proxy config for {}: {}", proxy_url_full, e);
                return false;
            }
        };

        let new_client = match reqwest::Client::builder()
            .proxy(proxy_config)
            .timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(2)
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to build client for {}: {}", proxy_url_full, e);
                return false;
            }
        };

        // Insert into cache
        let mut write = cache.write().await;
        write.insert(proxy_url_full.clone(), new_client.clone());
        new_client
    };

    // Try a simple HEAD request to RPC endpoint
    match client.head(rpc_url).send().await {
        Ok(_) => true, // Any response = proxy works
        Err(_) => false,
    }
}

/// Scan all proxies in parallel batches and ban unhealthy ones
pub async fn scan_proxies(
    proxies: &[ProxyConfig],
    rpc_url: &str,
    banlist: &ProxyBanlist,
    concurrent_limit: usize,
) -> (usize, usize) {
    tracing::info!(
        "🔍 Scanning {} proxies ({} concurrent)...",
        proxies.len(),
        concurrent_limit
    );

    let results: Vec<(usize, bool)> = stream::iter(proxies.iter().enumerate())
        .map(|(idx, proxy)| async move {
            let is_healthy = check_proxy_health(proxy, rpc_url).await;
            (idx, is_healthy)
        })
        .buffer_unordered(concurrent_limit)
        .collect()
        .await;

    let mut healthy_count = 0;
    let mut banned_count = 0;

    for (idx, is_healthy) in results {
        if is_healthy {
            healthy_count += 1;
            banlist.unban(idx).await; // Unban if was previously banned
        } else {
            banned_count += 1;
            banlist.ban(idx).await;
        }
    }

    (healthy_count, banned_count)
}

/// Scan proxies with early return - starts workers when minimum healthy proxies reached
/// Continues checking remaining proxies in background
///
/// # Arguments
/// * `proxies` - List of proxy configurations
/// * `rpc_url` - RPC endpoint URL for health checks
/// * `banlist` - Proxy banlist to update
/// * `concurrent_limit` - Max concurrent health checks
/// * `min_healthy` - Minimum healthy proxies needed before returning (e.g., 50)
///
/// # Returns
/// (healthy_count, banned_count, background_handle) - Handle to continue checking in background
pub async fn scan_proxies_partial(
    proxies: Arc<Vec<ProxyConfig>>,
    rpc_url: String,
    banlist: ProxyBanlist,
    concurrent_limit: usize,
    min_healthy: usize,
) -> (usize, usize, tokio::task::JoinHandle<(usize, usize)>) {
    tracing::info!(
        "🔍 Fast-start: Checking {} proxies, will start when {} are healthy...",
        proxies.len(),
        min_healthy
    );

    // Clone for foreground checks (we need to use these after spawning background task)
    let banlist_fg = banlist.clone();
    let proxies_fg = proxies.clone();
    let rpc_url_fg = rpc_url.clone();

    // Spawn background task to check ALL proxies
    let background_handle = tokio::spawn(async move {
        scan_proxies_with_progress(proxies, rpc_url, banlist, concurrent_limit).await
    });

    // Wait for minimum healthy proxies
    let mut healthy_count = 0;
    let mut banned_count = 0;
    let mut checked_count = 0;

    // Check proxies one by one until we have enough
    for (idx, proxy) in proxies_fg.iter().enumerate() {
        if healthy_count >= min_healthy {
            tracing::info!(
                "✅ Fast-start: {} healthy proxies reached! Starting workers while checking remaining {}...",
                healthy_count,
                proxies_fg.len() - checked_count
            );
            break;
        }

        let is_healthy = check_proxy_health(proxy, &rpc_url_fg).await;
        checked_count += 1;

        if is_healthy {
            healthy_count += 1;
            banlist_fg.unban(idx).await;
        } else {
            banned_count += 1;
            banlist_fg.ban(idx).await;
        }
    }

    (healthy_count, banned_count, background_handle)
}

/// Internal function to scan all proxies and update banlist
async fn scan_proxies_with_progress(
    proxies: Arc<Vec<ProxyConfig>>,
    rpc_url: String,
    banlist: ProxyBanlist,
    concurrent_limit: usize,
) -> (usize, usize) {
    // Convert to owned vector to avoid lifetime issues
    let proxy_vec: Vec<(usize, ProxyConfig)> = proxies
        .iter()
        .enumerate()
        .map(|(idx, proxy)| (idx, proxy.clone()))
        .collect();

    let results: Vec<(usize, bool)> = stream::iter(proxy_vec)
        .map(|(idx, proxy)| {
            let rpc_url = rpc_url.clone();
            async move {
                let is_healthy = check_proxy_health(&proxy, &rpc_url).await;
                (idx, is_healthy)
            }
        })
        .buffer_unordered(concurrent_limit)
        .collect()
        .await;

    let mut healthy_count = 0;
    let mut banned_count = 0;

    for (idx, is_healthy) in results {
        if is_healthy {
            healthy_count += 1;
            banlist.unban(idx).await;
        } else {
            banned_count += 1;
            banlist.ban(idx).await;
        }
    }

    tracing::info!(
        "✅ Background proxy check complete: {}/{} healthy, {} banned",
        healthy_count,
        proxies.len(),
        banned_count
    );

    (healthy_count, banned_count)
}

/// Background task to re-check banned proxies every 10 minutes
pub async fn start_recheck_task(
    proxies: Arc<Vec<ProxyConfig>>,
    rpc_url: String,
    banlist: ProxyBanlist,
    check_interval_minutes: u64,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(check_interval_minutes * 60));
    interval.tick().await; // Skip first immediate tick

    loop {
        interval.tick().await;

        let banned_indices = banlist.get_banned_indices().await;
        if banned_indices.is_empty() {
            tracing::debug!("⏱️ No banned proxies to re-check");
            continue;
        }

        tracing::info!("⏱️ Re-checking {} banned proxies...", banned_indices.len());

        let mut unbanned_count = 0;
        for idx in banned_indices {
            if let Some(proxy) = proxies.get(idx) {
                if check_proxy_health(proxy, &rpc_url).await {
                    banlist.unban(idx).await;
                    unbanned_count += 1;
                    tracing::debug!("✅ Proxy {} recovered and unbanned", idx);
                }
            }
        }

        if unbanned_count > 0 {
            tracing::info!("✅ Unbanned {} recovered proxies", unbanned_count);
        }

        // Cleanup expired bans
        banlist.cleanup_expired().await;
    }
}
