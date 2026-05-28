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
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

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
    /// Store with last_used timestamp for eviction (Phase 1.2)
    http_clients: RwLock<HashMap<Option<String>, (reqwest::Client, Instant)>>,
    /// Cache of shared Alloy RpcClients per proxy/RPC configuration
    /// This significantly reduces memory overhead when managing 2500+ wallets
    shared_rpc_clients: RwLock<HashMap<Option<String>, (crate::client::SharedRpcClient, Instant)>>,
    /// Available proxy configurations
    proxies: Vec<crate::tasks::ProxyConfig>,
    /// Spammer configuration
    pub config: Config,
    /// Set of wallet indices currently in use (leased)
    /// Using parking_lot::Mutex for synchronous access in Drop
    pub locked_wallets: Mutex<std::collections::HashSet<usize>>,
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
    /// Whether the lease has been explicitly released
    released: bool,
}

impl ClientLease {
    /// Explicitly release the client back to the pool
    ///
    /// For the spammer, we prefer IMMEDIATE release to maximize throughput.
    pub async fn release(mut self) {
        self.release_with_priority(false).await;
    }

    /// Release with priority (emergency cleanup)
    pub async fn release_with_priority(&mut self, is_emergency: bool) {
        if self.released {
            return;
        }
        self.released = true;

        let pool = self.pool.clone();
        let index = self.index;
        let nonce_config = pool.config.nonce.clone();

        // If emergency or cooldown is small (<1s), ignore it and release immediately
        if is_emergency || nonce_config.base_cooldown_ms < 1000 {
            pool.release_wallet(index);
        } else {
            tokio::spawn(async move {
                let cooldown_ms = nonce_config.base_cooldown_ms.max(nonce_config.min_cooldown_ms);

                tokio::time::sleep(std::time::Duration::from_millis(cooldown_ms)).await;
                pool.release_wallet(index);
            });
        }
    }

    /// Release immediately without cooldown
    pub async fn release_immediate(mut self) {
        if !self.released {
            self.released = true;
            self.pool.release_wallet(self.index);
        }
    }
}

