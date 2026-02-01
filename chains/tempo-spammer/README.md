# tempo-spammer

Multi-worker transaction spammer for the Tempo blockchain using Alloy v1.0.

## Overview

This project provides a high-performance transaction spammer for the Tempo blockchain with:
- **Alloy v1.0** - Modern Ethereum library with 10x faster ABI encoding
- **TIP-20 Support** - Native support for Tempo's token standard
- **Fee AMM Integration** - Pay gas in stablecoins
- **Multi-Wallet** - Support for multiple wallets via core-logic
- **Proxy Support** - Optional proxy rotation

## Quick Start

### Prerequisites

- Rust 1.75+
- `cargo`
- Wallet JSON files in `wallet-json/` directory
- Wallet password via `WALLET_PASSWORD` env var

### Building

```bash
cargo build -p tempo-spammer
```

### Running

```bash
# Set wallet password
export WALLET_PASSWORD=your_password

# Run spammer with default config
cargo run -p tempo-spammer --bin tempo-spammer

# Run with specific workers
cargo run -p tempo-spammer --bin tempo-spammer -- spammer --workers 4

# Run a single task for testing
cargo run -p tempo-spammer --bin tempo-debug -- --task 01_deploy_contract

# List available tasks
cargo run -p tempo-spammer --bin tempo-spammer -- list
```

## Configuration

Edit `config/config.toml`:

```toml
# RPC Settings
rpc_url = "https://rpc.moderato.tempo.xyz"
chain_id = 42431

# Worker Settings
worker_count = 1

# Gas Settings
default_gas_limit = 500000
max_fee_per_gas = 150000000000
priority_fee_per_gas = 1500000000

# Task Settings
task_interval_min = 5
task_interval_max = 15
task_timeout = 60
```

## Available Tasks

Complete task catalog with 50+ implementations. See [docs/TASK_CATALOG.md](docs/TASK_CATALOG.md) for detailed documentation.

### Core Operations

| ID | Name | Description | Complexity |
|----|------|-------------|------------|
| 01 | `deploy_contract` | Deploy a minimal Counter contract | Low |
| 02 | `claim_faucet` | Claim tokens from Tempo testnet faucet | Low |
| 03 | `send_token` | Transfer TIP-20 system tokens | Medium |
| 09 | `transfer_token` | Transfer stablecoins | Low |
| 10 | `transfer_memo` | Transfer with memo message | Low |

### Token Creation & Management

| ID | Name | Description | Dependencies |
|----|------|-------------|--------------|
| 04 | `create_stable` | Deploy a new stablecoin token | None |
| 05 | `swap_stable` | Swap between stablecoins on Fee AMM | Task 04 |
| 06 | `add_liquidity` | Add liquidity to DEX | Task 04 |
| 07 | `mint_stable` | Mint additional stablecoins | Task 04 |
| 08 | `burn_stable` | Burn stablecoin tokens | Task 04 |
| 12 | `remove_liquidity` | Remove liquidity from DEX | Task 06 |
| 13 | `grant_role` | Grant admin roles | Task 04 |
| 21 | `create_meme` | Create meme token | None |
| 22 | `mint_meme` | Mint meme tokens | Task 21 |
| 23 | `transfer_meme` | Transfer meme tokens | Task 21 |

### Batch Operations

| ID | Name | Description | Gas |
|----|------|-------------|-----|
| 17 | `batch_eip7702` | EIP-7702 batch transactions | 400k |
| 24 | `batch_swap` | Batch token swaps | 600k |
| 25 | `batch_system_token` | Batch system token ops | 400k |
| 26 | `batch_stable_token` | Batch stablecoin ops | 400k |
| 27 | `batch_meme_token` | Batch meme token ops | 400k |
| 43 | `batch_mint_stable` | Batch mint stablecoins | 300k |
| 44 | `batch_mint_meme` | Batch mint meme tokens | 300k |

### Multi-Send Operations

