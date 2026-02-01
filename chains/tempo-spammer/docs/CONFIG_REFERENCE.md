# Configuration Reference

Complete reference for all tempo-spammer configuration options.

## Table of Contents
- [Configuration File](#configuration-file)
- [Core Settings](#core-settings)
- [Network Settings](#network-settings)
- [Performance Settings](#performance-settings)
- [Gas Settings](#gas-settings)
- [Task Settings](#task-settings)
- [Proxy Configuration](#proxy-configuration)
- [Advanced Settings](#advanced-settings)
- [Environment Variables](#environment-variables)
- [Example Configurations](#example-configurations)
- [Validation](#validation)

---

## Configuration File

The tempo-spammer uses TOML format for configuration. Default location:

```
config/config.toml
```

### Loading Priority

1. Command line: `--config path/to/config.toml`
2. Environment: `TEMPO_CONFIG=path/to/config.toml`
3. Default: `config/config.toml`

---

## Core Settings

### `rpc_url`
- **Type:** `string`
- **Required:** Yes
- **Default:** None
- **Example:** `"https://rpc.moderato.tempo.xyz"`

The JSON-RPC endpoint for the Tempo blockchain.

**Notes:**
- Must be a valid HTTP(S) URL
- Should support Web3 JSON-RPC API
- Test connection before use

**Example:**
```toml
rpc_url = "https://rpc.moderato.tempo.xyz"
```

---

### `chain_id`
- **Type:** `u64`
- **Required:** Yes
- **Default:** `42431`
- **Example:** `42431`

The chain identifier for the network.

**Common Values:**
- `42431` - Tempo Testnet (Moderato)

**Example:**
```toml
chain_id = 42431
```

---

### `private_key_file`
- **Type:** `string`
- **Required:** Yes
- **Default:** `"wallet-json"`
- **Example:** `"wallet-json"`

Directory containing encrypted wallet JSON files.

**Structure:**
```
wallet-json/
├── wallet_001.json
├── wallet_002.json
└── ...
```

**Example:**
```toml
private_key_file = "wallet-json"
```

---

## Network Settings

### `worker_count`
- **Type:** `u64`
- **Required:** No
- **Default:** `1`
- **Range:** `1-100`
- **Example:** `10`

Number of concurrent worker threads.

**Guidelines:**
- Start with 1 for testing
- Increase gradually based on performance
- Should not exceed wallet count
- Consider proxy capacity

**Example:**
```toml
worker_count = 10
```

---

### `connection_timeout`
- **Type:** `u64` (seconds)
- **Required:** No
- **Default:** `30`
- **Range:** `5-300`
- **Example:** `30`

HTTP connection timeout for RPC requests.

**Example:**
```toml
connection_timeout = 30
```

---

### `request_timeout`
- **Type:** `u64` (seconds)
- **Required:** No
- **Default:** `30`
- **Range:** `5-300`
- **Example:** `30`

Total request timeout including retries.

**Example:**
```toml
request_timeout = 30
```

---

## Performance Settings

### `task_interval_min`
- **Type:** `u64` (milliseconds)
- **Required:** No
- **Default:** `5`
- **Range:** `0-10000`
- **Example:** `5`

Minimum delay between tasks per worker.

**Notes:**
- Lower = higher throughput, more load
- Higher = lower throughput, less load
- Actual delay is random between min and max

**Example:**
```toml
task_interval_min = 5
```

---

### `task_interval_max`
- **Type:** `u64` (milliseconds)
- **Required:** No
- **Default:** `15`
- **Range:** `0-60000`
- **Example:** `15`

Maximum delay between tasks per worker.

**Example:**
```toml
task_interval_max = 15
```

---

### `proxy_concurrent_limit`
- **Type:** `usize`
- **Required:** No
- **Default:** `50`
- **Range:** `1-1000`
- **Example:** `50`

Maximum concurrent proxy health checks.

**Notes:**
- Higher = faster scanning but more network load
- Lower = slower scanning but gentler on network

**Example:**
```toml
proxy_concurrent_limit = 50
```

---

### `proxy_recheck_interval`
- **Type:** `u64` (minutes)
- **Required:** No
- **Default:** `30`
- **Range:** `1-1440`
- **Example:** `30`

Interval for rechecking banned proxies.

**Example:**
```toml
proxy_recheck_interval = 30
```

---

## Gas Settings

### `default_gas_limit`
- **Type:** `u128`
- **Required:** No
- **Default:** `500000`
- **Range:** `21000-10000000`
- **Example:** `500000`

Default gas limit for transactions.

**Common Values:**
- `21000` - Simple transfer
- `500000` - Contract deployment
- `1000000` - Complex contract

**Example:**
```toml
default_gas_limit = 500000
```

---

### `max_fee_per_gas`
- **Type:** `u128`
- **Required:** No
- **Default:** `150000000000` (150 Gwei)
- **Example:** `150000000000`

Maximum fee per gas (EIP-1559).

**Notes:**
- In wei (1 Gwei = 1,000,000,000 wei)
- Caps total gas price
- Protects against overpaying

**Example:**
```toml
max_fee_per_gas = 150000000000
```

---

### `priority_fee_per_gas`
- **Type:** `u128`
- **Required:** No
- **Default:** `1500000000` (1.5 Gwei)
- **Example:** `1500000000`

Priority fee per gas (EIP-1559 tip).

**Notes:**
- In wei
- Higher = faster inclusion
- Lower = cheaper but slower

**Example:**
```toml
priority_fee_per_gas = 1500000000
```

---

### `gas_bump_percent`
- **Type:** `u64`
- **Required:** No
- **Default:** `20`
- **Range:** `0-100`
- **Example:** `20`

Percentage to bump gas price on retry.

**Example:**
```toml
gas_bump_percent = 20
```

---

## Task Settings

### `task_timeout`
- **Type:** `u64` (seconds)
- **Required:** No
- **Default:** `180`
- **Range:** `10-3600`
- **Example:** `180`

Maximum time allowed for a task to complete.

**Notes:**
- Tasks exceeding this are marked as failed
- Should account for network latency
- Complex tasks may need longer timeouts

**Example:**
```toml
task_timeout = 180
```

---

### `task_weights`
- **Type:** `map<string, u64>`
- **Required:** No
- **Default:** All tasks weight 10
- **Example:** See below

Weighted distribution for task selection.

**Format:**
```toml
[task_weights]
"01_deploy_contract" = 5
"02_claim_faucet" = 10
"03_send_token" = 20
```

**Notes:**
- Higher weight = more frequent execution
- Omitted tasks use default weight
- Total weight doesn't need to sum to 100

---

### `enabled_tasks`
- **Type:** `array<string>`
- **Required:** No
- **Default:** All tasks enabled
- **Example:** `["01_deploy_contract", "02_claim_faucet"]`

List of tasks to enable. If specified, only these tasks run.

**Example:**
```toml
enabled_tasks = ["01_deploy_contract", "02_claim_faucet", "03_send_token"]
```

---

### `disabled_tasks`
- **Type:** `array<string>`
- **Required:** No
- **Default:** None
- **Example:** `["50_deploy_storm"]`

List of tasks to disable.

**Example:**
```toml
disabled_tasks = ["50_deploy_storm", "49_time_bomb"]
```

---

## Proxy Configuration

### `proxies_file`
- **Type:** `string`
- **Required:** No
- **Default:** `"config/proxies.txt"`
- **Example:** `"config/proxies.txt"`

Path to proxy list file.

**Format:**
```
# Comment
ip:port
ip:port:username:password
```

**Example:**
```toml
proxies_file = "config/proxies.txt"
```

---

### `proxy_ban_duration`
- **Type:** `u64` (minutes)
- **Required:** No
- **Default:** `30`
- **Range:** `1-1440`
- **Example:** `30`

Duration to ban unhealthy proxies.

**Example:**
```toml
proxy_ban_duration = 30
```

---

### `proxy_max_failures`
- **Type:** `u64`
- **Required:** No
- **Default:** `3`
- **Range:** `1-10`
- **Example:** `3`

Maximum consecutive failures before banning.

**Example:**
```toml
proxy_max_failures = 3
```

---

## Advanced Settings

### `database_path`
- **Type:** `string`
- **Required:** No
- **Default:** `"tempo-spammer.db"`
- **Example:** `"tempo-spammer.db"`

Path to SQLite database file.

**Example:**
```toml
database_path = "tempo-spammer.db"
```

---

### `database_max_connections`
- **Type:** `u32`
- **Required:** No
- **Default:** `5`
- **Range:** `1-50`
- **Example:** `5`

Maximum database connection pool size.

**Example:**
```toml
database_max_connections = 5
```

---

### `log_level`
- **Type:** `string`
- **Required:** No
- **Default:** `"info"`
- **Options:** `"trace"`, `"debug"`, `"info"`, `"warn"`, `"error"`
- **Example:** `"info"`

Logging verbosity level.

**Example:**
```toml
log_level = "info"
```

---

### `log_file`
- **Type:** `string`
- **Required:** No
- **Default:** `"logs/smart_main.log"`
- **Example:** `"logs/smart_main.log"`

Path to log file.

**Example:**
```toml
log_file = "logs/smart_main.log"
```

---

### `nonce_cache_enabled`
- **Type:** `bool`
- **Required:** No
- **Default:** `true`
- **Example:** `true`

Enable nonce caching for performance.

**Example:**
```toml
nonce_cache_enabled = true
```

---

### `retry_attempts`
- **Type:** `u32`
- **Required:** No
- **Default:** `5`
- **Range:** `0-10`
- **Example:** `5`

Number of RPC retry attempts.

**Example:**
```toml
retry_attempts = 5
```

---

### `retry_backoff_ms`
- **Type:** `u64`
- **Required:** No
- **Default:** `100`
- **Range:** `10-10000`
- **Example:** `100`

Initial retry backoff in milliseconds.

**Example:**
```toml
retry_backoff_ms = 100
```

---

## Environment Variables

These environment variables override config file settings:

### `WALLET_PASSWORD`
- **Required:** Yes (if using encrypted wallets)
- **Example:** `export WALLET_PASSWORD="my_secure_password"`

Password for decrypting wallet JSON files.

---

### `RUST_LOG`
- **Required:** No
- **Default:** `"info"`
- **Example:** `export RUST_LOG=debug`

Logging level (overrides config file).

---

### `RUST_BACKTRACE`
- **Required:** No
- **Default:** `0`
- **Example:** `export RUST_BACKTRACE=1`

Enable stack traces on panic.

---

### `TEMPO_CONFIG`
- **Required:** No
- **Example:** `export TEMPO_CONFIG=/path/to/config.toml`

Override config file path.

---

### `DATABASE_URL`
- **Required:** No
- **Example:** `export DATABASE_URL="sqlite://tempo.db"`

Override database path.

---

## Example Configurations

### Minimal Config (Testing)

```toml
# config/config.toml
rpc_url = "https://rpc.moderato.tempo.xyz"
chain_id = 42431
private_key_file = "wallet-json"
worker_count = 1
```

---

### Standard Config (Production)

```toml
# config/config.toml

# Network
rpc_url = "https://rpc.moderato.tempo.xyz"
chain_id = 42431
private_key_file = "wallet-json"

# Workers
worker_count = 10

# Performance
task_interval_min = 5
task_interval_max = 15

# Gas
default_gas_limit = 500000
max_fee_per_gas = 150000000000
priority_fee_per_gas = 1500000000

# Tasks
task_timeout = 180

# Proxies
proxies_file = "config/proxies.txt"
proxy_ban_duration = 30

# Database
database_path = "tempo-spammer.db"

# Logging
log_level = "info"
log_file = "logs/smart_main.log"
```

---

### High-Performance Config

```toml
# config/config.toml

# Network
rpc_url = "https://fast-rpc.tempo.xyz"
chain_id = 42431
private_key_file = "wallet-json"

# Workers
worker_count = 50

# Performance
task_interval_min = 1
task_interval_max = 3

# Gas
default_gas_limit = 500000
max_fee_per_gas = 200000000000
priority_fee_per_gas = 2000000000
gas_bump_percent = 25

# Tasks
task_timeout = 120

# Task Weights - favor high-volume tasks
[task_weights]
"03_send_token" = 30
"21_create_meme" = 25
"23_transfer_meme" = 25
"01_deploy_contract" = 10
"02_claim_faucet" = 10

# Proxies
proxies_file = "config/proxies.txt"
proxy_concurrent_limit = 100
proxy_ban_duration = 15

# Database
database_path = "tempo-spammer.db"
database_max_connections = 10

# Advanced
nonce_cache_enabled = true
retry_attempts = 3
retry_backoff_ms = 50
```

---

### Development/Debug Config

```toml
# config/config.toml

# Network
rpc_url = "https://rpc.moderato.tempo.xyz"
chain_id = 42431
private_key_file = "wallet-json"

# Single worker for debugging
worker_count = 1

# Longer intervals for visibility
task_interval_min = 1000
task_interval_max = 2000

# Longer timeout for debugging
task_timeout = 300

# Only basic tasks
enabled_tasks = ["01_deploy_contract", "02_claim_faucet", "999_check_native_balance"]

# Verbose logging
log_level = "debug"
```

---

## Validation

### Check Config Syntax

```bash
# Use cargo to validate
cargo run -p tempo-spammer --bin tempo-spammer -- --config config/config.toml --dry-run
```

### Test Config Loading

```bash
# Run with specific config
cargo run -p tempo-spammer --bin tempo-debug -- --config config/config.toml --task 999
```

### Common Validation Errors

| Error | Cause | Solution |
|-------|-------|----------|
| `missing field rpc_url` | Required field missing | Add `rpc_url` to config |
| `invalid type` | Wrong data type | Check type (string vs number) |
| `unknown field` | Typo in field name | Check field name spelling |
| `invalid TOML` | Syntax error | Validate TOML syntax |

---

## Best Practices

1. **Start conservative:**
   - Begin with 1 worker
   - Gradually increase based on performance

2. **Monitor gas prices:**
   - Adjust `max_fee_per_gas` based on network conditions
   - Use lower fees during low congestion

3. **Test changes:**
   - Test config changes with single worker first
   - Use `tempo-debug` for testing

4. **Secure sensitive data:**
   - Never commit `WALLET_PASSWORD`
   - Use environment variables for secrets
   - Protect config files with appropriate permissions

5. **Backup config:**
   ```bash
   cp config/config.toml config/config.toml.backup
   ```

6. **Document changes:**
   - Comment non-obvious settings
   - Keep changelog of config changes

---

**Last Updated:** 2024-01-30  
**Version:** 0.1.0
