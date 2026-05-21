pub mod gas;

use anyhow::{Context, Result};
use core_logic::config::ProxyConfig;
use reqwest::Url;
use std::fs;
use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct TempProxy {
        path: std::path::PathBuf,
    }

    impl TempProxy {
        fn new(content: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "proxy_test_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
            let mut f = std::fs::File::create(&path).unwrap();
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
        let proxies = load_proxies("/nonexistent/proxies.txt").unwrap();
        assert!(proxies.is_empty());
    }

    #[test]
    fn test_empty_file_returns_empty() {
        let tp = TempProxy::new("");
        let proxies = load_proxies(tp.path.to_str().unwrap()).unwrap();
        assert!(proxies.is_empty());
    }

    #[test]
    fn test_single_ip_port() {
        let tp = TempProxy::new("192.168.1.1:8080");
        let proxies = load_proxies(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].url, "http://192.168.1.1:8080");
        assert!(proxies[0].username.is_none());
    }

    #[test]
    fn test_ip_port_user_pass() {
        let tp = TempProxy::new("192.168.1.1:8080:user:pass");
        let proxies = load_proxies(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].url, "http://192.168.1.1:8080");
        assert_eq!(proxies[0].username.as_deref(), Some("user"));
        assert_eq!(proxies[0].password.as_deref(), Some("pass"));
    }

    #[test]
    fn test_ip_user_pass_three_parts() {
        let tp = TempProxy::new("192.168.1.1:user:pass");
        let proxies = load_proxies(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].url, "http://192.168.1.1");
        assert_eq!(proxies[0].username.as_deref(), Some("user"));
        assert_eq!(proxies[0].password.as_deref(), Some("pass"));
    }

    #[test]
    fn test_full_url_with_auth() {
        let tp = TempProxy::new("http://user:pass@proxy.com:3128");
        let proxies = load_proxies(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].url, "http://proxy.com:3128");
        assert_eq!(proxies[0].username.as_deref(), Some("user"));
        assert_eq!(proxies[0].password.as_deref(), Some("pass"));
    }

    #[test]
    fn test_multiple_proxies() {
        let tp = TempProxy::new("10.0.0.1:3128\n10.0.0.2:3128:admin:secret\nhttp://proxy.com:8080");
        let proxies = load_proxies(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 3);
    }

    #[test]
    fn test_skips_empty_lines() {
        let tp = TempProxy::new("10.0.0.1:3128\n\n\n10.0.0.2:3128");
        let proxies = load_proxies(tp.path.to_str().unwrap()).unwrap();
        assert_eq!(proxies.len(), 2);
    }
}
