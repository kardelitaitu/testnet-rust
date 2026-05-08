use crate::security::SecurityUtils;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Supported blockchain types for polymorphic wallet management
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Zeroize)]
pub enum ChainType {
    Evm,
    Solana,
    Sui,
    Aptos,
    Tron,
    Ton,
}

impl Default for ChainType {
    fn default() -> Self {
        Self::Evm
    }
}

impl fmt::Display for ChainType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChainType::Evm => write!(f, "EVM"),
            ChainType::Solana => write!(f, "Solana"),
            ChainType::Sui => write!(f, "SUI"),
            ChainType::Aptos => write!(f, "Aptos"),
            ChainType::Tron => write!(f, "Tron"),
            ChainType::Ton => write!(f, "TON"),
        }
    }
}

/// Polymorphic wallet data that only holds requested chain keys to save RAM
#[derive(Clone, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct DecryptedWallet {
    #[serde(default)]
    pub mnemonic: String,
    
    // Targeted fields - only populated for the requested chain
    #[serde(default)]
    pub evm_private_key: String,
    #[serde(default)]
    pub evm_address: String,
    
    #[serde(default)]
    pub sol_private_key: String,
    #[serde(default)]
    pub sol_address: String,
    
    #[serde(default)]
    pub sui_private_key: String,
    #[serde(default)]
    pub sui_address: String,
    
    #[serde(default)]
    pub tron_private_key: String,
    #[serde(default)]
    pub tron_address: String,
    
    #[serde(default)]
    pub aptos_private_key: String,
    #[serde(default)]
    pub aptos_address: String,
    
    #[serde(default)]
    pub ton_private_key: String,
    #[serde(default)]
    pub ton_address: String,

    /// Metadata indicating which chain this wallet was optimized for
    #[serde(skip)]
    pub active_chain: ChainType,
}

impl DecryptedWallet {
    /// Returns the private key for the active chain
    pub fn private_key(&self) -> &str {
        match self.active_chain {
            ChainType::Evm => &self.evm_private_key,
            ChainType::Solana => &self.sol_private_key,
            ChainType::Sui => &self.sui_private_key,
            ChainType::Aptos => &self.aptos_private_key,
            ChainType::Tron => &self.tron_private_key,
            ChainType::Ton => &self.ton_private_key,
        }
    }

    /// Returns the address for the active chain
    pub fn address(&self) -> &str {
        match self.active_chain {
            ChainType::Evm => &self.evm_address,
            ChainType::Solana => &self.sol_address,
            ChainType::Sui => &self.sui_address,
            ChainType::Aptos => &self.aptos_address,
            ChainType::Tron => &self.tron_address,
            ChainType::Ton => &self.ton_address,
        }
    }
}

impl fmt::Debug for DecryptedWallet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecryptedWallet")
            .field("active_chain", &self.active_chain)
            .field("address", &self.address())
            .field("mnemonic", &"***REDACTED***")
            .field("private_key", &"***REDACTED***")
            .finish()
    }
}

#[derive(Debug)]
enum WalletSource {
    JsonFile(PathBuf),
    RawKey(String),
}

pub struct WalletManager {
    sources: Vec<WalletSource>,
    /// Cache is now segmented by (index, chain_type) to support multi-chain loading efficiently
    cache: Mutex<HashMap<(usize, ChainType), Arc<DecryptedWallet>>>,
}

impl WalletManager {
    const WALLETS_DIR: &'static str = "wallet-json";
    const PV_FILE: &'static str = "pv.txt";

    pub fn new() -> Result<Self> {
        // Try current dir first, then workspace root (../../)
        let candidates = vec![
            PathBuf::from(Self::WALLETS_DIR),
            PathBuf::from("../..").join(Self::WALLETS_DIR),
        ];

        let mut sources = Vec::new();

        for wallets_path in candidates {
            if wallets_path.exists() && wallets_path.is_dir() {
                println!("[WalletManager] Scanning wallets in {:?}", wallets_path);
                let mut entries: Vec<PathBuf> = fs::read_dir(&wallets_path)?
                    .filter_map(|res| res.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
                    .collect();

                entries.sort();
                println!(
                    "[WalletManager] Found {} wallet files in {:?}",
                    entries.len(),
                    wallets_path
                );

                for entry in entries {
                    sources.push(WalletSource::JsonFile(entry));
                }

                if !sources.is_empty() {
                    break;
                }
            }
        }

        // Fallback to pv.txt
        if sources.is_empty() {
            let pv_path = Path::new(Self::PV_FILE);
            if pv_path.exists() {
                let content = fs::read_to_string(pv_path)?;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        sources.push(WalletSource::RawKey(trimmed.to_string()));
                    }
                }
            }
        }

