use anyhow::Result;
use reqwest::Client;
use std::time::Duration;
use tempo_spammer::tasks::{ProxyConfig, load_proxies};

#[tokio::main]
async fn main() -> Result<()> {
    println!("🔍 Debug Proxy Tool");

    // 1. Load Proxies
    let proxies_path = "proxies.txt";
    let proxies = match load_proxies(proxies_path) {
        Ok(p) => {
            if p.is_empty() {
                println!(
                    "⚠️ No proxies found in {}, trying config/proxies.txt",
                    proxies_path
                );
                match load_proxies("config/proxies.txt") {
                    Ok(p2) => p2,
                    Err(_) => {
                        println!("❌ Failed to load any proxies. Exiting.");
                        return Ok(());
                    }
                }
            } else {
                p
            }
        }
        Err(e) => {
            println!("❌ Error loading proxies: {:?}", e);
            return Ok(());
        }
    };

    println!("✅ Loaded {} proxies", proxies.len());

    let target_url = "https://rpc.moderato.tempo.xyz";
    // We can also try a simpler URL like google to verify connectivity first
    let _health_check_url = "http://www.google.com";

    for (i, proxy_config) in proxies.iter().enumerate().take(5) {
        println!("\nTesting Proxy #{}: {}", i + 1, proxy_config.url);
        if let Some(user) = &proxy_config.username {
            println!("   Auth: {}:***", user);
        }

        match check_proxy(proxy_config, target_url).await {
            Ok(status) => println!("   ✅ Status: {}", status),
            Err(e) => println!("   ❌ Failed: {:?}", e),
        }
    }

    Ok(())
}

async fn check_proxy(config: &ProxyConfig, url: &str) -> Result<String> {
    let client_builder = Client::builder().timeout(Duration::from_secs(10));

    let proxy = reqwest::Proxy::all(&config.url)?;
    let proxy = if let (Some(u), Some(p)) = (&config.username, &config.password) {
        proxy.basic_auth(u, p)
    } else {
        proxy
    };

    let client = client_builder.proxy(proxy).build()?;

    let res = client.get(url).send().await?;
    Ok(res.status().to_string())
}
