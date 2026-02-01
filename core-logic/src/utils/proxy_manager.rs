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

        let content = fs::read_to_string(path).context("Failed to read proxies.txt")?;
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
