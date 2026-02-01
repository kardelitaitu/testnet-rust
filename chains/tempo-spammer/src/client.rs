//! Tempo Client - Alloy-based blockchain provider for Tempo
//!
//! This module provides a high-level client for interacting with the Tempo blockchain.
//! It wraps Alloy's provider with additional features like proxy support, retry logic,
//! and nonce management integration.
//!
//! # Features
//!
//! - **Alloy 1.4.3 Integration**: Modern Ethereum library with 10x faster ABI encoding
//! - **Proxy Support**: HTTP/HTTPS proxy with optional authentication
//! - **Automatic Retries**: Exponential backoff for failed requests (5 retries)
//! - **Connection Pooling**: Efficient HTTP connection reuse
//! - **Nonce Management**: Optional integration with NonceManager for caching
//!
//! # Example
//!
//! ```rust,no_run
//! use tempo_spammer::TempoClient;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create client without proxy
//! let client = TempoClient::new(
//!     "https://rpc.moderato.tempo.xyz",
//!     "0x...", // private key hex
//!     None,    // no proxy
//!     None,    // no proxy index
//! ).await?;
//!
//! // Get wallet address
//! let address = client.address();
//! println!("Wallet: {:?}", address);
//! # Ok(())
//! # }
//! ```
//!
//! # Proxy Configuration
//!
//! The client supports HTTP proxies with optional authentication:
//!
//! ```rust,no_run
//! use tempo_spammer::{TempoClient, tasks::ProxyConfig};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let proxy = ProxyConfig {
//!     url: "http://proxy.example.com:8080".to_string(),
//!     username: Some("user".to_string()),
//!     password: Some("pass".to_string()),
//! };
//!
//! let client = TempoClient::new(
//!     "https://rpc.moderato.tempo.xyz",
//!     "0x...",
//!     Some(&proxy),
//!     Some(0), // proxy index for tracking
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Retry Logic
//!
//! The client automatically retries failed requests with exponential backoff:
//! - Max retries: 5
//! - Initial backoff: 100ms
//! - Max backoff: 2000ms
//!
//! This handles transient network issues and RPC rate limiting.

use super::tasks::ProxyConfig;
use alloy::providers::Provider;
use alloy::rpc::client::ClientBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy::transports::http::Http;
use alloy_primitives::Address;
use anyhow::{Context, Result};
use reqwest::{Client, Proxy};
use std::sync::Arc;
use url::Url;

/// High-level client for Tempo blockchain interactions
///
/// Wraps Alloy's provider with additional features for production use including
/// proxy support, retry logic, and nonce management.
///
/// # Thread Safety
///
/// This struct is `Clone` and thread-safe. The provider is wrapped in an `Arc`
/// allowing multiple clones to share the same underlying connection pool.
///
/// # Fields
///
/// - `provider`: The Alloy provider for RPC calls
/// - `signer`: Local wallet for transaction signing
/// - `chain_id`: Chain identifier (defaults to 42431 for Tempo)
/// - `proxy_config`: Optional proxy configuration
/// - `proxy_index`: Index for tracking which proxy is in use
/// - `nonce_manager`: Optional nonce caching for high-throughput scenarios
#[derive(Clone)]
pub struct TempoClient {
    /// Alloy provider for blockchain interactions
    pub provider: Arc<dyn Provider + Send + Sync>,
    /// Local signer for transaction signing
    pub signer: PrivateKeySigner,
    /// Chain ID for the connected network
    pub chain_id: u64,
    /// Proxy configuration if using a proxy
    pub proxy_config: Option<ProxyConfig>,
    /// Index of the proxy in the pool (for tracking)
    pub proxy_index: Option<usize>,
    /// Optional nonce manager for caching (legacy)
    pub nonce_manager: Option<Arc<crate::NonceManager>>,
    /// Optional robust nonce manager with per-request tracking (recommended)
    pub robust_nonce_manager: Option<Arc<crate::RobustNonceManager>>,
}

