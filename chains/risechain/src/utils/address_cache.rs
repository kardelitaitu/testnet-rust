//! Cached address loader for recipient addresses.
//!
//! Loads addresses from the root `address.txt` file once at startup
//! and provides thread-safe access for all tasks.

use anyhow::{Context, Result};
use ethers::types::Address;
use once_cell::sync::OnceCell;
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use std::fs;
use std::path::Path;
use tracing::info;

static ADDRESS_CACHE: OnceCell<AddressCache> = OnceCell::new();

#[derive(Debug)]
pub struct AddressCache {
    addresses: Vec<Address>,
}

impl AddressCache {
    pub fn init() -> Result<()> {
        let paths = ["address.txt", "chains/risechain/address.txt"];

        for path in &paths {
            if Path::new(path).exists() {
                ADDRESS_CACHE.get_or_try_init(|| Self::load_from_file(path))?;
                return Ok(());
            }
        }

        anyhow::bail!("address.txt not found. Please create it in the root directory.")
    }

    pub fn init_from_path(path: &str) -> Result<()> {
        if !Path::new(path).exists() {
            anyhow::bail!("address.txt not found at: {}", path);
        }
        ADDRESS_CACHE.get_or_try_init(|| Self::load_from_file(path))?;
        Ok(())
    }

    fn load_from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read address file: {}", path))?;

        let addresses: Vec<Address> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .enumerate()
            .filter_map(|(i, line)| {
                let trimmed = line.trim();
                match trimmed.parse::<Address>() {
                    Ok(addr) => Some(addr),
                    Err(e) => {
                        tracing::warn!(
                            "Invalid address at line {} in {}: '{}' - {}",
                            i + 1,
                            path,
                            trimmed,
                            e
                        );
                        None
                    }
                }
            })
            .collect();

        if addresses.is_empty() {
            anyhow::bail!("No valid addresses found in {}", path);
        }

        info!(
            "Loaded {} addresses from {} into cache",
            addresses.len(),
            path
        );

        Ok(Self { addresses })
    }

    pub fn global() -> Result<&'static AddressCache> {
        ADDRESS_CACHE
            .get()
            .context("Address cache not initialized. Call AddressCache::init() first.")
    }

    pub fn get_random() -> Result<Address> {
        Self::global()?
            .addresses
            .choose(&mut OsRng)
            .copied()
            .context("Address cache is empty")
    }

    pub fn get_random_many(count: usize) -> Result<Vec<Address>> {
        let cache = Self::global()?;
        if cache.addresses.is_empty() {
            anyhow::bail!("Address cache is empty");
        }

        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(*cache.addresses.choose(&mut OsRng).unwrap());
        }
        Ok(result)
    }

    pub fn all() -> Result<Vec<Address>> {
        Ok(Self::global()?.addresses.clone())
    }

    pub fn len() -> usize {
        Self::global().map(|c| c.addresses.len()).unwrap_or(0)
    }

    pub fn is_empty() -> bool {
        Self::global()
            .map(|c| c.addresses.is_empty())
            .unwrap_or(true)
    }

    pub fn addresses(&self) -> &[Address] {
        &self.addresses
    }

    pub fn len_instance(&self) -> usize {
        self.addresses.len()
    }

    pub fn is_empty_instance(&self) -> bool {
        self.addresses.is_empty()
    }
}