        Ok(Self {
            sources,
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// Create a new WalletManager that scans a specific directory for wallet files.
    /// This is useful for chains that require a dedicated wallet folder.
    pub fn with_wallet_dir<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let dir_path = dir.as_ref().to_path_buf();
        let mut sources = Vec::new();

        if dir_path.exists() && dir_path.is_dir() {
            println!("[WalletManager] Scanning wallets in {:?}", dir_path);
            let mut entries: Vec<PathBuf> = fs::read_dir(&dir_path)?
                .filter_map(|res| res.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
                .collect();

            entries.sort();
            println!(
                "[WalletManager] Found {} wallet files in {:?}",
                entries.len(),
                dir_path
            );

            for entry in entries {
                sources.push(WalletSource::JsonFile(entry));
            }
        } else {
            println!(
                "[WalletManager] Directory {:?} not found or not a directory. No wallets loaded.",
                dir_path
            );
        }

        Ok(Self {
            sources,
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// Returns the number of available wallets
    pub fn count(&self) -> usize {
        self.sources.len()
    }

    /// List wallet identifiers (filenames or indices) without decrypting
    pub fn list_wallets(&self) -> Vec<String> {
        self.sources
            .iter()
            .enumerate()
            .map(|(i, src)| match src {
                WalletSource::JsonFile(path) => path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown.json")
                    .to_string(),
                WalletSource::RawKey(_) => format!("Wallet {}", i),
            })
            .collect()
    }

    /// Clears the decrypted wallet cache to free memory
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.lock().await;
        let count = cache.len();
        cache.clear();
        tracing::debug!("Cleared WalletManager cache ({} wallets)", count);
    }

    /// Get a decrypted wallet by index for a specific chain.
    /// Targeted decryption saves RAM by only loading requested keys.
    pub async fn get_wallet(
        &self,
        index: usize,
        password: Option<&str>,
    ) -> Result<Arc<DecryptedWallet>> {
        // Default to EVM for backward compatibility
        self.get_wallet_for_chain(index, password, ChainType::Evm).await
    }

    /// Get a decrypted wallet specifically for a targeted chain
    pub async fn get_wallet_for_chain(
        &self,
        index: usize,
        password: Option<&str>,
        chain: ChainType,
    ) -> Result<Arc<DecryptedWallet>> {
        // Check cache first
        {
            let cache = self.cache.lock().await;
            if let Some(wallet) = cache.get(&(index, chain)) {
                return Ok(Arc::clone(wallet));
            }
        }

        // Not in cache, decrypt
        let source = self.sources.get(index).context(format!(
            "Wallet index {} out of bounds (total: {})",
            index,
            self.sources.len()
        ))?;
        
        let wallet = match source {
            WalletSource::JsonFile(path) => Arc::new(Self::decrypt_json_wallet_targeted(path, password, chain)?),
            WalletSource::RawKey(key) => {
                // Raw keys are assumed to be EVM hex strings for now
                let mut w = DecryptedWallet {
                    mnemonic: "".to_string(),
                    evm_private_key: key.clone(),
                    evm_address: "".to_string(),
                    sol_private_key: "".to_string(),
                    sol_address: "".to_string(),
                    sui_private_key: "".to_string(),
                    sui_address: "".to_string(),
                    tron_private_key: "".to_string(),
                    tron_address: "".to_string(),
                    aptos_private_key: "".to_string(),
                    aptos_address: "".to_string(),
                    ton_private_key: "".to_string(),
                    ton_address: "".to_string(),
                    active_chain: ChainType::Evm,
                };
                
                // If requested non-EVM for raw key, it might not work as expected, 
                // but we populate the target field just in case it's a multi-format key
                match chain {
                    ChainType::Solana => w.sol_private_key = key.clone(),
                    ChainType::Sui => w.sui_private_key = key.clone(),
                    _ => {}
                }
                w.active_chain = chain;
                Arc::new(w)
            },
        };

        // Store in cache
        {
            let mut cache = self.cache.lock().await;
            cache.insert((index, chain), Arc::clone(&wallet));
        }

        Ok(wallet)
    }

    /// Internal: Targeted decryption that only extracts the requested chain keys
    fn decrypt_json_wallet_targeted(path: &Path, password: Option<&str>, chain: ChainType) -> Result<DecryptedWallet> {
        let content = fs::read_to_string(path)?;
        let json: Value = serde_json::from_str(&content)?;

        if let Some(encrypted_val) = json.get("encrypted") {
            if encrypted_val.is_object() {
                let pass = password.context("Password required for encrypted wallet")?;

                let ciphertext_hex = encrypted_val.get("ciphertext").and_then(|v| v.as_str()).unwrap_or("");
                let iv_hex = encrypted_val.get("iv").and_then(|v| v.as_str()).unwrap_or("");
                let salt_hex = encrypted_val.get("salt").and_then(|v| v.as_str()).unwrap_or("");
                let tag_hex = encrypted_val.get("tag").and_then(|v| v.as_str()).unwrap_or("");

                if !ciphertext_hex.is_empty() {
                    let decrypted = SecurityUtils::decrypt_components(
                        ciphertext_hex, iv_hex, salt_hex, tag_hex, pass,
                    )?;
                    
                    // Targeted Deserialization
                    // Parse full JSON to Value first, then pick only what we need
                    let full_data: Value = serde_json::from_str(&decrypted)?;
                    
                    let mut wallet = DecryptedWallet::default();
                    wallet.mnemonic = full_data.get("mnemonic").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    wallet.active_chain = chain;

                    match chain {
                        ChainType::Evm => {
                            wallet.evm_private_key = full_data.get("evm_private_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            wallet.evm_address = full_data.get("evm_address").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        },
                        ChainType::Solana => {
                            wallet.sol_private_key = full_data.get("sol_private_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            wallet.sol_address = full_data.get("sol_address").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        },
                        ChainType::Sui => {
                            wallet.sui_private_key = full_data.get("sui_private_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            wallet.sui_address = full_data.get("sui_address").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        },
                        ChainType::Aptos => {
                            wallet.aptos_private_key = full_data.get("aptos_private_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            wallet.aptos_address = full_data.get("aptos_address").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        },
                        ChainType::Tron => {
                            wallet.tron_private_key = full_data.get("tron_private_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            wallet.tron_address = full_data.get("tron_address").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        },
                        ChainType::Ton => {
                            wallet.ton_private_key = full_data.get("ton_private_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            wallet.ton_address = full_data.get("ton_address").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        },
                    }
                    
                    return Ok(wallet);
                }
            }
        }

        Err(anyhow!("Invalid or unrecognized wallet format in {:?}", path))
    }

    // Legacy support
    pub async fn get_private_keys(password: Option<String>) -> Result<Vec<String>> {
        let manager = Self::new()?;
        let mut keys = Vec::new();
        for i in 0..manager.count() {
            let w = manager.get_wallet(i, password.as_deref()).await?;
            keys.push(w.evm_private_key.clone());
        }
        Ok(keys)
    }
}

impl Default for DecryptedWallet {
    fn default() -> Self {
        Self {
            mnemonic: "".to_string(),
            evm_private_key: "".to_string(),
            evm_address: "".to_string(),
            sol_private_key: "".to_string(),
            sol_address: "".to_string(),
            sui_private_key: "".to_string(),
            sui_address: "".to_string(),
            tron_private_key: "".to_string(),
            tron_address: "".to_string(),
            aptos_private_key: "".to_string(),
            aptos_address: "".to_string(),
            ton_private_key: "".to_string(),
            ton_address: "".to_string(),
            active_chain: ChainType::Evm,
        }
    }
}
