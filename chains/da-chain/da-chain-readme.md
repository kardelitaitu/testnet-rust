# DA-Chain Spammer Documentation

## Overview
DA-Chain spammer is a Rust-based transaction testing tool for the DA-CHAIN network (DACC token, Chain ID 21894). It supports automated gas pricing (EIP-1559), pending transaction replacement, and configurable delays between tasks.

## Quick Start

### Prerequisites
- Rust toolchain installed
- Wallet files in `chains/da-chain/wallets-json-da-chain/`
- Set `WALLET_PASSWORD` environment variable

### Build
```bash
# Debug build
cargo build -p da-chain-project

# Release build (recommended for production)
cargo build --release -p da-chain-project
```

## Usage

### 1. Debug Single Task
Debug a specific task with a specific wallet:

**PowerShell:**
```powershell
$env:WALLET_PASSWORD="password"
cargo run -p da-chain-project --bin da-chain-debug_task -- --config chains/da-chain/config.toml --task 1 --wallet 1
```

**Command Prompt / .bat file:**
```batch
@echo off
set WALLET_PASSWORD=password
cargo run -p da-chain-project --bin da-chain-debug_task -- --config chains/da-chain/config.toml --task 1 --wallet 1
```

**Available Tasks:**
| Task ID | Name | Description |
|---------|------|-------------|
| 1 | checkBalance | Check wallet balance and display gas fees |
| 2 | simpleNativeTransfer | Send 0.5%-1.0% of balance to random recipient |

### 2. Run Spammer (Production)
Run the spammer with multiple workers:

**PowerShell:**
```powershell
$env:WALLET_PASSWORD="password"
cargo run --release -p da-chain-project -- --config chains/da-chain/config.toml --workers 1 --max-tps 5
```

To force a higher fee floor on noisy networks:

```powershell
cargo run --release -p da-chain-project -- --config chains/da-chain/config.toml --workers 1 --max-tps 5 --min-gwei 700
```

**Command Prompt / .bat file (`_start-da-chain.bat`):**
```batch
@echo off
set WALLET_PASSWORD=password
cargo run --release -p da-chain-project -- --config chains/da-chain/config.toml --workers 1 --max-tps 5
```

**CLI Arguments:**
| Argument | Description | Default |
|----------|-------------|---------|
| `--config <path>` | Path to config file | `chains/da-chain/config.toml` |
| `--workers N` | Number of workers (overrides interactive prompt) | Interactive prompt |
| `--max-tps N` | Max transactions per second per proxy | 10 |
| `--min-gwei N` | Minimum max-fee floor in gwei | 700 |
| `--no-proxy` | Disable proxy usage | false |
| `--export-metrics <path>` | Export metrics to file | None |
| `--metrics-interval N` | Metrics export interval in seconds | 30 |

## Configuration

Edit `chains/da-chain/config.toml`:

```toml
rpc_url = "https://rpctest.dachain.tech"
chain_id = 21894
explorer = "https://dachain.tech"
symbol = "DACC"
tps = 10
worker_amount = 5

# Custom wallet directory
wallet_dir = "chains/da-chain/wallets-json-da-chain"

# Random delay between tasks (in milliseconds)
# For da-chain: 1-2 minutes (60000-120000 ms)
min_delay_ms = 60000
max_delay_ms = 120000

# Optional: Create2 factory address
# create2_factory = "0x..."

# Optional: Proxies (or use proxies.txt in root)
# [[proxies]]
# url = "http://proxy:port"
# username = "user"
# password = "pass"
```

**Delay Settings:**
- `min_delay_ms` / `max_delay_ms`: Random delay between tasks (overrides TPS-based delay)
- Without delay settings: delay = `1000 / tps` milliseconds

## Wallet Setup

### Directory Structure
```
chains/da-chain/
├── wallets-json-da-chain/
│   ├── 0x1234...abcd.json
│   ├── 0x5678...ef01.json
│   └── ...
└── config.toml
```

### Wallet File Format (encrypted JSON)
```json
{
  "address": "0x...",
  "crypto": { ... },
  "id": "...",
  "version": 3
}
```

