use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

fn main() {
    let profile = env::var("PROFILE").unwrap_or_default();
    let is_release = profile == "release";

    // Check if values are already set via environment (for CI/CD)
    let wallet_password = env::var("WALLET_PASSWORD");
    let tempo_workers = env::var("TEMPO_WORKERS");

    let (password, workers_num) = if is_release {
        // Release build: prompt for values if not set via env
        if wallet_password.is_ok() && tempo_workers.is_ok() {
            println!("cargo:warning=Using environment variables for build configuration");
            (
                wallet_password.unwrap(),
                tempo_workers.unwrap().parse().unwrap_or(20),
            )
        } else {
            println!("\n╔════════════════════════════════════════════════════════════╗");
            println!("║           TEMPO SPAMMER BUILD CONFIGURATION                ║");
            println!("╚════════════════════════════════════════════════════════════╝\n");

            // Prompt for wallet password
            let password = if wallet_password.is_err() {
                print!("🔐 Enter wallet password: ");
                io::stdout().flush().unwrap();
                let mut input = String::new();
                io::stdin()
                    .read_line(&mut input)
                    .expect("Failed to read password");
                input.trim().to_string()
            } else {
                wallet_password.unwrap()
            };

            // Prompt for worker count
            let workers = if tempo_workers.is_err() {
                print!("👷 Enter number of workers [default: 20]: ");
                io::stdout().flush().unwrap();
                let mut input = String::new();
                io::stdin()
                    .read_line(&mut input)
                    .expect("Failed to read worker count");
                let input = input.trim();
                if input.is_empty() {
                    "20".to_string()
                } else {
                    input.to_string()
                }
            } else {
                tempo_workers.unwrap()
            };

            let workers_num: u64 = workers.parse().unwrap_or(20);

            println!("\n✅ Build configuration saved:");
            println!("   Workers: {}", workers_num);
            println!("   Password: [hidden]\n");

            (password, workers_num)
        }
    } else {
        // Debug build: use environment variables or empty defaults
        println!("cargo:warning=Debug build - using defaults or environment variables");
        (
            wallet_password.unwrap_or_default(),
            tempo_workers.unwrap_or_default().parse().unwrap_or(0),
        )
    };

    // Generate compile-time constants (always do this)
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("build_config.rs");

    let config_code = format!(
        r#"pub const COMPILE_TIME_PASSWORD: &str = "{}";
pub const COMPILE_TIME_WORKERS: u64 = {};
"#,
        password.replace('"', "\\\""), // Escape quotes
        workers_num
    );

    fs::write(&dest_path, config_code).expect("Failed to write build config");

    // Also set environment variables for this build
    if !password.is_empty() {
        println!("cargo:rustc-env=WALLET_PASSWORD={}", password);
    }
    if workers_num > 0 {
        println!("cargo:rustc-env=TEMPO_WORKERS={}", workers_num);
    }

    // Tell cargo to rerun if build script changes
    println!("cargo:rerun-if-changed=build.rs");
}
