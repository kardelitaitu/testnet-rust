use anyhow::{Context, Result};
use rand::{seq::SliceRandom, thread_rng};
use reqwest::Client;
use scraper::{Html, Selector};
use std::time::Duration;

use crate::config::ProxyConfig;

#[derive(Debug, Clone)]
pub struct ExplorerGasTrackerPayload {
    pub url: String,
    pub row_label: String,
    pub proxies: Vec<ProxyConfig>,
    pub request_timeout: Duration,
}

impl ExplorerGasTrackerPayload {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            row_label: "Normal".to_string(),
            proxies: Vec::new(),
            request_timeout: Duration::from_secs(60),
        }
    }

    pub fn with_row_label(mut self, row_label: impl Into<String>) -> Self {
        self.row_label = row_label.into();
        self
    }

    pub fn with_proxies(mut self, proxies: Vec<ProxyConfig>) -> Self {
        self.proxies = proxies;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }
}

impl Default for ExplorerGasTrackerPayload {
    fn default() -> Self {
        Self::new("")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExplorerGasSnapshot {
    pub source_url: String,
    pub row_label: String,
    pub normal_gwei: f64,
    pub base_gwei: Option<f64>,
    pub priority_gwei: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ExplorerGasTracker {
    client: Client,
    payload: ExplorerGasTrackerPayload,
}

impl ExplorerGasTracker {
    pub fn new(payload: ExplorerGasTrackerPayload) -> Result<Self> {
        let client = Self::build_client(&payload)?;
        Ok(Self { client, payload })
    }

    pub fn from_url(url: impl Into<String>) -> Result<Self> {
        Self::new(ExplorerGasTrackerPayload::new(url))
    }

    pub fn with_payload(mut self, payload: ExplorerGasTrackerPayload) -> Self {
        self.payload = payload;
        self
    }

    fn build_client(payload: &ExplorerGasTrackerPayload) -> Result<Client> {
        let mut client_builder = Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .timeout(payload.request_timeout);

        if let Some(proxy_conf) = payload.proxies.choose(&mut thread_rng()) {
            let mut proxy = reqwest::Proxy::all(&proxy_conf.url)
                .with_context(|| format!("Invalid proxy URL: {}", proxy_conf.url))?;
            if let (Some(u), Some(p)) = (&proxy_conf.username, &proxy_conf.password) {
                proxy = proxy.basic_auth(u, p);
            }
            client_builder = client_builder.proxy(proxy);
        }

        client_builder
            .build()
            .context("Failed to build explorer gas tracker HTTP client")
    }

    pub async fn fetch_snapshot(&self) -> Result<ExplorerGasSnapshot> {
        let client = if self.payload.proxies.is_empty() {
            self.client.clone()
        } else {
            Self::build_client(&self.payload)?
        };
        let response = client
            .get(&self.payload.url)
            .send()
            .await
            .with_context(|| format!("Failed to request gas tracker page: {}", self.payload.url))?
            .error_for_status()
            .with_context(|| format!("Gas tracker returned non-success status: {}", self.payload.url))?;

        let html = response
            .text()
            .await
            .with_context(|| format!("Failed to read gas tracker HTML: {}", self.payload.url))?;

        Self::parse_snapshot(&html, &self.payload)
            .with_context(|| format!("Failed to parse gas tracker HTML: {}", self.payload.url))
    }

    pub async fn fetch_normal_gwei(&self) -> Result<f64> {
        Ok(self.fetch_snapshot().await?.normal_gwei)
    }

    pub fn parse_snapshot(
        html: &str,
        payload: &ExplorerGasTrackerPayload,
    ) -> Result<ExplorerGasSnapshot> {
        let document = Html::parse_document(html);
        let li_selector = Selector::parse("li").expect("valid selector");

        for li in document.select(&li_selector) {
            let text = normalize_text(&li.text().collect::<Vec<_>>().join(" "));
            if !text.contains(&payload.row_label) {
                continue;
            }

            let tokens: Vec<&str> = text.split_whitespace().collect();
            let normal_gwei = find_number_before_unit(&tokens, "Gwei")
                .with_context(|| format!("No Gwei value found in tracker row: {}", text))?;
            let base_gwei = find_number_after_label(&tokens, "Base");
            let priority_gwei = find_number_after_label(&tokens, "Priority");

            return Ok(ExplorerGasSnapshot {
                source_url: payload.url.clone(),
                row_label: payload.row_label.clone(),
                normal_gwei,
                base_gwei,
                priority_gwei,
            });
        }

        anyhow::bail!(
            "No tracker row found for label '{}' at {}",
            payload.row_label,
            payload.url
        )
    }
}

fn normalize_text(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_number(token: &str) -> Option<f64> {
    token.replace(',', "").parse::<f64>().ok()
}

fn find_number_before_unit(tokens: &[&str], unit: &str) -> Option<f64> {
    tokens.windows(2).find_map(|pair| match pair {
        [num, token] if token.eq_ignore_ascii_case(unit) => parse_number(num),
        _ => None,
    })
}

fn find_number_after_label(tokens: &[&str], label: &str) -> Option<f64> {
    tokens.windows(2).find_map(|pair| match pair {
        [token, num] if token.eq_ignore_ascii_case(label) => parse_number(num),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        <li class="css-glt1hu">
            <div class="chakra-skeleton css-gb11u3">Normal</div>
            <div class="css-zhvi1f">
                <svg class="chakra-skeleton css-1s7bcni"><use href="/icons/sprite.d2fba260.svg#gas_xl"></use></svg>
                <div class="chakra-skeleton css-1aphmpp"><span class="css-ivo10y">644.6 Gwei</span></div>
            </div>
            <div class="chakra-skeleton css-werowh"><span> per transaction</span><span> / 45s</span></div>
            <div class="chakra-skeleton css-ns2ap1"><span>Base 634</span><span> / </span><span>Priority 10</span></div>
        </li>
    "#;

    #[test]
    fn parses_normal_gwei_snapshot() {
        let payload = ExplorerGasTrackerPayload::new("https://exptest.dachain.tech/gas-tracker");
        let snapshot = ExplorerGasTracker::parse_snapshot(SAMPLE, &payload).unwrap();

        assert_eq!(snapshot.row_label, "Normal");
        assert_eq!(snapshot.normal_gwei, 644.6);
        assert_eq!(snapshot.base_gwei, Some(634.0));
        assert_eq!(snapshot.priority_gwei, Some(10.0));
        assert_eq!(snapshot.source_url, payload.url);
    }

    #[test]
    fn rejects_missing_row() {
        let payload = ExplorerGasTrackerPayload::new("https://exptest.dachain.tech/gas-tracker");
        let err = ExplorerGasTracker::parse_snapshot("<div>Nothing here</div>", &payload).unwrap_err();
        assert!(err.to_string().contains("No tracker row found"));
    }
}
