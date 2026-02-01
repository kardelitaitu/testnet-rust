use anyhow::{Error, Result};
use chrono::{DateTime, Utc};
use chrono_tz::Asia::Bangkok;
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info};

// Include compile-time Telegram configuration from build.rs
include!(concat!(env!("OUT_DIR"), "/build_config.rs"));

/// Telegram bot configuration
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
}

impl TelegramConfig {
    /// Load configuration from Cargo.toml metadata
    /// Configure in [package.metadata.telegram] section of Cargo.toml
    pub fn new() -> Self {
        Self {
            bot_token: TELEGRAM_BOT_TOKEN.to_string(),
            chat_id: TELEGRAM_CHAT_ID.to_string(),
        }
    }
}

/// Telegram notification service
pub struct TelegramNotifier {
    config: TelegramConfig,
    client: Client,
    start_time: DateTime<Utc>,
    ip_address: String,
}

impl TelegramNotifier {
    pub async fn new(config: TelegramConfig) -> Self {
        let client = Client::new();
        let ip_address = Self::fetch_public_ip(&client).await;

        Self {
            config,
            client,
            start_time: Utc::now(),
            ip_address,
        }
    }

    /// Fetch public IP address using ipify.org API
    async fn fetch_public_ip(client: &Client) -> String {
        match client
            .get("https://api.ipify.org?format=text")
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    match response.text().await {
                        Ok(ip) => {
                            info!("Public IP address detected: {}", ip.trim());
                            ip.trim().to_string()
                        }
                        Err(e) => {
                            error!("Failed to parse IP response: {}", e);
                            "Unknown".to_string()
                        }
                    }
                } else {
                    error!("IP API returned error status: {}", response.status());
                    "Unknown".to_string()
                }
            }
            Err(e) => {
                error!("Failed to fetch public IP: {}", e);
                "Unknown".to_string()
            }
        }
    }

    /// Send a message to Telegram
    pub async fn send_message(&self, message: &str) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.config.bot_token
        );

        let payload = serde_json::json!({
            "chat_id": self.config.chat_id,
            "text": message,
            "parse_mode": "Markdown",
            "disable_notification": false,
        });

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| Error::new(e).context("Failed to send Telegram request"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!("Telegram API error: {} - {}", status, text);
            return Err(Error::msg(format!(
                "Telegram API error: {} - {}",
                status, text
            )));
        }

        info!("Telegram notification sent successfully");
        Ok(())
    }

    /// Format status message with GMT+7 (Asia/Bangkok) timezone
    fn format_status_message(&self, is_first: bool) -> String {
        let now_utc = Utc::now();
        let now_gmt7 = now_utc.with_timezone(&Bangkok);
        let uptime = now_utc.signed_duration_since(self.start_time);

        let uptime_str = if uptime.num_hours() > 0 {
            format!("{}h {}m", uptime.num_hours(), uptime.num_minutes() % 60)
        } else if uptime.num_minutes() > 0 {
            format!("{}m {}s", uptime.num_minutes(), uptime.num_seconds() % 60)
        } else {
            format!("{}s", uptime.num_seconds())
        };

        if is_first {
            format!(
                "🚀 *VPS + tempo-spammer started*\n\n\
                ✅ Status: Running\n\
                🌐 IP Address: `{}`\n\
                🕐 Start time: {} (GMT+7)\n\
                📍 VPS is active and operational",
                self.ip_address,
                now_gmt7.format("%Y-%m-%d %H:%M:%S")
            )
        } else {
            format!(
                "✅ *VPS + tempo-spammer is running*\n\n\
                🌐 IP Address: `{}`\n\
                🕐 Current time: {} (GMT+7)\n\
                ⏱️ Uptime: {}\n\
                📍 VPS is healthy and operational",
                self.ip_address,
                now_gmt7.format("%Y-%m-%d %H:%M:%S"),
                uptime_str
            )
        }
    }

    /// Start the notification scheduler
    /// Sends first notification immediately, then every 3 hours
    pub async fn start(self: Arc<Self>) {
        info!("Starting Telegram notification service (every 3 hours)");

        // Send first notification immediately
        let message = self.format_status_message(true);
        if let Err(e) = self.send_message(&message).await {
            error!("Failed to send initial Telegram notification: {}", e);
        } else {
            info!("Initial Telegram notification sent");
        }

        // Create interval for every 3 hours (3 * 60 * 60 = 10800 seconds)
        let mut interval = interval(Duration::from_secs(3 * 60 * 60));

        loop {
            interval.tick().await;

            let message = self.format_status_message(false);
            match self.send_message(&message).await {
                Ok(_) => info!("Periodic Telegram notification sent"),
                Err(e) => error!("Failed to send Telegram notification: {}", e),
            }
        }
    }
}

/// Initialize and spawn the notification service
pub async fn spawn_notification_service() -> Option<tokio::task::JoinHandle<()>> {
    let config = TelegramConfig::new();

    info!("Initializing Telegram bot (chat_id: {})", config.chat_id);

    let notifier = Arc::new(TelegramNotifier::new(config).await);

    Some(tokio::spawn(async move {
        notifier.start().await;
    }))
}
