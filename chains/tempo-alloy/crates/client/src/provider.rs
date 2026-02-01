//! Tempo Provider - Alloy-based provider for Tempo blockchain

use alloy::{
    network::{AnyNetwork, Network},
    prelude::*,
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    transports::Transport,
};
use anyhow::{Context, Result};
use reqwest::Client as HttpClient;
use std::sync::Arc;
use url::Url;

/// Tempo provider type - uses AnyNetwork to handle Tempo's custom transaction types
pub type TempoProvider = Provider<Http, AnyNetwork>;

/// Main client for interacting with the Tempo blockchain
#[derive(Debug, Clone)]
pub struct TempoClient {
    /// The Alloy provider with signer
    pub provider: Arc<TempoProvider>,
    /// The signer (private key)
    pub signer: PrivateKeySigner,
    /// Chain ID
    pub chain_id: u64,
}

impl TempoClient {
    /// Create a new TempoClient from a private key
    ///
    /// # Arguments
    /// * `rpc_url` - The RPC endpoint URL
    /// * `private_key` - The private key (hex string, with or without 0x prefix)
    ///
    /// # Example
    /// ```ignore
    /// let client = TempoClient::new(
    ///     "https://rpc.moderato.tempo.xyz",
    ///     "0xac1f73..."
    /// ).await?;
    /// ```
    pub async fn new(rpc_url: &str, private_key: &str) -> Result<Self> {
        let signer: PrivateKeySigner =
            private_key.parse().context("Failed to parse private key")?;

        let chain_id = signer.chain_id();

        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(signer.clone())
            .connect_http(rpc_url.parse().context("Invalid RPC URL")?);

        Ok(Self {
            provider: Arc::new(provider),
            signer,
            chain_id,
        })
    }

    /// Create a new TempoClient with proxy support
    ///
    /// # Arguments
    /// * `rpc_url` - The RPC endpoint URL
    /// * `private_key` - The private key (hex string)
    /// * `proxy_url` - Optional proxy URL
    /// * `proxy_username` - Optional proxy username
    /// * `proxy_password` - Optional proxy password
    pub async fn with_proxy(
        rpc_url: &str,
        private_key: &str,
        proxy_url: Option<&str>,
        proxy_username: Option<&str>,
        proxy_password: Option<&str>,
    ) -> Result<Self> {
        let signer: PrivateKeySigner =
            private_key.parse().context("Failed to parse private key")?;

        let chain_id = signer.chain_id();

        let provider = if let Some(proxy) = proxy_url {
            let mut proxy_builder = reqwest::Proxy::all(proxy)?;

            if let (Some(username), Some(password)) = (proxy_username, proxy_password) {
                proxy_builder = proxy_builder.basic_auth(username, password);
            }

            let client = HttpClient::builder().proxy(proxy_builder).build()?;

            let transport = alloy::transports::Http::new_with_client(
                Url::parse(rpc_url).context("Invalid RPC URL")?,
                client,
            );

            ProviderBuilder::new()
                .with_recommended_fillers()
                .wallet(signer.clone())
                .on(transport)
        } else {
            ProviderBuilder::new()
                .with_recommended_fillers()
                .wallet(signer.clone())
                .connect_http(rpc_url.parse().context("Invalid RPC URL")?)
        };

        Ok(Self {
            provider: Arc::new(provider),
            signer,
            chain_id,
        })
    }

    /// Get the sender's address
    #[inline]
    pub fn address(&self) -> Address {
        self.signer.address()
    }

    /// Get the chain ID
    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Get the provider reference
    #[inline]
    pub fn provider(&self) -> &TempoProvider {
        &self.provider
    }
}

/// Extension trait for provider operations
#[async_trait::async_trait]
pub trait TempoProviderExt {
    /// Get the next transaction nonce for the given address
    async fn get_next_nonce(&self, address: Address) -> Result<u64>;

    /// Get balance with optional block tag
    async fn get_balance(&self, address: Address, block: Option<BlockNumberOrTag>) -> Result<U256>;
}

#[async_trait::async_trait]
impl<T: Transport + Clone> TempoProviderExt for Provider<T, AnyNetwork> {
    async fn get_next_nonce(&self, address: Address) -> Result<u64> {
        let nonce = self.get_transaction_count(address).await?;
        Ok(nonce.as_u64())
    }

    async fn get_balance(&self, address: Address, block: Option<BlockNumberOrTag>) -> Result<U256> {
        let balance = self.get_balance(address, block).await?;
        Ok(balance)
    }
}
