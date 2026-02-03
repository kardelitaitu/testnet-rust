//! Client Pool - Wallet leasing and proxy rotation manager
//!
//! This module provides a sophisticated client pool for managing multiple wallets
//! and proxies with automatic rotation, health checking, and concurrency control.
//!
//! # Architecture
//!
//! The pool implements an RAII (Resource Acquisition Is Initialization) pattern
//! for wallet leasing:
//!
//! 1. **Acquisition**: Workers request a client via [`ClientPool::try_acquire_client`]
//! 2. **Selection**: Pool selects random available wallet with healthy proxy
//! 3. **Locking**: Wallet is marked as "in use" to prevent double-spending
//! 4. **Lease**: Returns a [`ClientLease`] that auto-releases on drop
//! 5. **Cooldown**: 4-second delay before wallet is available again (prevents nonce races)
//! 6. **Release**: Wallet returns to available pool
//!
//! # Concurrency Model
//!
//! - **RwLock** for client cache (many readers, few writers)
//! - **Mutex** for locked wallet set (exclusive access)
//! - **Async-aware** - all operations are non-blocking
//!
//! # Example
//!
//! ```rust,no_run
//! use tempo_spammer::ClientPool;
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create pool with 10 wallets
//! let pool = Arc::new(
//!     ClientPool::new(
//!         "config/config.toml",
//!         Some("wallet_password".to_string()),
//!         None, // use all available proxies
//!     ).await?
//! );
//!
//! // Worker acquires a client
//! if let Some(lease) = pool.try_acquire_client().await {
//!     // Use the client
//!     let address = lease.client.address();
//!     println!("Using wallet: {:?}", address);
//!     
//!     // Client is automatically released when lease drops
//! } else {
//!     println!("No available clients - all wallets in use");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Proxy Rotation
//!
//! The pool supports multiple proxies with automatic rotation:
//! - Random selection from healthy proxies
//! - Integration with [`ProxyBanlist`] for health tracking
//! - Automatic fallback to direct connection if all proxies banned
//! - Per-proxy HTTP client caching for connection reuse
//!
//! # Nonce Management
//!
//! Optional integration with [`NonceManager`] for high-throughput scenarios:
//! - Caches nonces locally to reduce RPC calls
//! - Thread-safe increment operations
//! - Automatic reset on "nonce too low" errors
//!
//! # Performance Considerations
//!
//! - HTTP clients are cached per proxy (connection reuse)
//! - Wallet clients are cached after first creation
//! - 4-second cooldown prevents nonce synchronization issues
//! - Random selection distributes load evenly

use crate::TempoClient;
use crate::config::TempoSpammerConfig as Config;
use crate::tasks::load_proxies;
use anyhow::{Context, Result};
use core_logic::WalletManager;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::RwLock;

/// Pool of clients for multi-wallet transaction spamming
///
/// Manages a collection of [`TempoClient`] instances with automatic rotation,
/// proxy health checking, and concurrency control via wallet leasing.
///
/// # Thread Safety
///
/// This struct is thread-safe and designed to be shared across multiple async
/// tasks via `Arc<ClientPool>`. All internal state is protected by appropriate
/// synchronization primitives.
///
/// # Fields
///
/// - `wallet_manager`: Source of wallet keys
/// - `clients`: Cache of created clients by wallet index
/// - `http_clients`: Cache of HTTP clients per proxy (for connection reuse)
/// - `proxies`: List of available proxy configurations
/// - `config`: Spammer configuration
/// - `locked_wallets`: Set of currently leased wallet indices
/// - `nonce_manager`: Optional nonce caching
/// - `proxy_banlist`: Optional proxy health tracking
pub struct ClientPool {
    /// Wallet manager for accessing encrypted keys
    wallet_manager: Arc<WalletManager>,
    /// Password for wallet decryption
    wallet_password: Option<String>,
    /// Cache of created clients by wallet index
    clients: RwLock<HashMap<usize, TempoClient>>,
    /// Cache of HTTP clients per proxy (None = direct, Some(url) = proxy)
    /// This enables connection reuse for better performance
    http_clients: RwLock<HashMap<Option<String>, reqwest::Client>>,
    /// Available proxy configurations
    proxies: Vec<crate::tasks::ProxyConfig>,
    /// Spammer configuration
    pub config: Config,
    /// Set of wallet indices currently in use (leased)
    pub locked_wallets: tokio::sync::Mutex<std::collections::HashSet<usize>>,
    /// Optional nonce manager for caching (legacy) - shared across all wallets
    pub nonce_manager: Option<Arc<crate::NonceManager>>,
    /// Optional robust nonce manager with per-request tracking (recommended) - shared across all wallets
    pub robust_nonce_manager: Option<Arc<crate::RobustNonceManager>>,
    /// Sharded nonce managers for per-wallet isolation (when config.nonce.per_wallet is true)
    pub sharded_nonce_managers: Vec<Arc<crate::NonceManager>>,
    /// Sharded robust nonce managers for per-wallet isolation (when config.nonce.per_wallet is true)
    pub sharded_robust_nonce_managers: Vec<Arc<crate::RobustNonceManager>>,
    /// Optional proxy banlist for health tracking
    pub proxy_banlist: Option<crate::proxy_health::ProxyBanlist>,
    /// Database manager for logging
    pub db: Option<Arc<core_logic::database::DatabaseManager>>,

