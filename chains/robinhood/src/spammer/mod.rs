use anyhow::Result;
use async_trait::async_trait;
use core_logic::config::SpamConfig;
use core_logic::traits::Spammer;
use ethers::prelude::*;
use tokio::time::{sleep, Duration};
use tracing::info;
// use std::sync::Arc;
// use std::str::FromStr;
use reqwest::Client;

pub struct EvmSpammer {
    config: SpamConfig,
    #[allow(dead_code)]
    provider: Provider<Http>,
    wallet: LocalWallet,
}

impl EvmSpammer {
    pub fn new_with_signer(
        config: SpamConfig,
        signer: LocalWallet,
        proxy_config: Option<core_logic::config::ProxyConfig>,
    ) -> Result<Self> {
        // Build reqwest client with proxy if needed
        let mut client_builder = Client::builder();

        if let Some(proxy_conf) = proxy_config {
            let mut proxy = reqwest::Proxy::all(&proxy_conf.url)?;
            if let (Some(u), Some(p)) = (&proxy_conf.username, &proxy_conf.password) {
                proxy = proxy.basic_auth(u, p);
            }
            client_builder = client_builder.proxy(proxy);
        }

        let client = client_builder.build()?;

        let provider = Provider::new(Http::new_with_client(
            reqwest::Url::parse(&config.rpc_url)?,
            client,
        ));

        Ok(Self {
            provider,
            wallet: signer.with_chain_id(config.chain_id),
            config,
        })
    }
}

use tokio_util::sync::CancellationToken;

// ...

#[async_trait]
impl Spammer for EvmSpammer {
    async fn new(_config: SpamConfig) -> Result<Self> {
        Err(anyhow::anyhow!("Use new_with_signer construction"))
    }

    async fn start(
        &self,
        cancellation_token: CancellationToken,
    ) -> Result<core_logic::traits::SpammerStats> {
        info!("EVM Spammer started for chain {}", self.config.chain_id);

        let mut stats = core_logic::traits::SpammerStats::default();

        loop {
            if cancellation_token.is_cancelled() {
                info!("EVM Spammer stopping (cancelled).");
                break;
            }

            // Mock spam loop using ethers
            let _tx = TransactionRequest::new()
                .to(self.wallet.address()) // Self-spam
                .value(0)
                .chain_id(self.config.chain_id);

            // In real impl, use self.wallet.sign_transaction(&tx) and self.provider.send_raw_transaction...
            info!(
                "Sending EVM Tx to {} on chain {}",
                self.wallet.address(),
                self.config.chain_id
            );
            stats.success += 1; // Mock success

            // Rate limit
            let sleep_ms = 1000 / self.config.target_tps.max(1) as u64;

            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("EVM Spammer stopping (cancelled during sleep).");
                    break;
                }
                _ = sleep(Duration::from_millis(sleep_ms)) => {}
            }
        }
        Ok(stats)
    }

    async fn stop(&self) -> Result<()> {
        info!("EVM Spammer stopping...");
        Ok(())
    }
}
