# 🚀 Quick Deployment Guide

> **Target Environment:** Cheap NVMe VPS/Server  
> **Prerequisites:** Fresh Linux/Windows server with Rust toolchain

---

## 📋 System Requirements

- **CPU:** 2+ cores (optimized for high-concurrency)
- **RAM:** 4GB minimum, 8GB recommended
- **Storage:** NVMe SSD (any size, fast I/O for wallet operations)
- **OS:** Ubuntu 20.04+ / Debian 11+ / Windows Server 2019+
- **Network:** Stable connection (required for RPC calls)

---

## 🛠️ Step 1: Install Rust Toolchain

### Linux/macOS
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustc --version  # Verify installation
```

### Windows
Download and run: https://rustup.rs/  
Then verify in PowerShell:
```powershell
rustc --version
```

---

## 📦 Step 2: Clone the Repository

```bash
# Clone the repo
git clone https://github.com/kardelitaitu/testnet-rust.git
cd testnet-rust

# Verify structure
ls -la
```

You should see:
```
chains/
  ├── risechain/
  ├── tempo-spammer/
  └── evm-project/
core-logic/
Cargo.toml
```

---

## 🔐 Step 3: Prepare Wallet Files

### Option A: Use Existing Encrypted Wallets
1. Copy your encrypted wallet files to `wallets/` directory:
   ```bash
   mkdir -p wallets
   # Upload your wallet_*.json files here
   ```

2. Set the wallet password as environment variable:
   ```bash
   export WALLET_PASSWORD="your_secure_password"
   ```
   Or on Windows:
   ```powershell
   $env:WALLET_PASSWORD = "your_secure_password"
   ```

### Option B: Generate New Wallets
Use the wallet generator tool (if available) or create them manually.

---

## 🌐 Step 4: Configure Proxies (Optional)

If using proxies, create `proxies.txt` in the root directory:
```
http://user:pass@ip:port
socks5://user:pass@ip:port
http://ip:port
```

**Note:** Leave empty or delete the file if not using proxies.

---

## ⚙️ Step 5: Configure the Spammer

### For Tempo Spammer
Edit `chains/tempo-spammer/config/config.toml`:
```toml
[network]
rpc_url = "https://your-tempo-rpc-url"
chain_id = 41714  # Adjust as needed

[spammer]
num_workers = 10          # Concurrent workers
task_timeout_secs = 120   # Task timeout
max_retries = 3           # Retry failed tasks

[wallet]
path = "../../wallets"    # Path to wallet directory
```

### For RiseChain
Edit `chains/risechain/config/config.toml` similarly.

---

## 🔨 Step 6: Build the Project

### Clean Build (Recommended for first deployment)
```bash
# Linux/macOS
cargo clean
cargo build --release --workspace

# Windows (use the batch script)
._clean_and_compile_all.bat
```

**Build Time:** 5-15 minutes depending on CPU  
**Output:** Optimized binaries in `target/release/`

### Verify Build
```bash
ls -lh target/release/
```

You should see binaries like:
- `tempo-spammer` (or `tempo-spammer.exe` on Windows)
- `debug_task`
- Other project binaries

---

## 🎯 Step 7: Test with Debug Mode (Recommended)

Before running the full spammer, test with a single task:

```bash
cargo run --bin debug_task --release
```

This will:
1. Prompt for wallet password (if not set via env var)
2. Run a single task to verify configuration
3. Show detailed logs for debugging

**Expected Output:**
```
✅ Wallet loaded successfully
✅ RPC connection established
✅ Task executed: [TaskName]
```

---

## 🚀 Step 8: Run the Spammer

### Tempo Spammer
```bash
# Linux/macOS
cargo run --release --bin tempo-spammer

# Or use the direct binary
./target/release/tempo-spammer
```

### Windows
```powershell
# Use the batch script
._start-tempo-spammer.bat

