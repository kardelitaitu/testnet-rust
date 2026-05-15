use anyhow::Result;
use core_logic::WalletManager;
use ethers::prelude::*;
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use crate::config::EvmConfig;
use crate::utils::load_proxies;
use core_logic::config::ProxyConfig;

pub struct RobinhoodClient {
    pub provider: Arc<Provider<Http>>,
    pub wallet: LocalWallet,
    pub proxy_url: Option<String>,
}

pub struct ClientPool {
    wallet_manager: Arc<WalletManager>,
    wallet_password: Option<String>,
    config: EvmConfig,
    proxies: Vec<ProxyConfig>,
    clients: RwLock<HashMap<usize, RobinhoodClient>>,
    http_clients: RwLock<HashMap<Option<String>, (reqwest::Client, Instant)>>,
    locked_wallets: Mutex<HashSet<usize>>,
    proxy_rotation_counter: AtomicUsize,
}

pub struct ClientLease {
    pub client: RobinhoodClient,
    pub index: usize,
    pool: Arc<ClientPool>,
}

impl Drop for ClientLease {
    fn drop(&mut self) {
        self.pool.release_wallet(self.index);
    }
}

impl ClientPool {
    pub fn new(config: EvmConfig, wallet_manager: Arc<WalletManager>, wallet_password: Option<String>) -> Result<Self> {
        let proxies = load_proxies("proxies.txt").unwrap_or_default();
        Ok(Self {
            wallet_manager,
            wallet_password,
            config,
            proxies,
            clients: RwLock::new(HashMap::new()),
            http_clients: RwLock::new(HashMap::new()),
            locked_wallets: Mutex::new(HashSet::new()),
            proxy_rotation_counter: AtomicUsize::new(0),
        })
    }

    pub async fn try_acquire_client(self: &Arc<Self>) -> Option<ClientLease> {
        let total_wallets = self.wallet_manager.count();
        if total_wallets == 0 { return None; }

        let selected_idx = {
            let mut locked = self.locked_wallets.lock();
            let available: Vec<usize> = (0..total_wallets).filter(|i| !locked.contains(i)).collect();
            if available.is_empty() { return None; }

            let idx = available[fastrand::usize(0..available.len())];
            locked.insert(idx);
            idx
        }; // Lock dropped here before .await

        match self.get_or_create_client(selected_idx).await {
            Ok(client) => Some(ClientLease { client, index: selected_idx, pool: self.clone() }),
            Err(_) => {
                self.release_wallet(selected_idx);
                None
            }
        }
    }

    async fn get_or_create_client(&self, wallet_idx: usize) -> Result<RobinhoodClient> {
        if let Some(client) = self.clients.read().get(&wallet_idx) {
            return Ok(RobinhoodClient {
                provider: client.provider.clone(),
                wallet: client.wallet.clone(),
                proxy_url: client.proxy_url.clone(),
            });
        }

        let wallet_data = self.wallet_manager.get_wallet(wallet_idx, self.wallet_password.as_deref()).await?;
        let wallet: LocalWallet = wallet_data.evm_private_key.parse::<LocalWallet>()?.with_chain_id(self.config.chain_id);

        let proxy_conf = if self.proxies.is_empty() { None } 
        else { Some(&self.proxies[self.proxy_rotation_counter.fetch_add(1, Ordering::SeqCst) % self.proxies.len()]) };

        let proxy_url = proxy_conf.map(|p| p.url.clone());
        let reqwest_client = self.get_or_create_http_client(proxy_url.clone()).await?;

        let provider = Arc::new(Provider::new(Http::new_with_client(
            reqwest::Url::parse(&self.config.rpc_url)?,
            reqwest_client,
        )));

        let client = RobinhoodClient { provider, wallet, proxy_url };
        self.clients.write().insert(wallet_idx, RobinhoodClient {
            provider: client.provider.clone(),
            wallet: client.wallet.clone(),
            proxy_url: client.proxy_url.clone(),
        });

        Ok(client)
    }

    async fn get_or_create_http_client(&self, proxy_url: Option<String>) -> Result<reqwest::Client> {
        if let Some((client, _)) = self.http_clients.read().get(&proxy_url) {
            return Ok(client.clone());
        }

        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .pool_idle_timeout(std::time::Duration::from_secs(30));

        if let Some(ref url) = proxy_url {
            let proxy_config = self.proxies.iter().find(|p| p.url == *url).unwrap();
            let mut proxy = reqwest::Proxy::all(url)?;
            if let (Some(u), Some(p)) = (&proxy_config.username, &proxy_config.password) {
                proxy = proxy.basic_auth(u, p);
            }
            builder = builder.proxy(proxy);
        }

        let client = builder.build()?;
        self.http_clients.write().insert(proxy_url, (client.clone(), Instant::now()));
        Ok(client)
    }

    pub fn release_wallet(&self, index: usize) {
        self.locked_wallets.lock().remove(&index);
    }

    pub fn count(&self) -> usize {
        self.wallet_manager.count()
    }
}