    // === O(1) Wallet Selection Optimization ===
    /// Set of currently available (unlocked) wallet indices
    /// Maintained incrementally for O(1) acquisition using swap-remove
    available_wallets: RwLock<Vec<usize>>,

    /// Maps wallet index to its position in available_wallets vec
    /// Enables O(1) removal when wallet is locked
    available_positions: RwLock<HashMap<usize, usize>>,

    /// Cache for proxy banned status to avoid repeated checks
    /// Maps proxy_index -> (is_banned, timestamp)
    proxy_cache: RwLock<HashMap<usize, (bool, std::time::Instant)>>,

    // === Proxy Rotation for Even Distribution ===
    /// Atomic counter for round-robin proxy rotation across all wallets
    /// Ensures all 390+ proxies are utilized evenly
    proxy_rotation_counter: AtomicUsize,

    /// Semaphore to limit total concurrent connections across all workers
    pub connection_semaphore: Arc<tokio::sync::Semaphore>,
}

/// RAII guard for a leased client
///
/// When dropped, automatically releases the wallet back to the pool after
/// a cooldown period. Implements [`Deref`] to allow transparent access to
/// the underlying [`TempoClient`].
///
/// # Usage
///
/// ```rust,no_run
/// use tempo_spammer::ClientPool;
/// use std::sync::Arc;
///
/// # async fn example(pool: Arc<ClientPool>) -> anyhow::Result<()> {
/// if let Some(lease) = pool.try_acquire_client().await {
///     // Access client through deref
///     let address = lease.address();
///     
///     // Or access explicitly
///     let client = &lease.client;
/// } // Released automatically here
/// # Ok(())
/// # }
/// ```
pub struct ClientLease {
    /// The leased client instance
    pub client: TempoClient,
    /// Index of the wallet in the pool
    pub index: usize,
    /// Reference to the pool for release on drop
    pool: Arc<ClientPool>,
    /// Connection permit that is released when lease is dropped
    pub permit: Option<tokio::sync::OwnedSemaphorePermit>,
}

impl ClientLease {
    /// Explicitly release the client back to the pool with cooldown
    ///
    /// This is the **preferred** way to release a client. The cooldown
    /// prevents nonce race conditions by ensuring transactions have
    /// time to propagate before the wallet is reused.
    ///
    /// # Example
    /// ```rust,no_run
    /// # async fn example() {
    /// // lease.release().await; // Explicit release with cooldown
    /// # }
    /// ```
    pub async fn release(self) {
        let pool = self.pool.clone();
        let index = self.index;
        let nonce_config = pool.config.nonce.clone();

        tokio::spawn(async move {
            // Use configurable cooldown with adaptive backoff
            // Base cooldown prevents nonce races by ensuring transactions
            // have time to propagate before the wallet is reused
            let cooldown_ms = nonce_config
                .base_cooldown_ms
                .max(nonce_config.min_cooldown_ms);

            tracing::debug!("Releasing wallet {} with {}ms cooldown", index, cooldown_ms);

            tokio::time::sleep(std::time::Duration::from_millis(cooldown_ms)).await;
            pool.release_wallet(index).await;
        });
    }

    /// Release immediately without cooldown
    ///
    /// **WARNING**: This may cause nonce races if used incorrectly.
    /// Only use this if you're certain the transaction has been confirmed.
    pub async fn release_immediate(self) {
        self.pool.release_wallet(self.index).await;
    }
}

