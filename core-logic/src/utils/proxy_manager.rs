use crate::config::ProxyConfig;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

pub struct ProxyManager;

impl ProxyManager {
    const PROXY_FILE: &'static str = "proxies.txt";

    /// Loads proxies from proxies.txt
    /// Format expected: independent lines of ip:port:username:password
    pub fn load_proxies() -> Result<Vec<ProxyConfig>> {
        let path = Path::new(Self::PROXY_FILE);
        if !path.exists() {
            warn!("{} not found. Running without proxies.", Self::PROXY_FILE);
            return Ok(Vec::new());
        }

        Self::load_proxies_from(Self::PROXY_FILE)
    }

    /// Loads proxies from a given path (used for testing)
    fn load_proxies_from(path: &str) -> Result<Vec<ProxyConfig>> {
        let path = Path::new(path);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(path).context("Failed to read proxy file")?;
        let mut proxies = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Simple split by colon
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() < 2 {
                warn!("Skipping invalid proxy line: {}", line);
                continue;
            }

            // Basic parsing logic
            // ip:port:user:pass -> 4 parts
            // ip:port -> 2 parts
            let url = format!("http://{}:{}", parts[0], parts[1]);

            let (username, password) = if parts.len() >= 4 {
                (Some(parts[2].to_string()), Some(parts[3].to_string()))
            } else {
                (None, None)
            };

            proxies.push(ProxyConfig {
                url, // Store as base URL (http://ip:port)
                username,
                password,
            });
        }

        info!("Loaded {} proxies from {}", proxies.len(), Self::PROXY_FILE);
        Ok(proxies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempProxy {
        path: std::path::PathBuf,
    }

    impl TempProxy {
        fn new(content: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!("cm_proxy_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos()));
            let mut f = std::fs::File::create(&path).unwrap();
            use std::io::Write;
            write!(f, "{}", content).unwrap();
            TempProxy { path }
        }
    }

    impl Drop for TempProxy {
        fn drop(&mut self) {
            std::fs::remove_file(&self.path).ok();
        }
    }

    #[test]
    fn test_missing_file_returns_empty() {
        let proxies = ProxyManager::load_proxies_from("/nonexistent/proxies.txt").unwrap();
        assert!(proxies.is_empty());
    }

    #[test]
    fn test_empty_file_returns_empty() {
        let tp = TempProxy::new("");
        let proxies = ProxyManager::load_proxies_from(tp.path.to_str().unwrap()).unwrap();
        assert!(proxies.is_empty());
    }

    #[test]
    fn test_ip_port() {
        let tp = TempProxy::new("10.0.0.1:3128");
        let proxies = ProxyManager::load_proxies_from(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].url, "http://10.0.0.1:3128");
        assert!(proxies[0].username.is_none());
    }

    #[test]
    fn test_ip_port_user_pass() {
        let tp = TempProxy::new("10.0.0.1:3128:admin:secret");
        let proxies = ProxyManager::load_proxies_from(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].url, "http://10.0.0.1:3128");
        assert_eq!(proxies[0].username.as_deref(), Some("admin"));
        assert_eq!(proxies[0].password.as_deref(), Some("secret"));
    }

    #[test]
    fn test_skips_comments() {
        let tp = TempProxy::new("# this is a comment\n10.0.0.1:3128\n# another comment\n10.0.0.2:3128:user:pass");
        let proxies = ProxyManager::load_proxies_from(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 2);
    }

    #[test]
    fn test_skips_invalid_lines() {
        let tp = TempProxy::new("justanip\n10.0.0.1:3128");
        let proxies = ProxyManager::load_proxies_from(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 1);
    }

    #[test]
    fn test_multiple_proxies() {
        let tp = TempProxy::new("192.168.1.1:8080\n192.168.1.2:8080:u:p\n10.0.0.1:3128");
        let proxies = ProxyManager::load_proxies_from(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 3);
    }

    #[test]
    fn test_trailing_whitespace() {
        let tp = TempProxy::new("  10.0.0.1:3128  \n\t10.0.0.2:8080:user:pass\t\n");
        let proxies = ProxyManager::load_proxies_from(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 2);
        assert_eq!(proxies[0].url, "http://10.0.0.1:3128");
        assert_eq!(proxies[1].url, "http://10.0.0.2:8080");
        assert_eq!(proxies[1].username.as_deref(), Some("user"));
    }

    #[test]
    fn test_3_part_format_ip_port_user() {
        // ip:port:user without password — 3 parts → parts.len()=3 < 4, so user/pass are None
        let tp = TempProxy::new("10.0.0.1:3128:admin");
        let proxies = ProxyManager::load_proxies_from(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].url, "http://10.0.0.1:3128");
        assert!(proxies[0].username.is_none());
        assert!(proxies[0].password.is_none());
    }

    #[test]
    fn test_only_whitespace_lines_skipped() {
        let tp = TempProxy::new("   \n\t\n10.0.0.1:3128\n\n");
        let proxies = ProxyManager::load_proxies_from(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 1);
    }

    #[test]
    fn test_load_proxies_missing_file_returns_empty() {
        // load_proxies() uses the constant "proxies.txt" in CWD
        // When the file doesn't exist, it should return empty vec
        let cwd = std::env::current_dir().unwrap();
        let proxies_txt = cwd.join("proxies.txt");
        // Ensure there's no proxies.txt in the CWD
        let existed = proxies_txt.exists();
        if existed {
            std::fs::rename(&proxies_txt, cwd.join("proxies.txt.bak")).ok();
        }
        let result = ProxyManager::load_proxies().unwrap();
        assert!(result.is_empty(), "load_proxies() should return empty when proxies.txt missing");
        // Restore if we moved it
        if existed {
            std::fs::rename(cwd.join("proxies.txt.bak"), &proxies_txt).ok();
        }
    }
}
