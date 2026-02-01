# Rust Multi-Chain Testnet Framework

A modular, high-performance optimization of the testnet automation framework, rewritten in Rust.

## 🚀 Features

*   **Multi-Chain Architecture**: Dedicated support for EVM (Rise, Monad, etc.) and Non-EVM chains (Solana, Sui).
*   **High Performance**:
    *   **Lazy Wallet Decryption**: Decrypts wallets only when needed (CLI/Spammer), caching results for efficiency.
    *   **Parallel Compilation**: Optimized build scripts for rapid development cycles.
    *   **Async/Await**: Built on `tokio` for efficient concurrent task execution.
*   **Modular Design**:
    *   **Core Logic**: Shared library for Wallet Manager, Logging, and Proxy management.
    *   **Task System**: Trait-based task definition (`RiseTask`) for easy extension.
    *   **Centralized Logging**: `tracing`-based structured logging with file and console output.
*   **Robust Tooling**:
    *   **Debugger (`debug_task`)**: Interactive CLI to test individual tasks, check balances, and debug specific wallets.
    *   **Spammer**: High-throughput automated transaction generator with random delays and proxy rotation.

## 📂 Project Structure

```
testnet-framework/
├── core-logic/             # Shared library (Wallets, Logging, Config)
│   ├── src/utils/          # WalletManager, ProxyManager, Logger
│   └── Cargo.toml
├── chains/
│   ├── risechain/          # RISE Chain implementation
│   │   ├── src/bin/        # Binaries (spammer, debug_task)
│   │   ├── src/task/       # Task implementations (Faucet, Balance)
│   │   └── config.toml     # Chain-specific config
│   ├── evm-project/        # Generic EVM template
│   └── solana-project/     # Solana implementation (WIP)
├── wallet-json/            # Encrypted wallet storage
├── proxies.txt             # Proxy list (ip:port:user:pass)
├── _clean_and_compile_all.bat # Build script
└── README.md
```

## 🛠️ Prerequisites

*   [Rust](https://www.rust-lang.org/tools/install) (Latest Stable)
*   `sqlite` (optional, for persistent tracking)

## ⚡ Quick Start

### 1. Build the Project
Use the optimized batch script to clean and build all components:
```powershell
.\_clean_and_compile_all.bat
```

### 2. Configure
Edit `chains/risechain/config.toml` to set RPC endpoints and worker limits.

### 3. Run Debugger
Interactive tool to check balances or run specific tasks:
```powershell
$env:WALLET_PASSWORD="your_password"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml
```

### 4. Run Spammer
Start the automated worker swarm:
```powershell
$env:WALLET_PASSWORD="your_password"; .\target_final\debug\rise-project.exe --config chains/risechain/config.toml
```

## 🔐 Security
*   **Wallet Encryption**: Wallets are stored as encrypted JSON files (AES-256-GCM / Scrypt).
*   **Sensitive Data**: Passwords are handled via environment variables (`WALLET_PASSWORD`) or secure interactive prompts.

## 🤖 Telegram Bot Notifications

The tempo-spammer includes an integrated Telegram bot that sends status notifications:
- **First notification**: Immediately when the spammer starts
- **Periodic notifications**: Every 3 hours while running

### Configuration (Pre-configured)

The bot is **already configured** with hardcoded credentials:
- **Bot Token**: `8405826533:AAEKFRxIfmCpXskDHsbP3h3DdtbzjvcJbZg`
- **Chat ID**: `1754837820`

**No setup required!** Just run the spammer and notifications will be sent automatically.

### Usage

```bash
cargo run -p tempo-spammer --release
```

### Notification Format

**First run:**
```
🚀 *VPS + tempo-spammer started*

✅ Status: Running
🌐 IP Address: `203.0.113.45`
🕐 Start time: 2026-02-01 12:17:00 (GMT+7)
📍 VPS is active and operational
```

**Every 3 hours:**
```
✅ *VPS + tempo-spammer is running*

🌐 IP Address: `203.0.113.45`
🕐 Current time: 2026-02-01 15:17:00 (GMT+7)
⏱️ Uptime: 3h 0m
📍 VPS is healthy and operational
```

**Features:**
- 🌐 **IP Address**: Automatically detects and displays your VPS public IP
- ⏱️ **Uptime**: Tracks how long the spammer has been running
- 🕐 **Timestamps**: All times in **GMT+7** (Asia/Bangkok timezone) with clear timezone indicator

### Customization

To use a different bot or chat ID, modify `src/bot/notification.rs`:
```rust
pub fn new() -> Self {
    Self {
        bot_token: "YOUR_BOT_TOKEN".to_string(),
        chat_id: "YOUR_CHAT_ID".to_string(),
    }
}

## 📦 Build-Time Configuration (No .env files needed!)

The tempo-spammer supports **compile-time configuration** - password and worker count are baked into the binary during build, eliminating the need for `.env` files or runtime configuration.

### Debug Build
Uses defaults or environment variables:
```bash
cargo build -p tempo-spammer
```

### Release Build (Interactive Prompts)
Prompts for password and workers during compilation:
```bash
cargo build -p tempo-spammer --release
```

**During release build, you'll see:**
```
╔════════════════════════════════════════════════════════════╗
║           TEMPO SPAMMER BUILD CONFIGURATION                ║
╚════════════════════════════════════════════════════════════╝

🔐 Enter wallet password: [your-password]
👷 Enter number of workers [default: 20]: 10

✅ Build configuration saved:
   Workers: 10
   Password: [hidden]
```

### Alternative: Environment Variables
Set values during build to skip prompts:
```bash
# Windows PowerShell
$env:WALLET_PASSWORD="your-password"; $env:TEMPO_WORKERS="15"; cargo build -p tempo-spammer --release

# Linux/macOS
WALLET_PASSWORD="your-password" TEMPO_WORKERS="15" cargo build -p tempo-spammer --release
```

### Running the Binary
Once built, the binary contains your configuration:
```bash
# No configuration needed - runs immediately!
./target/release/tempo-spammer.exe

# Or override at runtime:
$env:WALLET_PASSWORD="different-password"; ./target/release/tempo-spammer.exe --workers 25
```

**Configuration Priority:**
1. Runtime environment variables (highest priority)
2. Compile-time values (baked into binary)
3. Interactive prompts (fallback)