| ID | Name | Description | Gas |
|----|------|-------------|-----|
| 28 | `multi_send_disperse` | Disperse ETH to multiple addresses | 500k |
| 29 | `multi_send_disperse_stable` | Disperse stablecoins | 500k |
| 30 | `multi_send_disperse_meme` | Disperse meme tokens | 500k |
| 31 | `multi_send_concurrent` | Concurrent ETH transfers | 600k |
| 32 | `multi_send_concurrent_stable` | Concurrent stable transfers | 600k |
| 33 | `multi_send_concurrent_meme` | Concurrent meme transfers | 600k |

### Advanced Features

| ID | Name | Description | Gas |
|----|------|-------------|-----|
| 11 | `limit_order` | Place limit orders on DEX | 250k |
| 14 | `nft_create_mint` | Create NFT collection + mint | 400k |
| 15 | `mint_domain` | Mint domain name NFT | 200k |
| 16 | `mint_random_nft` | Mint random NFT | 150k |
| 18 | `tip403_policies` | TIP-403 policy operations | 200k |
| 45 | `deploy_viral_faucet` | Deploy viral faucet contract | 600k |
| 46 | `claim_viral_faucet` | Claim from viral faucet | 100k |
| 47 | `deploy_viral_nft` | Deploy viral NFT contract | 600k |
| 48 | `mint_viral_nft` | Mint viral NFT | 200k |
| 49 | `time_bomb` | Time-locked transaction | 400k |
| 50 | `deploy_storm` | Deploy storm contract | 800k |

### Analytics & Utilities

| ID | Name | Description |
|----|------|-------------|
| 19 | `wallet_analytics` | Collect wallet metrics | Low |
| 20 | `wallet_activity` | Monitor wallet activity | Low |
| 999 | `check_native_balance` | Check TEM balance | None |

### Additional Tasks

| ID | Name | Description |
|----|------|-------------|
| 34 | `batch_send_transaction` | Batch ETH transfers |
| 35 | `batch_send_transaction_stable` | Batch stable transfers |
| 36 | `batch_send_transaction_meme` | Batch meme transfers |
| 37 | `transfer_later` | Schedule ETH transfer |
| 38 | `transfer_later_stable` | Schedule stable transfer |
| 39 | `transfer_later_meme` | Schedule meme transfer |
| 40 | `distribute_shares` | Distribute ETH shares |
| 41 | `distribute_shares_stable` | Distribute stable shares |
| 42 | `distribute_shares_meme` | Distribute meme shares |

## Project Structure