impl Drop for ClientLease {
    /// Automatic release on drop
    fn drop(&mut self) {
        if self.released {
            return;
        }
        self.released = true;

        if let Some(permit) = self.permit.take() {
            drop(permit);
        }

        // Phase 2.1: Synchronous release from Drop
        // Now that release_wallet is synchronous, we can call it directly
        self.pool.release_wallet(self.index);
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
    /// use core_logic::WalletManager;
    /// use std::sync::Arc;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let wallet_manager = Arc::new(WalletManager::new()?);
    /// let pool = ClientPool::new(
    ///     config,
    ///     db,
    ///     wallet_manager,
    ///     Some("password".to_string()),
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(
        config: Config,
        db: Arc<core_logic::database::DatabaseManager>,
        wallet_manager: Arc<WalletManager>,
        wallet_password: Option<String>,
        connection_semaphore_size: usize,
    ) -> Result<Self> {
        // Initialize nonce managers
        let nonce_manager = Some(Arc::new(crate::NonceManager::new()));
        let robust_nonce_manager = Some(Arc::new(crate::RobustNonceManager::new()));

        // Initialize sharded nonce managers for per-wallet isolation
        let shard_count = config.nonce.shard_count;
        let sharded_nonce_managers: Vec<_> = (0..shard_count).map(|_| Arc::new(crate::NonceManager::new())).collect();
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
            shared_rpc_clients: RwLock::new(HashMap::new()),
            proxies: Vec::new(),
            config,
            locked_wallets: Mutex::new(std::collections::HashSet::new()),
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
    pub fn with_proxies(mut self, proxies: Vec<crate::tasks::ProxyConfig>) -> Self {
        self.proxies = proxies;
        self
    }

    /// Sets the proxy banlist for this pool
    pub fn with_proxy_banlist(mut self, banlist: crate::proxy_health::ProxyBanlist) -> Self {
        self.proxy_banlist = Some(banlist);
        self
    }

    /// Attempts to acquire an available client using O(1) fast path
    pub async fn try_acquire_client(self: &Arc<Self>) -> Option<ClientLease> {
        // Try fast O(1) path first
        if let Some(lease) = self.try_acquire_client_fast().await {
            return Some(lease);
        }

        // Fallback to legacy O(n) path if fast path fails
        self.try_acquire_client_legacy().await
    }

    /// Fast O(1) client acquisition
    async fn try_acquire_client_fast(self: &Arc<Self>) -> Option<ClientLease> {
        // 0. Acquire connection permit
        let permit = match self.connection_semaphore.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => return None,
        };

        // 1. Fast check: Get available count
        let available_count = {
            let available = self.available_wallets.read();
            available.len()
        };

        if available_count == 0 {
            return None;
        }

        // 2. Random selection with retry logic for banned proxies
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 5;

        loop {
            // Pick random wallet from available set
            let (selected_wallet, random_idx) = {
                let available = self.available_wallets.read();
                if available.is_empty() {
                    return None;
                }

                let idx = fastrand::usize(0..available.len());
                (available[idx], idx)
            };

            // 3. Check proxy health with caching
            let proxy_ok = self.check_proxy_cached(selected_wallet).await;

            if !proxy_ok {
                if retry_count < MAX_RETRIES {
                    retry_count += 1;
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    continue;
                } else {
                    return None;
                }
            }

            // 4. Lock the wallet (O(1) swap-remove)
            if !self.lock_wallet_fast(selected_wallet, random_idx) {
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
                        released: false,
                    });
                },
                Err(e) => {
                    tracing::error!("Failed to create client for wallet {}: {}", selected_wallet, e);
                    self.unlock_wallet_fast(selected_wallet);
                    return None;
                },
            }
        }
    }

    /// Legacy O(n) client acquisition (kept as fallback)
    async fn try_acquire_client_legacy(self: &Arc<Self>) -> Option<ClientLease> {
        let total_wallets = self.wallet_manager.count();
        if total_wallets == 0 {
            return None;
        }

        // Build list of available wallet indices - O(n) scan
        let available: Vec<usize> = {
            let locked = self.locked_wallets.lock();
            (0..total_wallets).filter(|i| !locked.contains(i)).collect()
        };

        if available.is_empty() {
            return None;
        }

        // Filter by proxy health
        if let Some(ref banlist) = self.proxy_banlist {
            let mut has_healthy_proxy = self.proxies.is_empty();
            if !has_healthy_proxy {
                for idx in 0..self.proxies.len() {
                    if !banlist.is_banned(idx).await {
                        has_healthy_proxy = true;
                        break;
                    }
                }
            }
            if !has_healthy_proxy {
                tracing::warn!("All proxies banned, falling back to direct connection");
            }
        }

        // Random selection
        let selected_idx = available[fastrand::usize(0..available.len())];

        // Lock the wallet
        {
            let mut locked = self.locked_wallets.lock();
            if !locked.insert(selected_idx) {
                return None;
            }
        }

        // Get or create the client
        let client = self.get_or_create_client(selected_idx).await;

        match client {
            Ok(client) => Some(ClientLease {
                client,
                index: selected_idx,
                pool: self.clone(),
                permit: None,
                released: false,
            }),
            Err(e) => {
                tracing::error!("Failed to create client for wallet {}: {}", selected_idx, e);
                self.release_wallet(selected_idx);
                None
            },
        }
    }

    /// Gets an existing client from cache or creates a new one
    async fn get_or_create_client(&self, wallet_idx: usize) -> Result<TempoClient> {
        // Check cache first
        {
            let clients = self.clients.read();
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

        // Phase 2: Atomic proxy selection
        let proxy_idx = if self.proxies.is_empty() {
            None
        } else {
            // Use atomic counter for round-robin selection
            let idx = self.proxy_rotation_counter.fetch_add(1, Ordering::SeqCst) % self.proxies.len();
            Some(idx)
        };

        let proxy_config = proxy_idx.map(|idx| &self.proxies[idx]);

        // Get or create HTTP client for this proxy configuration
        let (client, _used_proxy_idx) = match self
            .try_create_client_with_fallback(wallet_idx, &wallet.evm_private_key, proxy_idx, proxy_config)
            .await
        {
            Ok((c, idx)) => (c, idx),
            Err(e) => {
                tracing::error!("Failed to create TempoClient for wallet {}: {:?}", wallet_idx, e);
                return Err(e).with_context(|| format!("Failed to create TempoClient for wallet {}", wallet_idx));
            },
        };

        // Cache the client
        let mut clients = self.clients.write();
        clients.insert(wallet_idx, client.clone());

        Ok(client)
    }

    /// Get the appropriate nonce manager for a wallet index
    fn get_nonce_manager(&self, wallet_idx: usize) -> Option<Arc<crate::NonceManager>> {
        if self.config.nonce.per_wallet && !self.sharded_nonce_managers.is_empty() {
            let shard = wallet_idx % self.sharded_nonce_managers.len();
            Some(self.sharded_nonce_managers[shard].clone())
        } else {
            self.nonce_manager.clone()
        }
    }

    /// Get the appropriate robust nonce manager for a wallet index
    fn get_robust_nonce_manager(&self, wallet_idx: usize) -> Option<Arc<crate::RobustNonceManager>> {
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
            match self.get_or_create_http_client(Some(config.url.clone())).await {
                Ok(reqwest_client) => {
                    // Try to get or create shared RpcClient for this proxy
                    let shared_rpc = self
                        .get_or_create_shared_rpc_client(Some(config.url.clone()), reqwest_client.clone())
                        .await
                        .ok();

                    match TempoClient::new_from_reqwest(
                        &self.config.rpc_url,
                        private_key,
                        reqwest_client,
                        Some(config.clone()),
                        proxy_idx,
                        nonce_manager.clone(),
                        robust_nonce_manager.clone(),
                        self.config.nonce.use_pending_count,
                        shared_rpc,
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
                        },
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        "Failed to create HTTP client for proxy {:?}: {:?}. Trying direct connection.",
                        config.url,
                        e
                    );
                },
            }
        }

        // Second attempt: Direct connection (no proxy)
        tracing::info!("Using direct connection for wallet {}", wallet_idx);
        let direct_client = self.get_or_create_http_client(None).await?;
        let shared_rpc = self
            .get_or_create_shared_rpc_client(None, direct_client.clone())
            .await
            .ok();

        let client = TempoClient::new_from_reqwest(
            &self.config.rpc_url,
            private_key,
            direct_client,
            None,
            None,
            nonce_manager,
            robust_nonce_manager,
            self.config.nonce.use_pending_count,
            shared_rpc,
        )
        .await
        .context("Failed to create TempoClient with direct connection")?;

        Ok((client, None))
    }

    /// Gets or creates a shared Alloy RpcClient for a proxy/RPC configuration
    async fn get_or_create_shared_rpc_client(
        &self,
        proxy_url: Option<String>,
        reqwest_client: reqwest::Client,
    ) -> Result<crate::client::SharedRpcClient> {
        // Check cache first
        {
            let mut shared = self.shared_rpc_clients.write();
            if let Some((rpc_client, last_used)) = shared.get_mut(&proxy_url) {
                *last_used = Instant::now();
                return Ok(rpc_client.clone());
            }
        }

        // Create a new RpcClient
        use alloy::rpc::client::ClientBuilder;
        use alloy::transports::http::Http;
        use url::Url;

        let http_transport = Http::with_client(
            reqwest_client,
            self.config.rpc_url.parse::<Url>().context("Invalid RPC URL")?,
        );

        let rpc_client = ClientBuilder::default()
            .layer(alloy::transports::layers::RetryBackoffLayer::new(5, 100, 2000))
            .transport(http_transport, true);

        // Cache the RpcClient
        let mut shared = self.shared_rpc_clients.write();
        shared.insert(proxy_url, (rpc_client.clone(), Instant::now()));

        Ok(rpc_client)
    }

    /// Evicts idle shared RpcClients from the cache
    pub fn evict_idle_shared_rpc_clients(&self, max_idle: Duration) -> usize {
        let mut shared = self.shared_rpc_clients.write();
        let before_count = shared.len();

        shared.retain(|key, (_, last_used)| key.is_none() || last_used.elapsed() < max_idle);

        before_count - shared.len()
    }

    /// Gets or creates an HTTP client for a proxy configuration
    async fn get_or_create_http_client(&self, proxy_url: Option<String>) -> Result<reqwest::Client> {
        // Check cache first
        {
            let mut http_clients = self.http_clients.write();
            if let Some((client, last_used)) = http_clients.get_mut(&proxy_url) {
                *last_used = Instant::now();
                return Ok(client.clone());
            }
        }

        // Create new HTTP client
        let mut client_builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .connect_timeout(std::time::Duration::from_secs(10))
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .pool_max_idle_per_host(10);

        // Configure proxy if specified
        if let Some(ref url) = proxy_url {
            if let Some(proxy_config) = self.proxies.iter().find(|p| p.url == *url) {
                let proxy =
                    reqwest::Proxy::all(url).with_context(|| format!("Failed to create proxy for URL: {}", url))?;

                if let (Some(username), Some(password)) = (&proxy_config.username, &proxy_config.password) {
                    let proxy = proxy.basic_auth(username, password);
                    client_builder = client_builder.proxy(proxy);
                } else {
                    client_builder = client_builder.proxy(proxy);
                }
            }
        }

        let client = client_builder.build().context("Failed to build reqwest client")?;

        // Cache the HTTP client
        let mut http_clients = self.http_clients.write();
        http_clients.insert(proxy_url, (client.clone(), Instant::now()));

        Ok(client)
    }

    /// Evicts idle HTTP clients from the cache (Phase 1.2)
    pub fn evict_idle_http_clients(&self, max_idle: Duration) -> usize {
        let mut http_clients = self.http_clients.write();
        let before_count = http_clients.len();

        // Don't evict the direct connection client (None)
        http_clients.retain(|key, (_, last_used)| key.is_none() || last_used.elapsed() < max_idle);

        before_count - http_clients.len()
    }

    /// Clears the client cache to free up memory (Phase 3.3)
    pub fn clear_client_cache(&self) {
        let mut clients = self.clients.write();
        let count = clients.len();
        clients.clear();
        tracing::debug!("Cleared ClientPool client cache ({} entries)", count);
    }

    /// Performs a comprehensive cleanup of all internal caches to free RAM
    pub async fn cleanup(&self) {
        self.cleanup_with_priority(false).await;
    }

    /// Comprehensive cleanup with priority based on memory pressure
    pub async fn cleanup_with_priority(&self, is_emergency: bool) {
        // Under emergency, use aggressive 2-minute idle timeout, otherwise 10 minutes
        let idle_timeout = if is_emergency {
            Duration::from_secs(120)
        } else {
            Duration::from_secs(600)
        };

        let wallet_idle_timeout = if is_emergency {
            Duration::from_secs(600) // 10 min
        } else {
            Duration::from_secs(3600) // 1 hour
        };

        self.clear_client_cache();
        self.evict_idle_http_clients(idle_timeout);
        self.evict_idle_shared_rpc_clients(idle_timeout);

        if let Some(nm) = &self.nonce_manager {
            nm.clear().await;
        }

        if let Some(rnm) = &self.robust_nonce_manager {
            rnm.evict_idle_wallets(wallet_idle_timeout).await;
        }

        for snm in &self.sharded_nonce_managers {
            snm.clear().await;
        }

        for srnm in &self.sharded_robust_nonce_managers {
            srnm.evict_idle_wallets(wallet_idle_timeout).await;
        }

        self.wallet_manager.clear_cache().await;
        crate::proxy_health::clear_client_cache().await;
    }

    /// Releases a wallet back to the pool
    pub fn release_wallet(&self, index: usize) {
        // Use O(1) fast unlock
        self.unlock_wallet_fast(index);
    }

    /// Returns the number of available (non-locked) wallets
    pub fn available_count(&self) -> usize {
        let available = self.available_wallets.read();
        available.len()
    }

    /// Returns the total number of wallets in the pool
    pub fn total_count(&self) -> usize {
        self.wallet_manager.count()
    }

    /// Alias for `total_count()` for convenience.
    pub fn count(&self) -> usize {
        self.total_count()
    }

    // === O(1) Wallet Selection Helper Methods ===

    /// Check proxy health with caching
    async fn check_proxy_cached(&self, _wallet_idx: usize) -> bool {
        if self.proxies.is_empty() {
            return true;
        }

        if let Some(ref banlist) = self.proxy_banlist {
            let mut has_healthy_proxy = false;
            for idx in 0..self.proxies.len() {
                if !banlist.is_banned(idx).await {
                    has_healthy_proxy = true;
                    break;
                }
            }

            if !has_healthy_proxy {
                return false;
            }
        }

        true
    }

    /// O(1) removal from available set using swap-remove
    fn remove_from_available(&self, wallet_idx: usize) {
        let mut available = self.available_wallets.write();
        let mut positions = self.available_positions.write();

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
    fn lock_wallet_fast(&self, wallet_idx: usize, available_idx: usize) -> bool {
        // Add to locked set first
        {
            let mut locked = self.locked_wallets.lock();
            if !locked.insert(wallet_idx) {
                return false; // Already locked
            }
        }

        // Remove from available using swap-remove
        {
            let mut available = self.available_wallets.write();
            let mut positions = self.available_positions.write();

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
    fn unlock_wallet_fast(&self, wallet_idx: usize) {
        // Remove from locked
        {
            let mut locked = self.locked_wallets.lock();
            locked.remove(&wallet_idx);
        }

        // Add back to available
        {
            let mut available = self.available_wallets.write();
            let mut positions = self.available_positions.write();

            let new_pos = available.len();
            available.push(wallet_idx);
            positions.insert(wallet_idx, new_pos);
        }
    }

    /// Gets a client by wallet index
    pub async fn get_client(&self, wallet_idx: usize) -> Result<TempoClient> {
        // Check if wallet index is valid
        if wallet_idx >= self.wallet_manager.count() {
            anyhow::bail!("Wallet index {} out of bounds", wallet_idx);
        }

        // Get or create the client
        self.get_or_create_client(wallet_idx).await
    }

    pub async fn get_client_with_rotated_proxy(
        &self,
        wallet_idx: usize,
        rotation_offset: usize,
    ) -> Result<TempoClient> {
        if wallet_idx >= self.wallet_manager.count() {
            anyhow::bail!("Wallet index {} out of bounds", wallet_idx);
        }

        let wallet = self
            .wallet_manager
            .get_wallet(wallet_idx, self.wallet_password.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get wallet {}: {}", wallet_idx, e))?;

        let proxy_config = if self.proxies.is_empty() {
            None
        } else {
            let proxy_idx = (wallet_idx + rotation_offset) % self.proxies.len();
            Some(&self.proxies[proxy_idx])
        };

        let mut client_builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .pool_idle_timeout(None);

        if let Some(proxy_config) = proxy_config {
            let proxy = reqwest::Proxy::all(&proxy_config.url)
                .with_context(|| format!("Failed to create proxy for URL: {}", proxy_config.url))?;

            if let (Some(username), Some(password)) = (&proxy_config.username, &proxy_config.password) {
                let proxy = proxy.basic_auth(username, password);
                client_builder = client_builder.proxy(proxy);
            } else {
                client_builder = client_builder.proxy(proxy);
            }
        }

        let reqwest_client = client_builder.build().context("Failed to build reqwest client")?;

        let shared_rpc = self
            .get_or_create_shared_rpc_client(proxy_config.map(|p| p.url.clone()), reqwest_client.clone())
            .await
            .ok();

        let client = TempoClient::new_from_reqwest(
            &self.config.rpc_url,
            &wallet.evm_private_key,
            reqwest_client,
            proxy_config.cloned(),
            proxy_config.map(|_| (wallet_idx + rotation_offset) % self.proxies.len()),
            self.nonce_manager.clone(),
            self.robust_nonce_manager.clone(),
            self.config.nonce.use_pending_count,
            shared_rpc,
        )
        .await?;

        Ok(client)
    }
}
