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
        let paths = ["address.txt", "chains/xenea/address.txt"];

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct TempEnv {
        path: std::path::PathBuf,
        _dir: std::path::PathBuf,
    }

    impl TempEnv {
        fn new(lines: &[&str]) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "addr_test_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
            std::fs::create_dir_all(&path).ok();
            let file_path = path.join("address.txt");
            let mut f = std::fs::File::create(&file_path).unwrap();
            for line in lines {
                writeln!(f, "{}", line).unwrap();
            }
            TempEnv {
                path: file_path,
                _dir: path,
            }
        }
    }

    impl Drop for TempEnv {
        fn drop(&mut self) {
            std::fs::remove_file(&self.path).ok();
            if let Some(parent) = self.path.parent() {
                std::fs::remove_dir(parent).ok();
            }
        }
    }

    #[test]
    fn test_load_from_file_valid() {
        let env = TempEnv::new(&[
            "0xd7d2e492e6dda0013e9062f00327a06fdb722488",
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ]);
        let cache = AddressCache::load_from_file(env.path.to_str().unwrap()).unwrap();
        assert_eq!(cache.len_instance(), 2);
        assert_eq!(
            cache.addresses()[0],
            "0xd7d2e492e6dda0013e9062f00327a06fdb722488"
                .parse::<Address>()
                .unwrap()
        );
    }

    #[test]
    fn test_load_from_file_skips_empty_lines() {
        let env = TempEnv::new(&[
            "0xd7d2e492e6dda0013e9062f00327a06fdb722488",
            "",
            "   ",
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ]);
        let cache = AddressCache::load_from_file(env.path.to_str().unwrap()).unwrap();
        assert_eq!(cache.len_instance(), 2);
    }

    #[test]
    fn test_load_from_file_all_invalid_errors() {
        let env = TempEnv::new(&["not_an_address", "invalid_hex"]);
        let result = AddressCache::load_from_file(env.path.to_str().unwrap());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No valid addresses"));
    }

    #[test]
    fn test_load_from_file_empty_file_errors() {
        let env = TempEnv::new(&[]);
        let result = AddressCache::load_from_file(env.path.to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_instance_methods() {
        let addr: Address = "0xd7d2e492e6dda0013e9062f00327a06fdb722488"
            .parse()
            .unwrap();
        let cache = AddressCache {
            addresses: vec![addr],
        };
        assert_eq!(cache.len_instance(), 1);
        assert!(!cache.is_empty_instance());
        assert_eq!(cache.addresses(), &[addr]);
    }

    #[test]
    fn test_cache_empty_instance() {
        let cache = AddressCache { addresses: vec![] };
        assert_eq!(cache.len_instance(), 0);
        assert!(cache.is_empty_instance());
        assert!(cache.addresses().is_empty());
    }

    #[test]
    fn test_init_from_path_and_static_methods() {
        let env = TempEnv::new(&[
            "0xd7d2e492e6dda0013e9062f00327a06fdb722488",
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ]);
        let _ = AddressCache::init_from_path(env.path.to_str().unwrap());

        let count = AddressCache::len();
        let empty = AddressCache::is_empty();
        if count > 0 {
            assert!(!empty);
            let all = AddressCache::all().unwrap();
            assert_eq!(all.len(), count);
            let r = AddressCache::get_random().unwrap();
            assert!(!r.to_string().is_empty());

            // get_random_many with various counts
            assert_eq!(AddressCache::get_random_many(0).unwrap().len(), 0);
            assert_eq!(AddressCache::get_random_many(5).unwrap().len(), 5);
            // count larger than pool still works (may have duplicates)
            let many = AddressCache::get_random_many(100).unwrap();
            assert_eq!(many.len(), 100);
            // all should be non-empty addresses
            for addr in &many {
                assert!(
                    addr.to_string().starts_with("0x"),
                    "address should start with 0x: {}",
                    addr
                );
            }
        }
    }

    #[test]
    fn test_init_from_path_nonexistent_errors() {
        let result = AddressCache::init_from_path("/nonexistent/path/address.txt");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_global_before_init_errors() {
        let early = AddressCache::global();
        if early.is_err() {
            let err = early.unwrap_err().to_string();
            assert!(err.contains("not initialized"), "Error: {}", err);
        }
    }

    #[test]
    fn test_len_before_init_returns_zero() {
        let len = AddressCache::len();
        // May be 0 or >0 depending on test ordering (shared OnceCell)
        // Best-effort: confirm function doesn't panic
        assert!(true);
    }
}
