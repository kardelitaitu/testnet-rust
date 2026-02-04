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

/// Global cached address list
static ADDRESS_CACHE: OnceCell<AddressCache> = OnceCell::new();

/// Cached address storage
#[derive(Debug)]
pub struct AddressCache {
    addresses: Vec<Address>,
}

impl AddressCache {
    /// Initialize the global address cache from the root address.txt file.
    /// This should be called once at startup.
    pub fn init() -> Result<()> {
        ADDRESS_CACHE.get_or_try_init(|| Self::load_from_file("address.txt"))?;
        Ok(())
    }

    /// Initialize from a custom path (useful for testing)
    pub fn init_from_path(path: &str) -> Result<()> {
        ADDRESS_CACHE.get_or_try_init(|| Self::load_from_file(path))?;
        Ok(())
    }

    /// Load addresses from a file
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

    /// Get the global address cache instance
    pub fn global() -> Result<&'static AddressCache> {
        ADDRESS_CACHE
            .get()
            .context("Address cache not initialized. Call AddressCache::init() first.")
    }

    /// Get a random address from the cache
    pub fn get_random(&self) -> Result<Address> {
        self.addresses
            .choose(&mut OsRng)
            .copied()
            .context("Address cache is empty")
    }

    /// Get multiple random addresses (with possible duplicates)
    pub fn get_random_many(&self, count: usize) -> Result<Vec<Address>> {
        if self.addresses.is_empty() {
            anyhow::bail!("Address cache is empty");
        }

        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(*self.addresses.choose(&mut OsRng).unwrap());
        }
        Ok(result)
    }

    /// Get all cached addresses
    pub fn all(&self) -> &[Address] {
        &self.addresses
    }

    /// Get the count of cached addresses
    pub fn len(&self) -> usize {
        self.addresses.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_addresses() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "0x1234567890123456789012345678901234567890").unwrap();
        writeln!(file, "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd").unwrap();
        writeln!(file, "").unwrap(); // Empty line should be skipped
        writeln!(file, "invalid_address").unwrap(); // Invalid should be skipped

        let cache = AddressCache::load_from_file(file.path().to_str().unwrap()).unwrap();
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_get_random() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "0x1234567890123456789012345678901234567890").unwrap();

        let cache = AddressCache::load_from_file(file.path().to_str().unwrap()).unwrap();
        let addr = cache.get_random().unwrap();
        assert_eq!(
            addr,
            "0x1234567890123456789012345678901234567890"
                .parse::<Address>()
                .unwrap()
        );
    }
}
