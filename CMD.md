# Command Reference (CMD)

This document lists common commands for building, running, and debugging the testnet framework.

## 🏗️ Build & Setup

### Full Workspace Build
Cleans `target_final`, compiles all crates in parallel, and moves binaries to `target_final/debug`.
```powershell
.\_clean_and_compile_all.bat
```

### Check Logic Only
```powershell
cargo check --workspace
```

## 🐛 Debugging

### Run Debugger (Interactive)
```powershell
$env:WALLET_PASSWORD="password"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml
```

### Check Balances for All Wallets
```powershell
$env:WALLET_PASSWORD="password"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml --all
```

### Run Specific Task Directly
Use `--task <INDEX>` (0 = Faucet, 1 = Balance).
```powershell
.\target_final\debug\debug_task.exe --config chains/risechain/config.toml --task 1
```

## 🤖 Spammers

### RISE Spammer
```powershell
$env:WALLET_PASSWORD="password"; .\target_final\debug\rise-project.exe --config chains/risechain/config.toml
```

### EVM Spammer (Generic)
```powershell
$env:WALLET_PASSWORD="password"; .\target_final\debug\evm-project.exe --config chains/evm-project/config.toml
```

## ⚙️ Configuration

### Environment Variables
*   `WALLET_PASSWORD`: Password to decrypt JSON wallets. (Optional: Debugger prompts if missing).
*   `RUST_BACKTRACE=1`: Enable stack traces for crashes.
*   `RUST_LOG=debug`: Enable verbose logging (if `EnvFilter` is configured to use it).

### Config Files
*   **RISE**: `chains/risechain/config.toml`
*   **EVM**: `chains/evm-project/config.toml`


#### check wallet balance : 
```powershell
$env:WALLET_PASSWORD="password"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml --all
```