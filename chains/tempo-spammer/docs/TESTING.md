# Testing Guide

Comprehensive guide for testing the tempo-spammer.

## Table of Contents
- [Testing Overview](#testing-overview)
- [Unit Testing](#unit-testing)
- [Integration Testing](#integration-testing)
- [Task Testing](#task-testing)
- [Performance Testing](#performance-testing)
- [Proxy Testing](#proxy-testing)
- [Load Testing](#load-testing)
- [Test Data](#test-data)
- [Continuous Integration](#continuous-integration)

---

## Testing Overview

The tempo-spammer includes multiple testing approaches:

1. **Unit Tests** - Test individual functions and modules
2. **Integration Tests** - Test component interactions
3. **Task Tests** - Test individual task execution
4. **Performance Tests** - Benchmark throughput and latency
5. **Load Tests** - Test under high concurrency

### Test Environment Setup

```bash
# Create test directory
mkdir -p test-env
cd test-env

# Copy minimal config
cp ../config/config.toml ./

# Create test wallets (or use test wallet generator)
mkdir -p wallet-json
# ... generate test wallets

# Set test password
export WALLET_PASSWORD="test_password"

# Test RPC connection
curl -X POST https://rpc.moderato.tempo.xyz \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

---

## Unit Testing

### Running Unit Tests

```bash
# Run all unit tests
cargo test -p tempo-spammer

# Run specific module
cargo test -p tempo-spammer client_pool

# Run with output
cargo test -p tempo-spammer -- --nocapture

# Run ignored tests
cargo test -p tempo-spammer -- --ignored
```

### Writing Unit Tests

```rust
// src/client_pool.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_pool_creation() {
        let pool = ClientPool::new(
            "config/config.toml",
            Some("test".to_string()),
            None,
        ).await;
        
        assert!(pool.is_ok());
    }

    #[tokio::test]
    async fn test_wallet_leasing() {
        // Setup pool...
        
        // Acquire client
        let lease = pool.try_acquire_client().await;
        assert!(lease.is_some());
        
        // Verify client is locked
        let available = pool.available_count().await;
        assert_eq!(available, total - 1);
    }
}
```

### Test Utilities

```rust
// test_helpers.rs
pub fn create_test_config() -> TempoSpammerConfig {
    TempoSpammerConfig {
        rpc_url: "https://rpc.moderato.tempo.xyz".to_string(),
        chain_id: 42431,
        private_key_file: "test-wallets".to_string(),
        worker_count: 1,
        ..Default::default()
    }
}

pub async fn create_test_client() -> TempoClient {
    TempoClient::new(
        "https://rpc.moderato.tempo.xyz",
        "0x...", // test key
        None,
        None,
    ).await.unwrap()
}
```

---

## Integration Testing

### Testing Client Pool Integration

```bash
# Test with multiple workers
cargo run -p tempo-spammer --bin tempo-spammer -- \
  --config test-config.toml \
  --workers 5 \
  --duration 60
```

### Testing Database Integration

```bash
# Test database logging
export RUST_LOG=debug
cargo run -p tempo-spammer --bin tempo-spammer -- \
  --config test-config.toml \
  --workers 1 \
  --tasks 01_deploy_contract

# Verify database entries
sqlite3 tempo-spammer.db "SELECT * FROM task_metrics ORDER BY id DESC LIMIT 5;"
```

### Testing Proxy Integration

```bash
# Test with proxy list
cargo run -p tempo-spammer --bin debug_proxy -- \
  --proxies test-proxies.txt \
  --rpc https://rpc.moderato.tempo.xyz
```

---

## Task Testing

### Testing Individual Tasks

Use `tempo-debug` for testing single tasks:

```bash
# Basic syntax
cargo run -p tempo-spammer --bin tempo-debug \
  -- --config config/config.toml \
  --task <TASK_ID> \
  --wallet <WALLET_INDEX>

# Example: Test deploy contract
cargo run -p tempo-spammer --bin tempo-debug \
  -- --config config/config.toml \
  --task 01_deploy_contract \
  --wallet 0

# Example: Test with specific proxy
cargo run -p tempo-spammer --bin tempo-debug \
  -- --config config/config.toml \
  --task 03_send_token \
  --wallet 0 \
  --proxy 1
```

### Task Test Checklist

For each task, verify:

- [ ] Task executes without errors
- [ ] Success/failure properly reported
- [ ] Gas estimation is reasonable
- [ ] Database logging works (if applicable)
- [ ] Transaction is visible on explorer
- [ ] Task handles edge cases gracefully

### Testing Task Dependencies

```bash
# Task 23 requires Task 21

# First, verify Task 23 fails without Task 21
cargo run -p tempo-spammer --bin tempo-debug \
  -- --task 23_transfer_meme \
  --wallet 0
# Expected: "No meme tokens found"

# Run prerequisite
cargo run -p tempo-spammer --bin tempo-debug \
  -- --task 21_create_meme \
  --wallet 0

# Now Task 23 should succeed
cargo run -p tempo-spammer --bin tempo-debug \
  -- --task 23_transfer_meme \
  --wallet 0
```

### Batch Task Testing

```bash
# Test multiple tasks in sequence
for task in 01 02 03; do
  echo "Testing task $task..."
  cargo run -p tempo-spammer --bin tempo-debug \
    -- --task ${task} \
    --wallet 0
done
```

---

## Performance Testing

### Benchmarking TPS

```bash
#!/bin/bash
# benchmark.sh

DURATION=60
WORKERS=(1 5 10 20)

for w in "${WORKERS[@]}"; do
  echo "Testing with $w workers..."
  
  cargo run -p tempo-spammer --bin tempo-spammer \
    -- --config perf-config.toml \
    --workers $w \
    --duration $DURATION \
    2>&1 | tee "results_${w}workers.log"
  
  # Extract TPS from logs
  grep "TPS:" "results_${w}workers.log" | tail -1
done
```

### Measuring Latency

```bash
# Test latency with single worker
cargo run -p tempo-spammer --bin tempo-spammer \
  -- --config config.toml \
  --workers 1 \
  --duration 60 \
  --tasks 03_send_token

# Check logs for duration
grep "Duration:" logs/smart_main.log | awk '{sum+=$2; count++} END {print "Avg:", sum/count}'
```

### Gas Usage Analysis

```bash
# Run tasks and analyze gas usage
sqlite3 tempo-spammer.db <<EOF
SELECT 
  task_name,
  COUNT(*) as count,
  AVG(duration_ms) as avg_duration,
  SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) as successes
FROM task_metrics
GROUP BY task_name
ORDER BY avg_duration DESC;
EOF
```

---

## Proxy Testing

### Health Check Testing

```bash
# Test proxy health scanner
cargo run -p tempo-spammer --bin debug_proxy \
  -- --proxies config/proxies.txt \
  --rpc https://rpc.moderato.tempo.xyz \
  --concurrent 50

# Expected output:
# 🔍 Scanning 100 proxies (50 concurrent)...
# Healthy: 95, Banned: 5
```

### Proxy Rotation Testing

```bash
# Test with multiple proxies
for i in {1..10}; do
  cargo run -p tempo-spammer --bin tempo-debug \
    -- --task 01_deploy_contract \
    --wallet 0 \
    --proxy $i
done
```

### Failover Testing

```bash
# Test behavior when proxy fails
# 1. Start spammer with proxy
# 2. Block proxy IP (using iptables or firewall)
# 3. Verify spammer switches to backup proxy
```

---

## Load Testing

### Gradual Load Increase

```bash
#!/bin/bash
# load_test.sh

CONFIG="load-test-config.toml"

# Create config with increasing workers
cat > $CONFIG <<EOF
rpc_url = "https://rpc.moderato.tempo.xyz"
chain_id = 42431
private_key_file = "wallet-json"
task_interval_min = 1
task_interval_max = 2
EOF

# Test increasing load
for workers in 1 5 10 20 50; do
  echo "Testing with $workers workers..."
  
  timeout 120 cargo run -p tempo-spammer --bin tempo-spammer \
    -- --config $CONFIG \
    --workers $workers \
    2>&1 | grep -E "(Success|Failed|TPS)" | tail -5
  
  sleep 10  # Cooldown
done
```

### Stress Testing

```bash
# Maximum load test
cargo run -p tempo-spammer --bin tempo-spammer \
  -- --config stress-config.toml \
  --workers 100 \
  --duration 300 \
  --tasks 03_send_token

# Monitor system resources
htop  # or top
iostat -x 1
```

### Long-Running Test

```bash
# 24-hour stability test
cargo run -p tempo-spammer --bin tempo-spammer \
  -- --config production-config.toml \
  --workers 10 \
  --duration 86400 \
  2>&1 | tee long-running-test.log

# Monitor for memory leaks
grep "Memory" long-running-test.log
```

---

## Test Data

### Test Wallets

```bash
# Generate test wallets
mkdir -p test-wallets

for i in {1..10}; do
  # Generate wallet (use your wallet generator)
  ./generate-wallet.sh > "test-wallets/wallet_$(printf "%03d" $i).json"
done

# Verify wallets
ls test-wallets/*.json | wc -l
```

### Test Proxies

```bash
# Create test proxy list
cat > test-proxies.txt <<EOF
# Test proxies
127.0.0.1:8080
127.0.0.1:8081
127.0.0.1:8082
EOF

# Test with proxy
export http_proxy=http://127.0.0.1:8080
curl -I https://rpc.moderato.tempo.xyz
```

### Test Addresses

```bash
# Create test recipient addresses
cat > test-addresses.txt <<EOF
0x0000000000000000000000000000000000000001
0x0000000000000000000000000000000000000002
0x0000000000000000000000000000000000000003
EOF
```

---

## Continuous Integration

### GitHub Actions Workflow

```yaml
# .github/workflows/test.yml
name: Tests

on: [push, pull_request]

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          
      - name: Run unit tests
        run: cargo test -p tempo-spammer
        
  integration-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup test environment
        run: |
          mkdir -p test-wallets
          # Setup test wallets...
          
      - name: Run integration tests
        env:
          WALLET_PASSWORD: ${{ secrets.TEST_WALLET_PASSWORD }}
        run: |
          cargo run -p tempo-spammer --bin tempo-debug \
            -- --task 999 --wallet 0
```

### Pre-commit Hooks

```bash
# .git/hooks/pre-commit
#!/bin/bash

# Run tests before commit
cargo test -p tempo-spammer --quiet || exit 1

# Check formatting
cargo fmt -- --check || exit 1

# Run clippy
cargo clippy -p tempo-spammer -- -D warnings || exit 1
```

---

## Test Reports

### Generating Reports

```bash
# Run tests with JSON output
cargo test -p tempo-spammer --message-format=json > test-results.json

# Generate HTML report
cargo install cargo-tarpaulin
cargo tarpaulin -p tempo-spammer --out Html
```

### Coverage Analysis

```bash
# Install coverage tool
cargo install cargo-tarpaulin

# Run coverage
cargo tarpaulin -p tempo-spammer \
  --ignore-tests \
  --out Html \
  --output-dir coverage/

# View report
open coverage/tarpaulin-report.html
```

---

## Debugging Failed Tests

### Enable Debug Logging

```bash
export RUST_LOG=debug
export RUST_BACKTRACE=1
cargo test -p tempo-spammer -- --nocapture
```

### Test Isolation

```bash
# Run single test
cargo test -p tempo-spammer test_client_pool_creation -- --exact

# Run with timeout
timeout 30 cargo test -p tempo-spammer
```

### Common Test Failures

| Failure | Cause | Solution |
|---------|-------|----------|
| Timeout | Async deadlock | Check await points |
| Race condition | Shared state | Use proper synchronization |
| Network error | RPC unavailable | Mock network calls |
| Database locked | Concurrent access | Use test database |

---

## Best Practices

1. **Test Independence:**
   - Each test should be independent
   - Clean up resources after tests
   - Use temporary databases

2. **Test Coverage:**
   - Aim for >80% coverage
   - Test both success and failure paths
   - Test edge cases

3. **Performance Baselines:**
   - Record baseline metrics
   - Fail tests if performance degrades
   - Monitor trends over time

4. **Documentation:**
   - Document test setup
   - Include test data samples
   - Explain test scenarios

---

**Last Updated:** 2024-01-30  
**Version:** 0.1.0