impl TempoClient {
    /// Creates a new client from an existing reqwest client
    ///
    /// This is the advanced constructor that allows full control over the HTTP client
    /// configuration. Use this when you need custom timeouts, connection pooling, or
    /// other reqwest-specific settings.
    ///
    /// # Arguments
    ///
    /// * `rpc_url` - The RPC endpoint URL (e.g., "https://rpc.moderato.tempo.xyz")
    /// * `private_key` - Hex-encoded private key for signing transactions
    /// * `reqwest_client` - Pre-configured reqwest client
    /// * `proxy_config` - Optional proxy configuration for tracking
    /// * `proxy_index` - Optional index for proxy identification
    /// * `nonce_manager` - Optional nonce manager for caching
    ///
    /// # Returns
    ///
    /// Returns `Result<Self>` which is Ok if the client was created successfully,
    /// or Err if there was an issue parsing the private key or RPC URL.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tempo_spammer::TempoClient;
    /// use reqwest::Client;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let custom_client = Client::builder()
    ///     .timeout(std::time::Duration::from_secs(60))
    ///     .build()?;
    ///
    /// let client = TempoClient::new_from_reqwest(
    ///     "https://rpc.moderato.tempo.xyz",
    ///     "0x...",
    ///     custom_client,
    ///     None,
    ///     None,
    ///     None,
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new_from_reqwest(
        rpc_url: &str,
        private_key: &str,
        reqwest_client: Client,
        proxy_config: Option<ProxyConfig>,
        proxy_index: Option<usize>,
        nonce_manager: Option<Arc<crate::NonceManager>>,
        robust_nonce_manager: Option<Arc<crate::RobustNonceManager>>,
    ) -> Result<Self> {
        let signer: PrivateKeySigner =
            private_key.parse().context("Failed to parse private key")?;

        let chain_id = signer.chain_id().unwrap_or(42431);

        // Create a resilient RPC client with retry logic
        let http_transport = Http::with_client(
            reqwest_client,
            rpc_url.parse::<Url>().context("Invalid RPC URL")?,
        );

        let client = ClientBuilder::default()
            .layer(alloy::transports::layers::RetryBackoffLayer::new(
                5, 100, 2000,
            ))
            .transport(http_transport, true);

        let provider: Arc<dyn Provider + Send + Sync> = Arc::new(
            alloy::providers::ProviderBuilder::new()
                .wallet(signer.clone())
                .connect_client(client),
        );

        // Phase 1: Warm up the connection by sending HTTP HEAD request
        // This establishes TCP/TLS connection before first real use, preventing race conditions
        let warmup_client = reqwest::Client::new();
        match warmup_client
            .head(rpc_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(_) => tracing::debug!(
                "Connection warmed up successfully for proxy {:?}",
                proxy_index
            ),
            Err(e) => tracing::warn!("Connection warmup failed (will retry on first use): {}", e),
        }

        let client = Self {
            provider,
            signer,
            chain_id,
            proxy_config,
            proxy_index,
            nonce_manager,
            robust_nonce_manager,
        };

        // Phase 3: Verify provider is ready before returning
        // This ensures the connection is fully established and can reach the RPC endpoint
        client.verify_provider_ready().await?;

        Ok(client)
    }

