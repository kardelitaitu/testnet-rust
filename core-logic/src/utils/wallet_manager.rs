use crate::security::SecurityUtils;
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct DecryptedWallet {
    #[serde(default)]
    pub mnemonic: String,
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
}

impl fmt::Debug for DecryptedWallet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecryptedWallet")
            .field("evm_address", &self.evm_address)
            .field("sol_address", &self.sol_address)
            .field("sui_address", &self.sui_address)
            .field("tron_address", &self.tron_address)
            .field("aptos_address", &self.aptos_address)
            .field("ton_address", &self.ton_address)
            .field("mnemonic", &"***REDACTED***")
            .field("evm_private_key", &"***REDACTED***")
            .field("sol_private_key", &"***REDACTED***")
            .field("sui_private_key", &"***REDACTED***")
            .field("tron_private_key", &"***REDACTED***")
            .field("aptos_private_key", &"***REDACTED***")
            .field("ton_private_key", &"***REDACTED***")
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
    cache: Mutex<HashMap<usize, Arc<DecryptedWallet>>>,
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

                // If we found wallets in one location, stop searching to avoid duplicates or confusion
                if !sources.is_empty() {
                    break;
                }
            }
        }

        // Fallback to pv.txt if no JSON wallets found
        if sources.is_empty() {
            println!(
                "[WalletManager] No wallets found in wallet-json/, checking for pv.txt fallback"
            );
            let pv_path = Path::new(Self::PV_FILE);
            if pv_path.exists() {
                println!("[WalletManager] Loading raw keys from {}", Self::PV_FILE);
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

    /// Get a decrypted wallet by index. Decrypts if not cached.
    /// Returns Arc<DecryptedWallet> to avoid cloning sensitive data.
    pub async fn get_wallet(
        &self,
        index: usize,
        password: Option<&str>,
    ) -> Result<Arc<DecryptedWallet>> {
        // Check cache first
        {
            let cache = self.cache.lock().await;
            if let Some(wallet) = cache.get(&index) {
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
            WalletSource::JsonFile(path) => Arc::new(Self::decrypt_json_wallet(path, password)?),
            WalletSource::RawKey(key) => Arc::new(DecryptedWallet {
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
            }),
        };

        // Store in cache
        {
            let mut cache = self.cache.lock().await;
            cache.insert(index, Arc::clone(&wallet));
        }

        Ok(wallet)
    }

    // Helper for legacy support, loads ALL private keys
    pub async fn get_private_keys(password: Option<String>) -> Result<Vec<String>> {
        let manager = Self::new()?;
        let mut keys = Vec::new();
        for i in 0..manager.count() {
            let w = manager.get_wallet(i, password.as_deref()).await?;
            keys.push(w.evm_private_key.clone());
        }
        Ok(keys)
    }

    fn decrypt_json_wallet(path: &Path, password: Option<&str>) -> Result<DecryptedWallet> {
        let content = fs::read_to_string(path)?;
        let json: Value = serde_json::from_str(&content)?;

        if let Some(encrypted_val) = json.get("encrypted") {
            if encrypted_val.is_object() {
                let pass = password.context("Password required for encrypted wallet")?;

                let encrypted_block = encrypted_val;
                let ciphertext_hex = encrypted_block
                    .get("ciphertext")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let iv_hex = encrypted_block
                    .get("iv")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let salt_hex = encrypted_block
                    .get("salt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let tag_hex = encrypted_block
                    .get("tag")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if !ciphertext_hex.is_empty() {
                    let decrypted = SecurityUtils::decrypt_components(
                        ciphertext_hex,
                        iv_hex,
                        salt_hex,
                        tag_hex,
                        pass,
                    )?;
                    let wallet: DecryptedWallet = serde_json::from_str(&decrypted)?;
                    return Ok(wallet);
                }
            }
        }

        // Could handle unencrypted format if needed, but assuming encrypted for now based on previous code
        Err(anyhow!(
            "Invalid or unrecognized wallet format in {:?}",
            path
        ))
    }
}