### Decryption
- Set environment variable: `WALLET_PASSWORD=your_password`
- Or enter interactively when prompted

## Task Details

### Task 01: checkBalance
- Displays wallet address and balance
- Shows confirmed and pending nonce
- Displays current gas fees (max fee and priority fee in Gwei)
- No transactions sent

**Output example:**
```
Balance: 9.995877521728202041 DACC | Gas: 5670.35 Gwei (max), 46.15 Gwei (priority)
```

### Task 02: simpleNativeTransfer
- Sends **0.5% - 1.0%** of wallet balance (minimum: **0.0001 DACC**)
- Random recipient from `chains/da-chain/address.txt`
- Automatically detects and replaces pending transactions
- Supports EIP-1559 (dynamic gas) and legacy chains
- Increases gas by 10% when replacing stuck transactions

**Features:**
- ✅ Pending transaction detection (scans for stuck nonces)
- ✅ Transaction replacement (reuses nonce with higher gas)
- ✅ Random recipient selection from `address.txt`
- ✅ Automatic EIP-1559 / legacy chain detection
- ✅ Percentage-based transfer amount

## Gas Fee Configuration

### Automatic Gas Pricing
- **EIP-1559 chains**: Uses `max_fee_per_gas` and `max_priority_fee_per_gas`
- **Legacy chains**: Uses `gas_price`
- Default max fee cap: **20000 Gwei**
- Default priority fee: **10 Gwei**

### Gas Manager (in code)
```rust
// chains/da-chain/src/utils/gas.rs
pub const MAX_FEE_GWEI_DEFAULT: f64 = 20000.0;
pub const PRIORITY_FEE_GWEI_DEFAULT: f64 = 10.0;
```

## Debug Output

Both tasks print debug information to help trace execution:

```
[DEBUG] Checking balance for: 0x...
[DEBUG] Confirmed nonce: 4
[DEBUG] Pending nonce: 5
[WARNING] Pending transactions detected (1 pending)
[DEBUG] Balance: 9.99 DACC
[DEBUG] Gas fees - max: 5670.35 Gwei, priority: 46.15 Gwei
```

## File Structure
```
chains/da-chain/
├── src/
│   ├── main.rs                    # Entry point, CLI args, worker setup
│   ├── bin/
│   │   └── debug_task.rs         # Task debugger with nonce checking
│   ├── task/
│   │   ├── mod.rs                # TaskContext, TaskResult, DaChainTask trait
│   │   ├── t01_check_balance.rs  # Balance check + gas fee display
│   │   └── t02_simple_native_transfer.rs  # Native transfer with pending tx handling
│   ├── utils/
│   │   └── gas.rs               # GasManager with EIP-1559 support
│   └── config.rs                 # DaChainConfig struct
├── config.toml                    # Runtime configuration
├── address.txt                    # Recipients for task 02 (one address per line)
├── wallets-json-da-chain/        # Encrypted wallet files
└── da-chain-readme.md            # This file
```

## Troubleshooting

### "replacement transaction underpriced"
**Cause:** Stuck pending transaction with low gas price.

**Solution:** The spammer automatically detects and replaces pending transactions with 10% higher gas. Wait 1-2 minutes for the replacement to confirm.

### "Insufficient balance"
**Cause:** Wallet doesn't have enough DACC for transfer + gas.

**Solution:** Ensure wallet has > 0.0001 DACC + gas fees (typically ~0.1 DACC for simple transfer).

### Wallet decryption failed
**Cause:** Wrong `WALLET_PASSWORD`.

**Solution:**
```powershell
$env:WALLET_PASSWORD="correct_password"
```

### No proxies found
**Cause:** `proxies.txt` missing or empty.

**Solution:** Create `proxies.txt` in project root:
```
http://proxy1:port
http://user:pass@proxy2:port
```

## Development

### Running Tests
```bash
cargo test -p da-chain-project
```

### Code Formatting
```bash
cargo fmt -p da-chain-project
cargo check -p da-chain-project
```

### Commit Guidelines
- Use conventional commits: `feat(da-chain): ...`, `fix(da-chain): ...`
- Test with debug_task before committing
- Run `cargo fmt` before committing