    /// Creates a new client with optional proxy support
    ///
    /// This is the primary constructor for creating a TempoClient. It handles
    /// all HTTP client configuration including proxy setup, timeouts, and
    /// connection pooling automatically.
    ///
    /// # Arguments
    ///
    /// * `rpc_url` - The RPC endpoint URL (e.g., "https://rpc.moderato.tempo.xyz")
    /// * `private_key` - Hex-encoded private key for signing transactions
    /// * `proxy` - Optional proxy configuration for routing requests
    /// * `proxy_index` - Optional index for identifying which proxy is in use
    ///
    /// # Returns
    ///
    /// Returns `Result<Self>` which is Ok if the client was created successfully.
    /// Errors can occur from:
    /// - Invalid private key format
    /// - Invalid RPC URL
    /// - Proxy configuration issues
    /// - HTTP client build failures
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tempo_spammer::TempoClient;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// // Without proxy
    /// let client = TempoClient::new(
    ///     "https://rpc.moderato.tempo.xyz",
    ///     "0x...",
    ///     None,
    ///     None,
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(
        rpc_url: &str,
        private_key: &str,
        proxy: Option<&ProxyConfig>,
        proxy_index: Option<usize>,
    ) -> Result<Self> {
        let signer: PrivateKeySigner =
            private_key.parse().context("Failed to parse private key")?;

        let chain_id = signer.chain_id().unwrap_or(42431);

        // Build reqwest client with proxy
        let mut client_builder = Client::builder();

        if let Some(proxy_config) = proxy {
            let proxy_url = &proxy_config.url;
            let proxy = Proxy::all(proxy_url).context("Failed to create proxy")?;

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
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .pool_idle_timeout(std::time::Duration::from_secs(30)) // Reuse connections
            .pool_max_idle_per_host(5) // Limit cached connections
            .build()
            .context("Failed to build reqwest client")?;

        // Create a resilient RPC client with retry logic
        let http_transport = Http::with_client(
            reqwest_client,
            rpc_url.parse::<Url>().context("Invalid RPC URL")?,
        );

        let client = ClientBuilder::default()
            .layer(alloy::transports::layers::RetryBackoffLayer::new(
                5, 100, 2000,
            ))
            .transport(http_transport, true);

        let provider: Arc<dyn Provider + Send + Sync> = Arc::new(
            alloy::providers::ProviderBuilder::new()
                .wallet(signer.clone())
                .connect_client(client),
        );

        // Phase 1: Warm up the connection by sending HTTP HEAD request
        // This establishes TCP/TLS connection before first real use, preventing race conditions
        let warmup_client = reqwest::Client::new();
        match warmup_client
            .head(rpc_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(_) => tracing::debug!(
                "Connection warmed up successfully for proxy {:?}",
                proxy_index
            ),
            Err(e) => tracing::warn!("Connection warmup failed (will retry on first use): {}", e),
        }

        let client = Self {
            provider,
            signer,
            chain_id,
            proxy_config: proxy.cloned(),
            proxy_index,
            nonce_manager: None,
            robust_nonce_manager: None,
        };

        // Phase 3: Verify provider is ready before returning
        // This ensures the connection is fully established and can reach the RPC endpoint
        client.verify_provider_ready().await?;

        Ok(client)
    }

    /// Returns the wallet address
    ///
    /// This is a convenience method that extracts the address from the signer.
    /// The address is derived from the private key provided during construction.
    ///
    /// # Returns
    ///
    /// The Ethereum address ([`Address`]) associated with this client's wallet.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tempo_spammer::TempoClient;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let client = TempoClient::new(
    ///     "https://rpc.moderato.tempo.xyz",
    ///     "0x...",
    ///     None,
    ///     None,
    /// ).await?;
    ///
    /// let address = client.address();
    /// println!("Wallet address: {:?}", address);
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn address(&self) -> Address {
        self.signer.address()
    }

    /// Returns the chain ID
    ///
    /// Returns the chain identifier for the network this client is connected to.
    /// Defaults to 42431 (Tempo testnet) if not specified in the private key.
    ///
    /// # Returns
    ///
    /// The chain ID as a `u64`.
    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Returns a reference to the underlying provider
    ///
    /// Provides access to the Alloy provider for advanced use cases that require
    /// direct provider access not covered by the high-level client methods.
    ///
    /// # Returns
    ///
    /// A reference to the provider trait object.
    #[inline]
    pub fn provider(&self) -> &(dyn Provider + Send + Sync) {
        &*self.provider
    }

    /// Phase 3: Verify provider is ready by making a simple RPC call
    ///
    /// This ensures the connection is fully established before returning the client,
    /// preventing race conditions in release mode where the first request arrives
    /// before the TCP/TLS handshake is complete.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Provider is ready and can reach the RPC endpoint
    /// * `Err` - Provider connection failed
    async fn verify_provider_ready(&self) -> Result<()> {
        // Simple eth_chainId call to verify connection works
        match self.provider.get_chain_id().await {
            Ok(chain_id) => {
                tracing::debug!("Provider ready - chain ID verified: {}", chain_id);
                Ok(())
            }
            Err(e) => {
                tracing::warn!("Provider not ready: {}", e);
                Err(anyhow::anyhow!("Provider connection failed: {}", e))
            }
        }
    }
}

impl std::fmt::Debug for TempoClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TempoClient")
            .field("proxy_index", &self.proxy_index)
            .finish_non_exhaustive()
    }
}

