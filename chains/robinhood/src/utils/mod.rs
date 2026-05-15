pub mod gas;

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use reqwest::Url;
use core_logic::config::ProxyConfig;

pub fn load_proxies(path: &str) -> Result<Vec<ProxyConfig>> {
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path).context("Failed to read proxies.txt")?;

    let proxies: Vec<ProxyConfig> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let line = line.trim();
            // Try parsing as URL first if it looks like one
            if line.starts_with("http") && line.contains('@') {
                if let Ok(u) = Url::parse(line) {
                    let host = u.host_str().unwrap_or("").to_string();
                    let port = u
                        .port()
                        .unwrap_or(if u.scheme() == "https" { 443 } else { 80 });
                    let username = if !u.username().is_empty() {
                        Some(u.username().to_string())
                    } else {
                        None
                    };
                    let password = if let Some(p) = u.password() {
                        Some(p.to_string())
                    } else {
                        None
                    };

                    let base_url = format!("{}://{}:{}", u.scheme(), host, port);

                    return Some(ProxyConfig {
                        url: base_url,
                        username,
                        password,
                    });
                }
            }

            let parts: Vec<&str> = line.split(':').map(|s| s.trim()).collect();
            match parts.len() {
                1 => Some(ProxyConfig {
                    url: if parts[0].starts_with("http") {
                        parts[0].to_string()
                    } else {
                        format!("http://{}", parts[0])
                    },
                    username: None,
                    password: None,
                }),
                2 => Some(ProxyConfig {
                    url: format!("http://{}:{}", parts[0], parts[1]),
                    username: None,
                    password: None,
                }),
                3 => Some(ProxyConfig {
                    url: format!("http://{}", parts[0]), 
                    username: Some(parts[1].to_string()),
                    password: Some(parts[2].to_string()),
                }),
                4 => Some(ProxyConfig {
                    url: format!("http://{}:{}", parts[0], parts[1]),
                    username: Some(parts[2].to_string()),
                    password: Some(parts[3].to_string()),
                }),
                _ => None,
            }
        })
        .collect();

    Ok(proxies)
}
