use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Read Telegram configuration from Cargo.toml metadata
    let bot_token = env::var("CARGO_PKG_METADATA_TELEGRAM_BOT_TOKEN")
        .unwrap_or_else(|_| "8405826533:AAEKFRxIfmCpXskDHsbP3h3DdtbzjvcJbZg".to_string());
    let chat_id = env::var("CARGO_PKG_METADATA_TELEGRAM_CHAT_ID")
        .unwrap_or_else(|_| "1754837820".to_string());

    // Generate compile-time constants
    // Password and workers will be prompted at runtime, not build time
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("build_config.rs");

    let config_code = format!(
        r#"pub const COMPILE_TIME_PASSWORD: &str = "";
pub const COMPILE_TIME_WORKERS: u64 = 0;
pub const TELEGRAM_BOT_TOKEN: &str = "{}";
pub const TELEGRAM_CHAT_ID: &str = "{}";
"#,
        bot_token.replace('"', "\\\""),
        chat_id.replace('"', "\\\"")
    );

    fs::write(&dest_path, config_code).expect("Failed to write build config");

    // Tell cargo to rerun if build script or Cargo.toml changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
}