impl TempoClient {
    /// Resets the nonce cache for this wallet
    ///
    /// Clears the cached nonce value, forcing the next nonce request to fetch
    /// from the RPC. This is useful when recovering from "nonce too low" errors
    /// or when transactions are submitted outside of this client instance.
    ///
    /// # Note
    ///
    /// This method only has an effect if a [`NonceManager`] was provided during
    /// client construction. If no nonce manager is configured, this is a no-op.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tempo_spammer::TempoClient;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let client = TempoClient::new(
    ///     "https://rpc.moderato.tempo.xyz",
    ///     "0x...",
    ///     None,
    ///     None,
    /// ).await?;
    ///
    /// // After a "nonce too low" error, reset the cache
    /// client.reset_nonce_cache().await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn reset_nonce_cache(&self) {
        let address = self.signer.address();

        if let Some(manager) = &self.nonce_manager {
            manager.reset(address).await;
        }

        if let Some(robust_manager) = &self.robust_nonce_manager {
            robust_manager.reset(address).await;
        }
    }

    /// Gets the next pending nonce for this wallet
    ///
    /// Returns the next nonce to use for a transaction. This method implements
    /// a two-tier caching strategy:
    ///
    /// 1. **Local Cache**: If a [`NonceManager`] is configured, attempts to get
    ///    the nonce from the local cache and auto-increment
    /// 2. **RPC Fallback**: If cache miss or no manager, fetches from RPC via
    ///    `eth_getTransactionCount`
    ///
    /// # Arguments
    ///
    /// * `rpc_url` - The RPC endpoint URL for fallback requests
    ///
    /// # Returns
    ///
    /// Returns `Result<u64>` containing the next nonce to use for a transaction.
    ///
    /// # Errors
    ///
    /// Can fail if:
    /// - RPC request fails
    /// - Proxy configuration is invalid
    /// - Network timeout occurs
    pub async fn get_pending_nonce(&self, rpc_url: &str) -> Result<u64> {
        let address = self.signer.address();

        // 1. Try Local Manager
        if let Some(manager) = &self.nonce_manager {
            if let Some(next_nonce) = manager.get_and_increment(address).await {
                return Ok(next_nonce);
            }
        }

        // 2. Fallback to RPC using existing provider (avoids creating new HTTP clients)
        // This prevents connection pool exhaustion when many tasks run concurrently
        let rpc_nonce = self
            .provider
            .get_transaction_count(address)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get transaction count: {}", e))?;

        // 3. Update Manager with NEXT expected nonce
        if let Some(manager) = &self.nonce_manager {
            manager.set(address, rpc_nonce + 1).await;
        }

        Ok(rpc_nonce)
    }

    /// Gets a nonce using the robust nonce manager with reservation pattern
    ///
    /// This is the recommended method for high-throughput scenarios. It provides:
    /// - Per-request tracking
    /// - Automatic recovery from nonce races
    /// - Reuse of failed nonces
    ///
    /// # Arguments
    /// * `rpc_url` - RPC endpoint for fallback initialization
    ///
    /// # Returns
    /// * `Ok(NonceReservation)` - Reserved nonce ready to use
    /// * `Err` - Failed to get nonce
    ///
    /// # Example
    /// ```rust,no_run
    /// # async fn example(client: &TempoClient) -> anyhow::Result<()> {
    /// let reservation = client.get_robust_nonce("https://rpc.example.com").await?;
    /// let nonce = reservation.nonce;
    /// // ... use nonce in transaction ...
    /// reservation.mark_submitted().await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_robust_nonce(
        &self,
        rpc_url: &str,
    ) -> Result<crate::robust_nonce_manager::NonceReservation> {
        let address = self.signer.address();

        if let Some(manager) = &self.robust_nonce_manager {
            // Try to reserve from robust manager
            if let Some(reservation) = manager.reserve_nonce(address).await {
                return Ok(reservation);
            }

            // Need to initialize from RPC
            let rpc_nonce = self.fetch_nonce_from_rpc(rpc_url).await?;
            manager.initialize(address, rpc_nonce).await;

            // Try again
            if let Some(reservation) = manager.reserve_nonce(address).await {
                return Ok(reservation);
            }

            anyhow::bail!("Failed to reserve nonce after initialization");
        } else {
            anyhow::bail!("Robust nonce manager not configured");
        }
    }

    /// Handles a "nonce too low" error with automatic recovery
    ///
    /// Call this when a transaction fails with "nonce too low" to:
    /// 1. Mark the failed nonce
    /// 2. Sync with blockchain
    /// 3. Adjust cached nonce for future requests
    ///
    /// # Arguments
    /// * `attempted_nonce` - The nonce that was rejected
    /// * `actual_next_nonce` - The actual next nonce from the error
    pub async fn handle_robust_nonce_error(&self, attempted_nonce: u64, actual_next_nonce: u64) {
        let address = self.signer.address();

        if let Some(manager) = &self.robust_nonce_manager {
            manager
                .handle_nonce_error(address, attempted_nonce, actual_next_nonce)
                .await;
        }
    }

    /// Confirms a nonce was successfully mined
    ///
    /// Call this when a transaction is confirmed on-chain to update statistics
    /// and allow cleanup of old confirmed nonces.
    ///
    /// # Arguments
    /// * `nonce` - The confirmed nonce
    pub async fn confirm_robust_nonce(&self, nonce: u64) {
        let address = self.signer.address();

        if let Some(manager) = &self.robust_nonce_manager {
            manager.confirm_nonce(address, nonce).await;
        }
    }

    /// Gets statistics from the robust nonce manager
    ///
    /// Returns detailed statistics about nonce state for monitoring.
    pub async fn get_robust_nonce_stats(&self) -> Option<crate::robust_nonce_manager::NonceStats> {
        let address = self.signer.address();

        if let Some(manager) = &self.robust_nonce_manager {
            manager.get_stats(address).await
        } else {
            None
        }
    }

    /// Resets the robust nonce manager state for this wallet
    ///
    /// Forces a full resync with the blockchain on next nonce request.
    pub async fn reset_robust_nonce(&self) {
        let address = self.signer.address();

        if let Some(manager) = &self.robust_nonce_manager {
            manager.reset(address).await;
        }
    }

    /// Helper: Fetch nonce from RPC using existing provider
    ///
    /// Uses the client's existing provider instead of creating new HTTP connections,
    /// preventing connection pool exhaustion under high concurrency.
    async fn fetch_nonce_from_rpc(&self, _rpc_url: &str) -> Result<u64> {
        let address = self.signer.address();

        // Use existing provider to avoid creating new HTTP clients
        let rpc_nonce = self
            .provider
            .get_transaction_count(address)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get transaction count: {}", e))?;

        Ok(rpc_nonce)
    }
}