# Or run directly
.\target\release\tempo-spammer.exe
```

### RiseChain Spammer
```bash
cargo run --release --bin rise-project
```

---

## 📊 Step 9: Monitor Execution

### Log Output
- **Stdout:** Real-time human-readable logs
- **File:** `logs/smart_main.log` (audit-ready, grepable)

### Check Logs
```bash
# Follow live logs
tail -f logs/smart_main.log

# Search for errors
grep -i "error" logs/smart_main.log

# Count successful tasks
grep -c "✅" logs/smart_main.log
```

### Performance Metrics
Watch for:
- **TPS** (Transactions Per Second)
- **Success Rate** (%)
- **Proxy Health** (if using proxies)
- **Nonce Management** (no "nonce too low" errors)

---

## 🔧 Troubleshooting

### Issue: "nonce too low" errors
**Solution:** The framework uses lazy wallet loading with robust nonce management. If you see this:
1. Check RPC connection stability
2. Reduce `num_workers` in config
3. Review logs for specific wallet errors

### Issue: Proxy connection failures
**Solution:** 
1. Verify proxy format in `proxies.txt`
2. Test proxies manually: `curl --proxy http://proxy:port https://google.com`
3. Remove dead proxies from the list

### Issue: Build errors
**Solution:**
1. Update Rust: `rustup update`
2. Clean build: `cargo clean && cargo build --release`
3. Check `Cargo.lock` is present (don't delete it)

### Issue: "Calculated amount is zero"
**Solution:**
1. Ensure wallets have sufficient funds
2. Check token contract addresses in config
3. Review RPC responses in debug mode

---

## 📈 Optimization Tips

### For High-Performance VPS
- **CPU-bound:** Increase `num_workers` in config (test incrementally)
- **Network-bound:** Use quality proxies or direct connection
- **I/O-bound:** Ensure NVMe is used (check with `df -h`)

### For Limited Resources
- **Low RAM:** Reduce `num_workers` to 5 or less
- **Slow CPU:** Use `--release` build (already optimized)
- **Poor Network:** Increase `task_timeout_secs` to 180+

### Session Management
Run in background with `tmux` or `screen`:
```bash
# Create session
tmux new -s spammer

# Run spammer
cargo run --release --bin tempo-spammer

# Detach: Ctrl+B then D
# Reattach: tmux attach -t spammer
```

---

## 🛡️ Security Checklist

- [ ] Wallet password is set via environment variable (not hardcoded)
- [ ] `proxies.txt` does not contain credentials in plain text (if shared)
- [ ] Logs directory is secured (contains wallet activity)
- [ ] `.gitignore` excludes `wallets/`, `proxies.txt`, `logs/`
- [ ] Server firewall is configured (only necessary ports open)

---

## 🎓 Advanced Usage

### Run Specific Tasks Only
Edit the task selection logic in the binary source code to filter tasks by ID.

### Custom Task Development
1. Implement the `RiseTask` trait in `core-logic/src/utils/task_trait.rs`
2. Add your task to the chain's `tasks/` directory
3. Register in the main binary
4. Test with `debug_task` before deploying

### Multi-Chain Deployment
Run multiple spammers simultaneously:
```bash
# Terminal 1
cargo run --release --bin tempo-spammer

# Terminal 2
cargo run --release --bin rise-project
```

---

## 📞 Quick Reference

| Command | Purpose |
| :--- | :--- |
| `._clean_and_compile_all.bat` | Clean build (Windows) |
| `cargo run --bin debug_task` | Test single task |
| `cargo run --release --bin tempo-spammer` | Run Tempo spammer |
| `cargo run --release --bin rise-project` | Run Rise spammer |
| `tail -f logs/smart_main.log` | Monitor logs |
| `cargo check --workspace` | Verify compilation |

---

## 📝 Post-Deployment

After successful deployment:
1. **Backup:** Save encrypted wallets and config files
2. **Monitor:** Set up log rotation (`logrotate` on Linux)
3. **Update:** Pull latest changes: `git pull && cargo build --release`
4. **Scale:** Increase workers gradually based on server capacity

---

**🎉 You're ready to spam!** For issues, check `logs/smart_main.log` or refer to the architecture in `GEMINI.md`.