impl Drop for ClientLease {
    /// Automatic release on drop (with warning)
    ///
    /// This is a safety fallback. If you see this warning in logs,
    /// you should update your code to call `lease.release().await` explicitly.
    fn drop(&mut self) {
        tracing::warn!(
            target: "client_pool",
            "ClientLease dropped without explicit release(). \
             Using automatic release with cooldown. \
             Prefer calling lease.release().await explicitly."
        );
        let pool = self.pool.clone();
        let index = self.index;
        let nonce_config = pool.config.nonce.clone();

        tokio::spawn(async move {
            // Use configurable cooldown with adaptive backoff
            let cooldown_ms = nonce_config
                .base_cooldown_ms
                .max(nonce_config.min_cooldown_ms);

            tracing::debug!(
                "Auto-releasing wallet {} with {}ms cooldown",
                index,
                cooldown_ms
            );

            tokio::time::sleep(std::time::Duration::from_millis(cooldown_ms)).await;
            pool.release_wallet(index).await;
        });
    }
}

impl std::ops::Deref for ClientLease {
    type Target = TempoClient;

    /// Allows transparent access to the underlying client
    ///
    /// This enables using the lease directly as if it were the client:
    /// ```rust,ignore
    /// let address = lease.address(); // Calls TempoClient::address()
    /// ```
    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl ClientPool {
    /// Creates a new client pool
    ///
    /// Initializes the pool with wallets from the wallet manager.
    /// Clients are created lazily on first use. Use `with_proxies()` to add proxies.
    ///
    /// # Arguments
    ///
    /// * `config` - The TempoSpammerConfig configuration object
    /// * `db` - Database manager for logging
    /// * `wallet_password` - Optional password for wallet decryption
    ///
    /// # Returns
    ///
    /// Returns `Result<Self>` which is Ok if the pool was created successfully.
    ///
    /// # Errors
    ///
    /// Can fail if:
    /// - Wallet manager initialization fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tempo_spammer::ClientPool;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let pool = ClientPool::new(
    ///     config,
    ///     db,
    ///     Some("password".to_string()),
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(
        config: Config,
        db: Arc<core_logic::database::DatabaseManager>,
        wallet_password: Option<String>,
        connection_semaphore_size: usize,
    ) -> Result<Self> {
        let wallet_manager = Arc::new(WalletManager::new()?);

        // Initialize nonce managers
        let nonce_manager = Some(Arc::new(crate::NonceManager::new()));
        let robust_nonce_manager = Some(Arc::new(crate::RobustNonceManager::new()));

        // Initialize sharded nonce managers for per-wallet isolation
        // This reduces contention when managing 2500+ wallets
        let shard_count = config.nonce.shard_count;
        let sharded_nonce_managers: Vec<_> = (0..shard_count)
            .map(|_| Arc::new(crate::NonceManager::new()))
            .collect();
        let sharded_robust_nonce_managers: Vec<_> = (0..shard_count)
            .map(|_| Arc::new(crate::RobustNonceManager::new()))
            .collect();

        // Initialize proxy banlist
        let proxy_banlist = Some(crate::proxy_health::ProxyBanlist::new(10)); // 10 min ban

        // Initialize O(1) wallet selection structures
        let total_wallets = wallet_manager.count();
        let initial_available: Vec<usize> = (0..total_wallets).collect();
        let initial_positions: HashMap<usize, usize> = (0..total_wallets).map(|i| (i, i)).collect();

        Ok(Self {
            wallet_manager,
            wallet_password,
            clients: RwLock::new(HashMap::new()),
            http_clients: RwLock::new(HashMap::new()),
            proxies: Vec::new(), // Empty initially, use with_proxies() to add
            config,
            locked_wallets: tokio::sync::Mutex::new(std::collections::HashSet::new()),
            nonce_manager,
            robust_nonce_manager,
            sharded_nonce_managers,
            sharded_robust_nonce_managers,
            proxy_banlist,
            db: Some(db),
            // O(1) optimization fields
            available_wallets: RwLock::new(initial_available),
            available_positions: RwLock::new(initial_positions),
            proxy_cache: RwLock::new(HashMap::new()),
            // Proxy rotation counter for even distribution
            proxy_rotation_counter: AtomicUsize::new(0),
            connection_semaphore: Arc::new(tokio::sync::Semaphore::new(connection_semaphore_size)),
        })
    }

    /// Sets the proxies for this pool
    ///
    /// This is a builder-style method that consumes self and returns it
    /// with the proxies configured.
    ///
    /// # Arguments
    ///
    /// * `proxies` - Vector of proxy configurations
    ///
    /// # Returns
    ///
    /// Self with proxies configured
    pub fn with_proxies(mut self, proxies: Vec<crate::tasks::ProxyConfig>) -> Self {
        self.proxies = proxies;
        self
    }

    /// Sets the proxy banlist for this pool
    ///
    /// This is a builder-style method that consumes self and returns it
    /// with the proxy banlist configured.
    ///
    /// # Arguments
    ///
    /// * `banlist` - Proxy banlist for health tracking
    ///
    /// # Returns
    ///
    /// Self with proxy banlist configured
    pub fn with_proxy_banlist(mut self, banlist: crate::proxy_health::ProxyBanlist) -> Self {
        self.proxy_banlist = Some(banlist);
        self
    }

    /// Attempts to acquire an available client using O(1) fast path
    ///
    /// This is the primary method for acquiring clients. It uses an optimized O(1)
    /// algorithm that maintains an incremental set of available wallets.
    ///
    /// Falls back to legacy O(n) method if fast path fails.
    ///
    /// # Returns
    ///
    /// - `Some(ClientLease)` - A leased client ready for use
    /// - `None` - No wallets available (all in use)
    pub async fn try_acquire_client(self: &Arc<Self>) -> Option<ClientLease> {
        // Try fast O(1) path first
        if let Some(lease) = self.try_acquire_client_fast().await {
            return Some(lease);
        }

        // Fallback to legacy O(n) path if fast path fails
        self.try_acquire_client_legacy().await
    }

    /// Fast O(1) client acquisition
    ///
    /// Uses swap-remove technique for O(1) wallet selection.
    /// Maintains available_wallets vec and available_positions map.
    async fn try_acquire_client_fast(self: &Arc<Self>) -> Option<ClientLease> {
        // 0. Acquire connection permit (prevents overload before we even lock a wallet)
        // If pool is saturated, return None immediately to trigger backoff in worker loop
        let permit = match self.connection_semaphore.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => return None,
        };

        // 1. Fast check: Get available count
        let available_count = {
            let available = self.available_wallets.read().await;
            available.len()
        };

        if available_count == 0 {
            return None;
        }

        // 2. Random selection with retry logic for banned proxies
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 5; // Increased from 1 to 5 for better resilience

        loop {
            // Pick random wallet from available set using fast RNG
            let (selected_wallet, random_idx) = {
                let available = self.available_wallets.read().await;
                if available.is_empty() {
                    return None;
                }

                // Use fastrand for better performance (no expensive RNG initialization)
                let idx = fastrand::usize(0..available.len());
                (available[idx], idx)
            };

            // 3. Check proxy health with caching
            let proxy_ok = self.check_proxy_cached(selected_wallet).await;

            if !proxy_ok {
                // Proxy banned - don't remove wallet, just retry with different wallet
                // This prevents wallet starvation when proxies fail
                if retry_count < MAX_RETRIES {
                    retry_count += 1;
                    // Brief delay to let proxy recover and avoid hammering
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    continue;
                } else {
                    // Max retries reached, return None to try legacy path
                    return None;
                }
            }

            // 4. Lock the wallet (O(1) swap-remove)
            if !self.lock_wallet_fast(selected_wallet, random_idx).await {
                // Race condition - wallet was taken, try again
                if retry_count < MAX_RETRIES {
                    retry_count += 1;
                    continue;
                } else {
                    return None;
                }
            }

            // 5. Create/get client
            match self.get_or_create_client(selected_wallet).await {
                Ok(client) => {
                    return Some(ClientLease {
                        client,
                        index: selected_wallet,
                        pool: self.clone(),
                        permit: Some(permit),
                    });
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to create client for wallet {}: {}",
                        selected_wallet,
                        e
                    );
                    self.unlock_wallet_fast(selected_wallet).await;
                    return None;
                }
            }
        }
    }

    /// Legacy O(n) client acquisition (kept as fallback)
    ///
    /// Scans all wallets linearly. Slower but handles edge cases.
    async fn try_acquire_client_legacy(self: &Arc<Self>) -> Option<ClientLease> {
        let total_wallets = self.wallet_manager.count();
        if total_wallets == 0 {
            return None;
        }

        // Get locked wallets
        let locked = self.locked_wallets.lock().await;

        // Build list of available wallet indices - O(n) scan
        let mut available: Vec<usize> =
            (0..total_wallets).filter(|i| !locked.contains(i)).collect();

        drop(locked);

        if available.is_empty() {
            return None;
        }

        // Filter by proxy health - check if ANY proxy is available for this wallet
        // With rotating proxy assignment, wallets can use any healthy proxy
        if let Some(ref banlist) = self.proxy_banlist {
            // Check if at least one proxy is healthy
            let mut has_healthy_proxy = self.proxies.is_empty(); // true if no proxies

            if !has_healthy_proxy {
                // Check if any proxy is not banned
                for idx in 0..self.proxies.len() {
                    if !banlist.is_banned(idx).await {
                        has_healthy_proxy = true;
                        break;
                    }
                }
            }

            if !has_healthy_proxy {
                // All proxies banned - try direct connection
                tracing::warn!("All proxies banned, falling back to direct connection");
            }
            // Don't filter wallets - with rotation they can use any healthy proxy
        }

        if available.is_empty() {
            return None;
        }

        // Random selection using fastrand
        let selected_idx = available[fastrand::usize(0..available.len())];

        // Lock the wallet
        let mut locked = self.locked_wallets.lock().await;
        if !locked.insert(selected_idx) {
            return None;
        }
        drop(locked);

        // Get or create the client
        let client = self.get_or_create_client(selected_idx).await;

        match client {
            Ok(client) => Some(ClientLease {
                client,
                index: selected_idx,
                pool: self.clone(),
                // Legacy path doesn't limit connections strictly, or acquire explicitly here if needed
                // For now we can assume fast path is primary
                permit: None,
            }),
            Err(e) => {
                // Failed to create client, release the lock
                tracing::error!("Failed to create client for wallet {}: {}", selected_idx, e);
                self.release_wallet(selected_idx).await;
                None
            }
        }
    }

    /// Gets an existing client from cache or creates a new one
    async fn get_or_create_client(&self, wallet_idx: usize) -> Result<TempoClient> {
        // Check cache first
        {
            let clients = self.clients.read().await;
            if let Some(client) = clients.get(&wallet_idx) {
                return Ok(client.clone());
            }
        }

        // Need to create a new client
        let wallet = self
            .wallet_manager
            .get_wallet(wallet_idx, self.wallet_password.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get wallet {}: {}", wallet_idx, e))?;

        // Phase 2: Atomic proxy selection - calculate once, use everywhere
        // This prevents race conditions where proxy_idx changes between selection and client creation
        let proxy_idx = if self.proxies.is_empty() {
            None
        } else {
            // Use atomic counter for round-robin selection
            let idx =
                self.proxy_rotation_counter.fetch_add(1, Ordering::SeqCst) % self.proxies.len();
            Some(idx)
        };

        let proxy_config = proxy_idx.map(|idx| &self.proxies[idx]);

        // Get or create HTTP client for this proxy configuration
        // Try to create client with proxy first, fallback to direct connection
        let (client, used_proxy_idx) = match self
            .try_create_client_with_fallback(
                wallet_idx,
                &wallet.evm_private_key,
                proxy_idx,
                proxy_config,
            )
            .await
        {
            Ok((c, idx)) => (c, idx),
            Err(e) => {
                tracing::error!(
                    "Failed to create TempoClient for wallet {}: {:?}. RPC: {}, Proxy: {:?}",
                    wallet_idx,
                    e,
                    self.config.rpc_url,
                    proxy_config.map(|p| &p.url)
                );
                return Err(e).with_context(|| {
                    format!("Failed to create TempoClient for wallet {}", wallet_idx)
                });
            }
        };

        // Update proxy_idx_for_client to reflect what was actually used
        let proxy_idx_for_client = used_proxy_idx;

        // Cache the client
        let mut clients = self.clients.write().await;
        clients.insert(wallet_idx, client.clone());

        Ok(client)
    }

    /// Get the appropriate nonce manager for a wallet index
    ///
    /// Returns sharded manager if per_wallet is enabled, otherwise returns shared manager
    fn get_nonce_manager(&self, wallet_idx: usize) -> Option<Arc<crate::NonceManager>> {
        if self.config.nonce.per_wallet && !self.sharded_nonce_managers.is_empty() {
            let shard = wallet_idx % self.sharded_nonce_managers.len();
            Some(self.sharded_nonce_managers[shard].clone())
        } else {
            self.nonce_manager.clone()
        }
    }

    /// Get the appropriate robust nonce manager for a wallet index
    ///
    /// Returns sharded manager if per_wallet is enabled, otherwise returns shared manager
    fn get_robust_nonce_manager(
        &self,
        wallet_idx: usize,
    ) -> Option<Arc<crate::RobustNonceManager>> {
        if self.config.nonce.per_wallet && !self.sharded_robust_nonce_managers.is_empty() {
            let shard = wallet_idx % self.sharded_robust_nonce_managers.len();
            Some(self.sharded_robust_nonce_managers[shard].clone())
        } else {
            self.robust_nonce_manager.clone()
        }
    }

    /// Try to create client with proxy, fallback to direct connection on failure
    async fn try_create_client_with_fallback(
        &self,
        wallet_idx: usize,
        private_key: &str,
        proxy_idx: Option<usize>,
        proxy_config: Option<&crate::tasks::ProxyConfig>,
    ) -> Result<(TempoClient, Option<usize>)> {
        // Get sharded nonce managers for this wallet
        let nonce_manager = self.get_nonce_manager(wallet_idx);
        let robust_nonce_manager = self.get_robust_nonce_manager(wallet_idx);

        // First attempt: Try with proxy if available
        if let Some(config) = proxy_config {
            match self
                .get_or_create_http_client(Some(config.url.clone()))
                .await
            {
                Ok(reqwest_client) => {
                    match TempoClient::new_from_reqwest(
                        &self.config.rpc_url,
                        private_key,
                        reqwest_client,
                        Some(config.clone()),
                        proxy_idx,
                        nonce_manager.clone(),
                        robust_nonce_manager.clone(),
                        self.config.nonce.use_pending_count,
                    )
                    .await
                    {
                        Ok(client) => return Ok((client, proxy_idx)),
                        Err(e) => {
                            // Proxy failed, ban it and try direct connection
                            tracing::warn!(
                                "Proxy {:?} failed for wallet {}, trying direct connection. Error: {:?}",
                                config.url,
                                wallet_idx,
                                e
                            );
                            if let Some(idx) = proxy_idx {
                                if let Some(ref banlist) = self.proxy_banlist {
                                    banlist.ban(idx).await;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to create HTTP client for proxy {:?}: {:?}. Trying direct connection.",
                        config.url,
                        e
                    );
                }
            }
        }

        // Second attempt: Direct connection (no proxy)
        tracing::info!("Using direct connection for wallet {}", wallet_idx);
        let direct_client = self.get_or_create_http_client(None).await?;
        let client = TempoClient::new_from_reqwest(
            &self.config.rpc_url,
            private_key,
            direct_client,
            None,
            None,
            nonce_manager,
            robust_nonce_manager,
            self.config.nonce.use_pending_count,
        )
        .await
        .context("Failed to create TempoClient with direct connection")?;

        Ok((client, None))
    }

    /// Gets or creates an HTTP client for a proxy configuration
    async fn get_or_create_http_client(
        &self,
        proxy_url: Option<String>,
    ) -> Result<reqwest::Client> {
        // Check cache first
        {
            let http_clients = self.http_clients.read().await;
            if let Some(client) = http_clients.get(&proxy_url) {
                return Ok(client.clone());
            }
        }

        // Create new HTTP client with connection limits to prevent pool exhaustion
        let mut client_builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60)) // Increased for large batches
            .connect_timeout(std::time::Duration::from_secs(10))
            .pool_idle_timeout(std::time::Duration::from_secs(30)) // Close idle connections after 30s
            .pool_max_idle_per_host(10); // Limit connections per proxy to prevent exhaustion

        // Configure proxy if specified
        if let Some(ref url) = proxy_url {
            if let Some(proxy_config) = self.proxies.iter().find(|p| p.url == *url) {
                let proxy = reqwest::Proxy::all(url)
                    .with_context(|| format!("Failed to create proxy for URL: {}", url))?;

                if let (Some(username), Some(password)) =
                    (&proxy_config.username, &proxy_config.password)
                {
                    let proxy = proxy.basic_auth(username, password);
                    client_builder = client_builder.proxy(proxy);
                } else {
                    client_builder = client_builder.proxy(proxy);
                }
            }
        }

        let client = client_builder
            .build()
            .context("Failed to build reqwest client")?;

        // Phase 4: Pre-warm the connection pool by sending a dummy request
        // This establishes initial connections before real traffic hits the pool
        let warmup_client = client.clone();
        let warmup_url = self.config.rpc_url.clone();
        let proxy_url_for_warmup = proxy_url.clone();
        tokio::spawn(async move {
            match warmup_client
                .head(&warmup_url)
                .timeout(std::time::Duration::from_secs(3))
                .send()
                .await
            {
                Ok(_) => tracing::debug!(
                    "HTTP client pool warmed up for proxy: {:?}",
                    proxy_url_for_warmup
                ),
                Err(e) => tracing::warn!(
                    "Pool warmup failed for proxy {:?}: {}",
                    proxy_url_for_warmup,
                    e
                ),
            }
        });

        // Cache the HTTP client
        let mut http_clients = self.http_clients.write().await;
        http_clients.insert(proxy_url, client.clone());

        Ok(client)
    }

    /// Releases a wallet back to the pool
    ///
    /// Internal method called automatically by [`ClientLease::drop`].
    /// Removes the wallet index from the locked set and adds back to available,
    /// making it available for other workers.
    ///
    /// # Arguments
    ///
    /// * `index` - The wallet index to release
    ///
    /// # Note
    ///
    /// This method is typically not called directly. Use the [`ClientLease`]
    /// RAII pattern instead for automatic release.
    ///
    /// Uses O(1) fast path for adding back to available set.
    pub async fn release_wallet(&self, index: usize) {
        // Use O(1) fast unlock
        self.unlock_wallet_fast(index).await;
    }

    /// Returns the number of available (non-locked) wallets
    ///
    /// Useful for monitoring pool saturation and load balancing decisions.
    /// Uses O(1) available_wallets vec for fast lookup.
    ///
    /// # Returns
    ///
    /// The count of wallets not currently leased.
    pub async fn available_count(&self) -> usize {
        let available = self.available_wallets.read().await;
        available.len()
    }

    /// Returns the total number of wallets in the pool
    ///
    /// # Returns
    ///
    /// Total count of wallets managed by this pool.
    pub fn total_count(&self) -> usize {
        self.wallet_manager.count()
    }

    /// Returns the total number of wallets in the pool
    ///
    /// Alias for `total_count()` for convenience.
    ///
    /// # Returns
    ///
    /// Total count of wallets managed by this pool.
    pub fn count(&self) -> usize {
        self.total_count()
    }

    // === O(1) Wallet Selection Helper Methods ===

    /// Check proxy health with 30-second caching
    ///
    /// Returns true if any proxy is available (not banned or no proxy)
    /// With rotating proxy assignment, we check if there are ANY healthy proxies
    async fn check_proxy_cached(&self, _wallet_idx: usize) -> bool {
        if self.proxies.is_empty() {
            return true; // No proxy = always available
        }

        // With rotating assignment, check if at least one proxy is healthy
        // The rotation logic will skip banned proxies automatically
        if let Some(ref banlist) = self.proxy_banlist {
            // Check if any proxy is healthy (not banned)
            let mut has_healthy_proxy = false;
            for idx in 0..self.proxies.len() {
                if !banlist.is_banned(idx).await {
                    has_healthy_proxy = true;
                    break;
                }
            }

            if !has_healthy_proxy {
                tracing::warn!("All proxies currently banned - will retry with backoff");
                return false;
            }
        }

        // At least one proxy is available
        true
    }

    /// O(1) removal from available set using swap-remove
    ///
    /// Removes wallet from available_wallets vec and updates positions map
    async fn remove_from_available(&self, wallet_idx: usize) {
        let mut available = self.available_wallets.write().await;
        let mut positions = self.available_positions.write().await;

        if let Some(&pos) = positions.get(&wallet_idx) {
            let last_idx = available.len().saturating_sub(1);

            if pos < available.len() {
                let last_wallet = available[last_idx];

                // Swap with last element (O(1))
                available.swap(pos, last_idx);
                available.pop();

                // Update position of swapped element
                if pos != last_idx && pos < available.len() {
                    positions.insert(last_wallet, pos);
                }

                positions.remove(&wallet_idx);
            }
        }
    }

    /// O(1) wallet lock using swap-remove
    ///
    /// Removes from available and adds to locked set
    /// Returns false if wallet already locked (race condition)
    async fn lock_wallet_fast(&self, wallet_idx: usize, available_idx: usize) -> bool {
        // Add to locked set first
        {
            let mut locked = self.locked_wallets.lock().await;
            if !locked.insert(wallet_idx) {
                return false; // Already locked
            }
        }

        // Remove from available using swap-remove
        {
            let mut available = self.available_wallets.write().await;
            let mut positions = self.available_positions.write().await;

            if available_idx < available.len() && available[available_idx] == wallet_idx {
                let last_idx = available.len() - 1;
                let last_wallet = available[last_idx];

                // Swap-remove (O(1))
                available.swap(available_idx, last_idx);
                available.pop();

                // Update position of swapped element
                if available_idx != last_idx {
                    positions.insert(last_wallet, available_idx);
                }

                positions.remove(&wallet_idx);
            }
        }

        true
    }

    /// O(1) wallet unlock - adds back to available
    ///
    /// Removes from locked set and pushes to available vec
    async fn unlock_wallet_fast(&self, wallet_idx: usize) {
        // Remove from locked
        {
            let mut locked = self.locked_wallets.lock().await;
            locked.remove(&wallet_idx);
        }

        // Add back to available
        {
            let mut available = self.available_wallets.write().await;
            let mut positions = self.available_positions.write().await;

            let new_pos = available.len();
            available.push(wallet_idx);
            positions.insert(wallet_idx, new_pos);
        }
    }

    /// Gets a client by wallet index
    ///
    /// This method retrieves or creates a client for the specified wallet index.
    /// Unlike `try_acquire_client`, this does not lock the wallet - it's meant for
    /// direct access when you know which wallet you want to use.
    ///
    /// # Arguments
    ///
    /// * `wallet_idx` - The index of the wallet to get a client for
    ///
    /// # Returns
    ///
    /// Returns `Ok(TempoClient)` if successful, `Err` if the wallet index is out of bounds
    /// or if client creation fails.
    pub async fn get_client(&self, wallet_idx: usize) -> Result<TempoClient> {
        // Check if wallet index is valid
        if wallet_idx >= self.wallet_manager.count() {
            anyhow::bail!("Wallet index {} out of bounds", wallet_idx);
        }

        // Get or create the client
        self.get_or_create_client(wallet_idx).await
    }

    /// Gets a client with a rotated proxy
    ///
    /// This method creates a new client for the specified wallet with a different
    /// proxy than what would normally be assigned. This is useful for retry logic
    /// when a proxy is failing.
    ///
    /// # Arguments
    ///
    /// * `wallet_idx` - The index of the wallet to get a client for
    /// * `rotation_offset` - Offset to apply to proxy selection (allows trying different proxies)
    ///
    /// # Returns
    ///
    /// Returns `Ok(TempoClient)` if successful, `Err` if client creation fails.
    pub async fn get_client_with_rotated_proxy(
        &self,
        wallet_idx: usize,
        rotation_offset: usize,
    ) -> Result<TempoClient> {
        // Check if wallet index is valid
        if wallet_idx >= self.wallet_manager.count() {
            anyhow::bail!("Wallet index {} out of bounds", wallet_idx);
        }

        // Get wallet
        let wallet = self
            .wallet_manager
            .get_wallet(wallet_idx, self.wallet_password.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get wallet {}: {}", wallet_idx, e))?;

        // Select a different proxy (rotate by using wallet_idx + offset)
        // Ensure offset is non-zero if possible to actually rotate
        let proxy_config = if self.proxies.is_empty() {
            None
        } else {
            let proxy_idx = (wallet_idx + rotation_offset) % self.proxies.len();
            Some(&self.proxies[proxy_idx])
        };

        // Create a fresh HTTP client (don't use cache for rotated proxy)
        let mut client_builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .pool_idle_timeout(None);

        // Configure proxy if specified
        if let Some(ref proxy_config) = proxy_config {
            let proxy = reqwest::Proxy::all(&proxy_config.url)
                .with_context(|| format!("Failed to create proxy for URL: {}", proxy_config.url))?;

            if let (Some(username), Some(password)) =
                (&proxy_config.username, &proxy_config.password)
            {
                let proxy = proxy.basic_auth(username, password);
                client_builder = client_builder.proxy(proxy);
            } else {
                client_builder = client_builder.proxy(proxy);
            }
        }

        let reqwest_client = client_builder
            .build()
            .context("Failed to build reqwest client")?;

        // Create the TempoClient
        let client = TempoClient::new_from_reqwest(
            &self.config.rpc_url,
            &wallet.evm_private_key,
            reqwest_client,
            proxy_config.cloned(),
            proxy_config.map(|_| (wallet_idx + rotation_offset) % self.proxies.len()),
            self.nonce_manager.clone(),
            self.robust_nonce_manager.clone(),
            self.config.nonce.use_pending_count,
        )
        .await
        .with_context(|| {
            format!(
                "Failed to create TempoClient for wallet {} with rotated proxy",
                wallet_idx
            )
        })?;

        Ok(client)
    }
}
