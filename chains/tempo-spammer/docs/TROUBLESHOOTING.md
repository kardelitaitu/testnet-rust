# Troubleshooting Guide

Common issues and solutions for the tempo-spammer.

## Table of Contents
- [Quick Diagnostics](#quick-diagnostics)
- [Wallet Issues](#wallet-issues)
- [Transaction Failures](#transaction-failures)
- [Proxy Issues](#proxy-issues)
- [Database Issues](#database-issues)
- [Performance Issues](#performance-issues)
- [Getting Help](#getting-help)

---

## Quick Diagnostics

### Checklist Before Troubleshooting

Before diving into specific issues, verify these basics:

1. [ ] **Wallet password is set**: `echo $WALLET_PASSWORD` (Linux/Mac) or `$env:WALLET_PASSWORD` (PowerShell)
2. [ ] **Config file exists**: `ls config/config.toml`
3. [ ] **Wallets are present**: `ls wallet-json/*.json`
4. [ ] **Network connectivity**: `curl https://rpc.moderato.tempo.xyz`
5. [ ] **Build is current**: `cargo build -p tempo-spammer`

### Enable Debug Logging

```bash
# Set log level to debug
export RUST_LOG=debug

# Run with backtrace on error
export RUST_BACKTRACE=1

# Run spammer
cargo run -p tempo-spammer --bin tempo-spammer
```

---

## Wallet Issues

### "Failed to parse private key"

**Symptoms:**
```
Error: Failed to parse private key
```

**Causes:**
- Invalid hex format
- Missing `0x` prefix
- Wrong password for encrypted wallet
- Corrupted wallet JSON file

**Solutions:**

1. **Verify password:**
```bash
# Check password is set
echo $WALLET_PASSWORD

# Test with debug binary
cargo run -p tempo-spammer --bin tempo-debug -- --task 999 --wallet 0
```

2. **Check wallet format:**
```bash
# View wallet file (don't share this!)
cat wallet-json/wallet_001.json | jq .

# Should show encrypted structure:
# {
#   "encrypted": {
#     "ciphertext": "...",
#     "iv": "...",
#     "salt": "...",
#     "tag": "..."
#   }
# }
```

3. **Regenerate wallets** (if corrupted):
```bash
# Backup existing
mv wallet-json wallet-json-backup

# Generate new wallets
# (Use your wallet generation script)
```

### "No wallets found"

**Symptoms:**
```
❌ No wallets found
```

**Causes:**
- `wallet-json/` directory missing
- No `.json` files in directory
- Wrong directory path

**Solutions:**

1. **Check directory:**
```bash
ls -la wallet-json/
```

2. **Verify config:**
```toml
# config/config.toml
private_key_file = "wallet-json"  # Should match directory name
```

3. **Create directory if missing:**
```bash
mkdir -p wallet-json
# Add wallet files...
```

### "Wallet index out of bounds"

**Symptoms:**
```
❌ Wallet 10 not found (have 5)
```

**Causes:**
- Requesting wallet index that doesn't exist
- Worker count > wallet count

**Solutions:**

1. **Check wallet count:**
```bash
ls wallet-json/*.json | wc -l
```

2. **Adjust worker count:**
```toml
# config/config.toml
worker_count = 5  # Should be <= wallet count
```

---

## Transaction Failures

### "nonce too low"

**Symptoms:**
```
Failed: nonce too low
```

**Causes:**
- Nonce cache out of sync with blockchain
- Transactions submitted outside the spammer
- Wallet used by multiple processes

**Solutions:**

1. **Automatic fix** (already implemented):
```rust
// The spammer automatically resets nonce cache on this error
// See client.rs: reset_nonce_cache()
```

2. **Manual reset** (if automatic fails):
```bash
# Restart the spammer - this clears all caches
cargo run -p tempo-spammer --bin tempo-spammer
```

3. **Wait and retry**:
```bash
# Wait for pending transactions to clear
echo "Waiting 30 seconds..."
sleep 30

# Retry
cargo run -p tempo-spammer --bin tempo-spammer
```

### "insufficient funds"

**Symptoms:**
```
Failed: insufficient funds for gas * price + value
```

**Causes:**
- Wallet has no native TEM tokens
- Gas price too high
- Trying to send more than balance

**Solutions:**

1. **Check balance:**
```bash
cargo run -p tempo-spammer --bin tempo-debug -- --task 999 --wallet 0
```

2. **Claim from faucet:**
```bash
# Run faucet task
cargo run -p tempo-spammer --bin tempo-debug -- --task 02_claim_faucet --wallet 0
```

3. **Reduce gas price:**
```toml
# config/config.toml
max_fee_per_gas = 100000000000  # Lower from default
priority_fee_per_gas = 1000000000
```

### "intrinsic gas too low"

**Symptoms:**
```
Failed: intrinsic gas too low
```

**Causes:**
- Gas limit too low for transaction type
- Contract deployment needs more gas

**Solutions:**

1. **Increase gas limit:**
```toml
# config/config.toml
default_gas_limit = 500000  # Increase from 21000
```

2. **Task-specific limits:**
```rust
// In task code, override gas limit:
let tx = TransactionRequest::default()
    .gas_limit(1000000);  // For deployments
```

### "replacement transaction underpriced"

**Symptoms:**
```
Failed: replacement transaction underpriced
```

**Causes:**
- Trying to replace pending transaction with same nonce
- New gas price not high enough (must be 10%+ higher)

**Solutions:**

1. **Wait for confirmation:**
```bash
# Wait for pending transaction
echo "Waiting 60 seconds..."
sleep 60
```

2. **Clear pending transactions** (advanced):
```bash
# Send 0 ETH to self with high gas to clear queue
# (Use debug task with manual nonce)
```

### Transaction reverts

**Symptoms:**
```
Failed: transaction reverted
```

**Causes:**
- Contract logic rejected the call
- Insufficient token balance
- Missing approval for token spend
- Task requirements not met

**Solutions:**

1. **Check task requirements:**
```bash
# Review task documentation
cat docs/TASK_CATALOG.md | grep -A 10 "Task 21"
```

2. **Run prerequisite tasks:**
```bash
# Example: Task 23 requires Task 21
cargo run -p tempo-spammer --bin tempo-debug -- --task 21_create_meme --wallet 0

# Then run dependent task
cargo run -p tempo-spammer --bin tempo-debug -- --task 23_transfer_meme --wallet 0
```

3. **Check token balances:**
```bash
# Run balance check
cargo run -p tempo-spammer --bin tempo-debug -- --task 999 --wallet 0
```

---

## Proxy Issues

### "Failed to create proxy"

**Symptoms:**
```
Error: Failed to create proxy
```

**Causes:**
- Invalid proxy URL format
- Missing protocol (http://)
- Authentication error

**Solutions:**

1. **Check proxy format:**
```bash
# config/proxies.txt
# Correct formats:
192.168.1.1:8080
192.168.1.1:8080:user:pass

# Incorrect:
192.168.1.1  # Missing port
http://192.168.1.1:8080  # Should not include protocol
```

2. **Test proxy manually:**
```bash
# Test with curl
curl -x http://user:pass@192.168.1.1:8080 \
  https://rpc.moderato.tempo.xyz \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

### "Proxy health check failed"

**Symptoms:**
```
🔍 Scanning 100 proxies (50 concurrent)...
Healthy: 95, Banned: 5
```

**Causes:**
- Proxy is down
- Network timeout
- Rate limiting

**Solutions:**

1. **Check banned proxies:**
```bash
# Run proxy debug binary
cargo run -p tempo-spammer --bin debug_proxy
```

2. **Increase ban duration:**
```rust
// In code, adjust ban duration
let banlist = ProxyBanlist::new(60); // 60 minutes
```

3. **Use direct connection:**
```bash
# Run without proxies
cargo run -p tempo-spammer --bin tempo-spammer -- --no-proxy
```

### "All proxies banned"

**Symptoms:**
```
⚠️  All proxies banned, using direct connection
```

**Causes:**
- All proxies failed health checks
- Network issues
- RPC endpoint blocking proxies

**Solutions:**

1. **Wait for recheck:**
```bash
# Proxies are rechecked every 30 minutes
# Or restart to force immediate recheck
```

2. **Check proxy list:**
```bash
# Verify proxies are working
head -5 config/proxies.txt
```

3. **Use direct connection temporarily:**
```bash
# Empty proxy file or use --no-proxy flag
```

---

## Database Issues

### "database is locked"

**Symptoms:**
```
Error: database is locked
```

**Causes:**
- Multiple processes accessing SQLite
- Previous process didn't exit cleanly
- Database timeout too short

**Solutions:**

1. **Check for zombie processes:**
```bash
# Find tempo-spammer processes
ps aux | grep tempo-spammer

# Kill if necessary
kill -9 <PID>
```

2. **Increase timeout:**
```rust
// In database initialization
sqlx::sqlite::SqliteConnectOptions::new()
    .busy_timeout(Duration::from_secs(30))
```

3. **Delete lock file** (if exists):
```bash
rm tempo-spammer.db-journal
rm tempo-spammer.db-wal
```

### "Failed to open database"

**Symptoms:**
```
⚠️  Failed to open database: SqliteError(...)
```

**Causes:**
- Database file corrupted
- Permission denied
- Disk full

**Solutions:**

1. **Check disk space:**
```bash
df -h
```

2. **Check permissions:**
```bash
ls -la tempo-spammer.db
chmod 644 tempo-spammer.db
```

3. **Recreate database:**
```bash
# Backup old data
mv tempo-spammer.db tempo-spammer.db.backup

# New database will be created automatically
```

### "table does not exist"

**Symptoms:**
```
Error: table 'task_metrics' does not exist
```

**Causes:**
- Database schema not initialized
- Migration needed

**Solutions:**

1. **Database auto-creates tables** - restart spammer

2. **Manual schema creation:**
```sql
-- Connect to database
sqlite3 tempo-spammer.db

-- Create tables
CREATE TABLE task_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    worker_id TEXT NOT NULL,
    wallet_address TEXT NOT NULL,
    task_name TEXT NOT NULL,
    status TEXT NOT NULL,
    message TEXT,
    duration_ms INTEGER,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

---

## Performance Issues

### Slow transaction throughput

**Symptoms:**
- TPS much lower than target
- High latency between transactions

**Causes:**
- Task interval too long
- Too few workers
- Proxy latency
- RPC rate limiting

**Solutions:**

1. **Reduce task interval:**
```toml
# config/config.toml
task_interval_min = 1  # Reduce from 5
task_interval_max = 3  # Reduce from 15
```

2. **Increase workers:**
```toml
worker_count = 10  # Increase from 1
```

3. **Check proxy latency:**
```bash
# Test proxy response time
cargo run -p tempo-spammer --bin debug_proxy
```

4. **Use faster RPC:**
```toml
# config/config.toml
rpc_url = "https://faster-rpc.tempo.xyz"
```

### High memory usage

**Symptoms:**
- Process using GBs of memory
- System swapping

**Causes:**
- Too many workers
- Connection pool too large
- Memory leak

**Solutions:**

1. **Reduce worker count:**
```toml
worker_count = 5  # Reduce from 50
```

2. **Limit proxy count:**
```rust
// In code, limit proxy pool
let pool = ClientPool::new(config_path, password, Some(10)).await?;
```

3. **Restart periodically:**
```bash
# Use systemd or cron to restart every hour
```

### CPU usage too high

**Symptoms:**
- 100% CPU usage
- System unresponsive

**Causes:**
- Too many concurrent tasks
- Tight loops without delays

**Solutions:**

1. **Add delays between tasks:**
```toml
task_interval_min = 10
task_interval_max = 20
```

2. **Reduce worker count:**
```toml
worker_count = 3
```

---

## Task-Specific Issues

### Task 21 (Create Meme) fails

**Symptoms:**
```
Failed: Insufficient PathUSD for meme creation
```

**Solution:**
```bash
# Claim PathUSD from faucet first
cargo run -p tempo-spammer --bin tempo-debug -- --task 02_claim_faucet --wallet 0

# Then create meme
cargo run -p tempo-spammer --bin tempo-debug -- --task 21_create_meme --wallet 0
```

### Task 23 (Transfer Meme) fails

**Symptoms:**
```
Failed: No meme tokens found for wallet
```

**Solution:**
```bash
# Create meme first (prerequisite)
cargo run -p tempo-spammer --bin tempo-debug -- --task 21_create_meme --wallet 0

# Then transfer
cargo run -p tempo-spammer --bin tempo-debug -- --task 23_transfer_meme --wallet 0
```

### Task 05 (Swap Stable) fails

**Symptoms:**
```
Failed: Approval failed
```

**Solution:**
```bash
# Check PathUSD balance
cargo run -p tempo-spammer --bin tempo-debug -- --task 999 --wallet 0

# Claim more if needed
cargo run -p tempo-spammer --bin tempo-debug -- --task 02_claim_faucet --wallet 0
```

---

## Getting Help

### Debug Mode

Run with maximum debugging:

```bash
# Set environment
export RUST_LOG=trace
export RUST_BACKTRACE=full

# Run specific task
cargo run -p tempo-spammer --bin tempo-debug -- --task 01_deploy_contract --wallet 0
```

### Check Logs

```bash
# View recent logs
tail -f logs/smart_main.log

# Search for errors
grep "Failed" logs/smart_main.log | tail -20

# Check specific worker
grep "WK:001" logs/smart_main.log
```

### Community Support

1. **Check existing issues:**
   - Search GitHub issues
   - Check this troubleshooting guide

2. **Create a bug report:**
   - Include error message
   - Include config (redact sensitive data)
   - Include steps to reproduce
   - Include logs (with `RUST_LOG=debug`)

3. **Emergency contacts:**
   - Critical issues: #dev-critical channel
   - General questions: #general channel

### Diagnostic Script

```bash
#!/bin/bash
# diagnostic.sh - Run this to gather diagnostic info

echo "=== Tempo-Spammer Diagnostics ==="
echo ""

echo "1. Environment:"
echo "WALLET_PASSWORD set: $([ -z "$WALLET_PASSWORD" ] && echo 'NO' || echo 'YES')"
echo "RUST_LOG: ${RUST_LOG:-not set}"
echo ""

echo "2. Files:"
echo "Config exists: $([ -f config/config.toml ] && echo 'YES' || echo 'NO')"
echo "Wallets count: $(ls wallet-json/*.json 2>/dev/null | wc -l)"
echo "Proxies count: $([ -f config/proxies.txt ] && wc -l < config/proxies.txt || echo '0')"
echo ""

echo "3. Build:"
cargo build -p tempo-spammer 2>&1 | tail -5
echo ""

echo "4. Test connection:"
curl -s -X POST https://rpc.moderato.tempo.xyz \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | jq .result
echo ""

echo "5. Recent errors:"
grep "Failed" logs/smart_main.log 2>/dev/null | tail -5 || echo "No logs found"
echo ""

echo "=== End Diagnostics ==="
```

---

## Common Error Codes

| Error | Meaning | Solution |
|-------|---------|----------|
| `-32000` | Server error | Check RPC endpoint |
| `nonce too low` | Nonce mismatch | Wait or reset cache |
| `insufficient funds` | No gas money | Claim from faucet |
| `replacement underpriced` | Gas too low | Wait or increase gas |
| `reverted` | Contract rejected | Check task requirements |
| `timeout` | Request timeout | Check network/proxy |

---

## Prevention Tips

1. **Always test single task first:**
   ```bash
   cargo run -p tempo-spammer --bin tempo-debug -- --task <ID> --wallet 0
   ```

2. **Start with 1 worker:**
   ```toml
   worker_count = 1
   ```

3. **Monitor logs:**
   ```bash
   tail -f logs/smart_main.log | grep "Failed"
   ```

4. **Check prerequisites:**
   - Review task dependencies in TASK_CATALOG.md
   - Run prerequisite tasks first

5. **Keep backups:**
   ```bash
   cp tempo-spammer.db tempo-spammer.db.backup.$(date +%Y%m%d)
   ```

---

**Last Updated:** 2024-01-30  
**Version:** 0.1.0
