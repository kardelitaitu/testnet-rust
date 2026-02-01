use anyhow::{Context, Result};
use clap::Parser;
use core_logic::security::SecurityUtils;
// use std::fs;
use serde_json::Value;

#[derive(Parser)]
struct Args {
    #[arg(short, long)]
    file: String,
    #[arg(short, long)]
    password: Option<String>,
}

fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = Args::parse();
    println!("Reading file: {}", args.file);

    let password = args
        .password
        .or_else(|| std::env::var("WALLET_PASSWORD").ok())
        .context("Password definition not found (arg or .env)")?;

    let content = std::fs::read_to_string(&args.file).context("Failed to read file")?;
    let json: Value = serde_json::from_str(&content).context("Failed to parse JSON")?;

    println!("JSON Structure parsed successfully.");

    // Extract fields based on observed structure
    let encrypted = json.get("encrypted").context("Missing 'encrypted' field")?;
    let ciphertext = encrypted
        .get("ciphertext")
        .and_then(|v| v.as_str())
        .context("Missing ciphertext")?;
    let iv = encrypted
        .get("iv")
        .and_then(|v| v.as_str())
        .context("Missing iv")?;
    let salt = encrypted
        .get("salt")
        .and_then(|v| v.as_str())
        .context("Missing salt")?;
    let tag = encrypted
        .get("tag")
        .and_then(|v| v.as_str())
        .context("Missing tag")?; // AES-GCM tag often appended or separate

    println!("Ciphertext: {}...", &ciphertext[..20.min(ciphertext.len())]);
    println!("IV: {}", iv);
    println!("Salt: {}", salt);
    println!("Attempting decryption...");

    // Use the updated SecurityUtils which now uses Scrypt with correct params
    match SecurityUtils::decrypt_components(ciphertext, iv, salt, tag, &password) {
        Ok(plaintext) => {
            println!("SUCCESS!");
            println!("Decrypted: {}", plaintext);
        }
        Err(e) => {
            println!("Failed: {}", e);
        }
    }

    Ok(())
}