```
tempo-spammer/
├── Cargo.toml
├── config/
│   ├── config.toml         # Main configuration
│   ├── address.txt         # Recipient addresses (one per line)
│   └── proxies.txt         # Optional proxy list
├── docs/                   # Documentation
│   ├── TASK_CATALOG.md     # Complete task reference (50+ tasks)
│   ├── ARCHITECTURE.md     # System architecture
│   ├── CONFIG_REFERENCE.md # Configuration options
│   ├── TROUBLESHOOTING.md  # Common issues & solutions
│   ├── TESTING.md          # Testing guide
│   ├── TASK_DEVELOPMENT.md # Creating new tasks
│   ├── SECURITY.md         # Security practices
│   └── PERFORMANCE.md      # Performance tuning
├── src/
│   ├── lib.rs              # Library exports & crate docs
│   ├── client.rs           # TempoClient (Alloy-based)
│   ├── client_pool.rs      # Wallet leasing & rotation
│   ├── config.rs           # Config loader
│   ├── nonce_manager.rs    # Nonce caching
│   ├── proxy_health.rs     # Proxy health checking
│   └── tasks/              # 50+ task implementations
│       ├── mod.rs          # Task trait + utilities
│       ├── tempo_tokens.rs # Token utilities
│       ├── t01_deploy_contract.rs
│       ├── t02_claim_faucet.rs
│       ├── t03_send_token.rs
│       ├── t04_create_stable.rs
│       ├── t05_swap_stable.rs
│       ├── t06_add_liquidity.rs
│       ├── t07_mint_stable.rs
│       ├── t08_burn_stable.rs
│       ├── t09_transfer_token.rs
│       ├── t10_transfer_memo.rs
│       ├── t11_limit_order.rs
│       ├── t12_remove_liquidity.rs
│       ├── t13_grant_role.rs
│       ├── t14_nft_create_mint.rs
│       ├── t15_mint_domain.rs
│       ├── t16_mint_random_nft.rs
│       ├── t17_batch_eip7702.rs
│       ├── t18_tip403_policies.rs
│       ├── t19_wallet_analytics.rs
│       ├── t20_wallet_activity.rs
│       ├── t21_create_meme.rs
│       ├── t22_mint_meme.rs
│       ├── t23_transfer_meme.rs
│       ├── t24_batch_swap.rs
│       ├── t25_batch_system_token.rs
│       ├── t26_batch_stable_token.rs
│       ├── t27_batch_meme_token.rs
│       ├── t28_multi_send_disperse.rs
│       ├── t29_multi_send_disperse_stable.rs
│       ├── t30_multi_send_disperse_meme.rs
│       ├── t31_multi_send_concurrent.rs
│       ├── t32_multi_send_concurrent_stable.rs
│       ├── t33_multi_send_concurrent_meme.rs
│       ├── t34_batch_send_transaction.rs
│       ├── t35_batch_send_transaction_stable.rs
│       ├── t36_batch_send_transaction_meme.rs
│       ├── t37_transfer_later.rs
│       ├── t38_transfer_later_stable.rs
│       ├── t39_transfer_later_meme.rs
│       ├── t40_distribute_shares.rs
│       ├── t41_distribute_shares_stable.rs
│       ├── t42_distribute_shares_meme.rs
│       ├── t43_batch_mint_stable.rs
│       ├── t44_batch_mint_meme.rs
│       ├── t45_deploy_viral_faucet.rs
│       ├── t46_claim_viral_faucet.rs
│       ├── t47_deploy_viral_nft.rs
│       ├── t48_mint_viral_nft.rs
│       ├── t49_time_bomb.rs
│       └── t50_deploy_storm.rs
├── bin/
│   ├── tempo-spammer.rs    # Main multi-worker spammer
│   ├── tempo-debug.rs      # Single task tester
│   ├── tempo-runner.rs     # Sequential runner
│   ├── tempo-sequence.rs   # Sequence executor
│   └── debug_proxy.rs      # Proxy debugger
├── TODO.md                 # Development TODOs
├── CHANGELOG.md            # Version history
└── README.md
```

## Tempo Testnet

| Property | Value |
|----------|-------|
| Network Name | Tempo Testnet (Moderato) |
| Chain ID | 42431 |
| RPC URL | https://rpc.moderato.tempo.xyz |
| Block Explorer | https://explore.tempo.xyz |

## System Tokens (TIP-20)

| Name | Address |
|------|---------|
| PathUSD | 0x20C0000000000000000000000000000000000000 |
| AlphaUSD | 0x20c0000000000000000000000000000000000001 |
| BetaUSD | 0x20c0000000000000000000000000000000000002 |
| ThetaUSD | 0x20c0000000000000000000000000000000000003 |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `WALLET_PASSWORD` | Password for encrypted wallet JSON files |
| `RUST_LOG` | Log level (e.g., `info`, `debug`, `trace`) |

## Adding New Tasks

1. Create a new file in `src/tasks/` (e.g., `t06_my_task.rs`)
2. Implement the `TempoTask` trait:

```rust
use async_trait::async_trait;
use crate::tasks::{TaskContext, TempoTask, TaskResult};
use anyhow::Result;

#[derive(Debug, Clone, Default)]
pub struct MyTask;

#[async_trait]
impl TempoTask for MyTask {
    fn name(&self) -> &'static str {
        "06_my_task"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        // Your task logic here
        Ok(TaskResult {
            success: true,
            message: "Task completed".to_string(),
            tx_hash: None,
        })
    }
}
```

3. Export it in `src/tasks/mod.rs`:

```rust
pub mod t06_my_task;
```

4. Register it in the binary:

```rust
let tasks: Vec<Box<dyn TempoTask>> = vec![
    // ... existing tasks ...
    Box::new(t06_my_task::MyTask::new()),
];
```

## License

MIT
