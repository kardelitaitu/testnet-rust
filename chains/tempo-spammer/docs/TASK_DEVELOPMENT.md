# Task Development Guide

Step-by-step guide for creating new tasks in the tempo-spammer.

## Table of Contents
- [Overview](#overview)
- [Prerequisites](#prerequisites)
- [Creating a Task](#creating-a-task)
- [Task Template](#task-template)
- [Common Patterns](#common-patterns)
- [Testing Your Task](#testing-your-task)
- [Registration](#registration)
- [Advanced Topics](#advanced-topics)
- [Examples](#examples)

---

## Overview

Tasks are the fundamental unit of work in the tempo-spammer. Each task:
- Implements the `TempoTask` trait
- Performs a specific blockchain operation
- Returns a `TaskResult` indicating success/failure
- Can be executed by spammer workers

### Task Categories

| Category | ID Range | Examples |
|----------|----------|----------|
| Core | 01-10 | Deploy, Faucet, Transfer |
| Token | 21-30 | Create, Mint, Swap |
| Batch | 24-27, 43-44 | Batch operations |
| Multi-Send | 28-33 | Disperse, Concurrent |
| Advanced | 45-50 | Viral, Storm |

---

## Prerequisites

### Knowledge Requirements

- **Rust**: Basic async/await, error handling
- **Ethereum**: Transactions, gas, nonces
- **Alloy**: Provider, transactions, encoding
- **Tempo**: System tokens, contracts

### Tools

```bash
# Ensure you have:
rustc --version  # 1.75+
cargo --version

# Clone and build
git clone https://github.com/your-org/tempo-spammer.git
cd tempo-spammer
cargo build -p tempo-spammer
```

### Environment Setup

```bash
# Set wallet password
export WALLET_PASSWORD="your_password"

# Create test config
cp chains/tempo-spammer/config/config.example.toml \
   chains/tempo-spammer/config/config.toml

# Test connection
cargo run -p tempo-spammer --bin tempo-debug -- --task 999
```

---

## Creating a Task

### Step 1: Choose Task ID

Check `docs/TASK_CATALOG.md` for available IDs:

```bash
# Find next available ID
grep "^| [0-9]" docs/TASK_CATALOG.md | tail -5
```

**Naming Convention:**
- Format: `tXX_description.rs`
- Example: `t51_my_new_task.rs`

### Step 2: Create Task File

```bash
# Create file
touch chains/tempo-spammer/src/tasks/t51_my_task.rs
```

### Step 3: Implement Task

See [Task Template](#task-template) below.

### Step 4: Export Task

Add to `src/tasks/mod.rs`:

```rust
pub mod t51_my_task;
pub use t51_my_task::MyTask;
```

### Step 5: Register Task

Add to binary files (e.g., `bin/tempo-debug.rs`):

```rust
(
    51,
    "51_my_task",
    "My New Task",
    Box::new(tempo_spammer::tasks::t51_my_task::MyTask::new()),
),
```

### Step 6: Test

```bash
# Test single execution
cargo run -p tempo-spammer --bin tempo-debug \
  -- --task 51_my_task --wallet 0

# Check it works
cargo run -p tempo-spammer --bin tempo-spammer \
  -- --workers 1 --duration 60
```

### Step 7: Document

Update `docs/TASK_CATALOG.md` with task details.

---

## Task Template

Complete template for a new task:

```rust
//! My New Task
//!
//! Brief description of what this task does.
//!
//! ## Workflow:
//! 1. Step one
//! 2. Step two
//! 3. Step three
//!
//! ## Success Criteria:
//! ✅ Criterion one
//! ✅ Criterion two

use crate::tasks::prelude::*;
use alloy::rpc::types::TransactionRequest;
use anyhow::{Context, Result};
use async_trait::async_trait;

/// My new task implementation
#[derive(Debug, Clone, Default)]
pub struct MyTask;

impl MyTask {
    /// Creates a new instance of the task
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for MyTask {
    fn name(&self) -> &'static str {
        "51_my_task"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        // 1. Pre-checks
        // Check balances, prerequisites, etc.

        // 2. Build transaction
        let tx = TransactionRequest::default()
            .from(address)
            // ... add transaction details
            ;

        // 3. Send transaction
        let pending = client
            .provider
            .send_transaction(tx)
            .await
            .context("Failed to send transaction")?;

        let tx_hash = pending.tx_hash().clone();

        // 4. Optional: Wait for confirmation
        // let receipt = pending.get_receipt().await?;

        // 5. Log to database (optional)
        if let Some(db) = &ctx.db {
            // db.log_something(...).await?;
        }

        // 6. Return result
        Ok(TaskResult {
            success: true,
            message: format!("Task completed: {:?}", tx_hash),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
```

---

## Common Patterns

### Pattern 1: Simple Transfer

```rust
async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
    let client = &ctx.client;
    let address = ctx.address();

    // Get recipient
    let recipient = get_random_address()?;

    // Build transfer
    let tx = TransactionRequest::default()
        .to(recipient)
        .value(U256::from(1000000000000000u64)) // 0.001 ETH
        .from(address);

    // Send
    let pending = client.provider.send_transaction(tx).await?;
    let tx_hash = pending.tx_hash();

    Ok(TaskResult {
        success: true,
        message: format!("Sent to {:?}", recipient),
        tx_hash: Some(format!("{:?}", tx_hash)),
    })
}
```

### Pattern 2: Token Transfer

```rust
async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
    let client = &ctx.client;
    let address = ctx.address();

    // Token address
    let token: Address = "0x20C0000000000000000000000000000000000000".parse()?;

    // Build transfer call data
    // transfer(address,uint256) selector: 0xa9059cbb
    let mut calldata = hex::decode("a9059cbb000000000000000000000000")?;
    calldata.extend_from_slice(recipient.as_slice());
    calldata.extend_from_slice(&amount.to_be_bytes::<32>());

    let tx = TransactionRequest::default()
        .to(token)
        .input(calldata.into())
        .from(address);

    let pending = client.provider.send_transaction(tx).await?;

    Ok(TaskResult {
        success: true,
        message: "Token transferred".to_string(),
        tx_hash: Some(format!("{:?}", pending.tx_hash())),
    })
}
```

### Pattern 3: Contract Deployment

```rust
async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
    let client = &ctx.client;

    // Contract bytecode
    let bytecode = hex::decode("6080604052...")?;

    let mut tx = TransactionRequest::default()
        .input(bytecode.into())
        .from(ctx.address())
        .gas_limit(500_000);
    
    tx.to = Some(alloy::primitives::TxKind::Create);

    let pending = client.provider.send_transaction(tx).await?;
    let receipt = pending.get_receipt().await?;

    // Get contract address from receipt
    let contract_address = receipt.contract_address
        .ok_or_else(|| anyhow!("Deployment failed"))?;

    // Log to database
    if let Some(db) = &ctx.db {
        db.log_counter_contract_creation(
            &ctx.address().to_string(),
            &contract_address.to_string(),
            ctx.chain_id(),
        ).await?;
    }

    Ok(TaskResult {
        success: true,
        message: format!("Deployed at {:?}", contract_address),
        tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
    })
}
```

### Pattern 4: Contract Interaction

```rust
async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
    let client = &ctx.client;

    // Contract address
    let contract: Address = "0x...".parse()?;

    // Build call data
    // increment() selector: 0xd09de08a
    let calldata = hex::decode("d09de08a")?;

    let tx = TransactionRequest::default()
        .to(contract)
        .input(calldata.into())
        .from(ctx.address());

    let pending = client.provider.send_transaction(tx).await?;
    let receipt = pending.get_receipt().await?;

    if !receipt.inner.status() {
        return Ok(TaskResult {
            success: false,
            message: "Transaction reverted".to_string(),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        });
    }

    Ok(TaskResult {
        success: true,
        message: "Contract called successfully".to_string(),
        tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
    })
}
```

### Pattern 5: Database Integration

```rust
async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
    // ... perform operation ...

    // Log to database if available
    if let Some(db) = &ctx.db {
        match db.log_task_result(
            TaskResult {
                success: true,
                message: "Done".to_string(),
                tx_hash: Some("0x...".to_string()),
            },
            self.name(),
            &ctx.address().to_string(),
            "001",
            1000, // duration_ms
        ).await {
            Ok(_) => tracing::debug!("Logged to database"),
            Err(e) => tracing::warn!("Failed to log: {}", e),
        }
    }

    Ok(TaskResult {
        success: true,
        message: "Completed".to_string(),
        tx_hash: Some("0x...".to_string()),
    })
}
```

### Pattern 6: Error Handling

```rust
async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
    let client = &ctx.client;

    // Try operation with fallback
    let result = match do_operation(client).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Operation failed: {}, trying fallback", e);
            
            // Try fallback
            match do_fallback(client).await {
                Ok(r) => r,
                Err(e) => {
                    // Return graceful failure
                    return Ok(TaskResult {
                        success: false,
                        message: format!("Failed: {}", e),
                        tx_hash: None,
                    });
                }
            }
        }
    };

    Ok(TaskResult {
        success: true,
        message: "Success".to_string(),
        tx_hash: Some(result),
    })
}
```

---

## Testing Your Task

### Unit Test

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_my_task() {
        let task = MyTask::new();
        let ctx = create_test_context().await;

        let result = task.run(&ctx).await;

        assert!(result.is_ok());
        let task_result = result.unwrap();
        assert!(task_result.success);
    }
}
```

### Integration Test

```bash
# Test with tempo-debug
cargo run -p tempo-spammer --bin tempo-debug \
  -- --task 51_my_task --wallet 0

# Check logs
tail -f logs/smart_main.log | grep "51_my_task"

# Verify on explorer
# Check transaction hash on https://explore.tempo.xyz
```

### Load Test

```bash
# Test with spammer
cargo run -p tempo-spammer --bin tempo-spammer \
  -- --workers 5 --duration 60

# Monitor success rate
grep "51_my_task" logs/smart_main.log | \
  grep -c "Success"
```

---

## Registration

### Register in tempo-debug

Edit `bin/tempo-debug.rs`:

```rust
let tasks: Vec<(usize, &str, &str, Box<dyn TempoTask>)> = vec![
    // ... existing tasks ...
    (
        51,
        "51_my_task",
        "My New Task",
        Box::new(tempo_spammer::tasks::t51_my_task::MyTask::new()),
    ),
];
```

### Register in tempo-spammer

Edit `bin/tempo-spammer.rs`:

```rust
let weighted_tasks = vec![
    // ... existing tasks ...
    (51, 10), // (task_id, weight)
];
```

### Register in Other Binaries

Repeat for:
- `tempo-runner.rs`
- `tempo-sequence.rs`

---

## Advanced Topics

### Using Alloy Sol Types

```rust
use alloy_sol_types::{sol, SolCall};

sol! {
    interface IERC20 {
        function transfer(address to, uint256 amount) external returns (bool);
        function balanceOf(address account) external view returns (uint256);
    }
}

// Build call
let call = IERC20::transferCall {
    to: recipient,
    amount: U256::from(1000),
};
let calldata = call.abi_encode();
```

### Batch Operations

```rust
async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
    let client = &ctx.client;
    let mut tx_hashes = Vec::new();

    // Send multiple transactions
    for i in 0..5 {
        let tx = build_transaction(i).await?;
        let pending = client.provider.send_transaction(tx).await?;
        tx_hashes.push(pending.tx_hash().clone());
    }

    Ok(TaskResult {
        success: true,
        message: format!("Sent {} transactions", tx_hashes.len()),
        tx_hash: Some(format!("First: {:?}", tx_hashes[0])),
    })
}
```

### Random Data Generation

```rust
use rand::Rng;

fn generate_random_amount() -> U256 {
    let mut rng = rand::thread_rng();
    let amount = rng.gen_range(1000..10000);
    U256::from(amount)
}

fn generate_random_address() -> Address {
    let mut rng = rand::rngs::OsRng;
    let bytes: [u8; 20] = rng.gen();
    Address::from_slice(&bytes)
}
```

### Gas Estimation

```rust
async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
    let client = &ctx.client;

    // Estimate gas
    let gas_price = ctx.gas_manager.estimate_gas(client).await?;
    let bumped_gas = ctx.gas_manager.bump_fees(gas_price, 20);

    let tx = TransactionRequest::default()
        .gas_price(bumped_gas)
        // ...
        ;

    // ...
}
```

---

## Examples

### Example 1: Simple Task

```rust
//! Simple Transfer Task
//!
//! Sends a small amount of TEM to a random address.

use crate::tasks::prelude::*;
use alloy::rpc::types::TransactionRequest;
use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct SimpleTransferTask;

impl SimpleTransferTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for SimpleTransferTask {
    fn name(&self) -> &'static str {
        "51_simple_transfer"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        // Generate random recipient
        let recipient = get_random_address()?;

        // Send 0.001 TEM
        let tx = TransactionRequest::default()
            .to(recipient)
            .value(U256::from(1000000000000000u64))
            .from(address);

        let pending = client.provider.send_transaction(tx).await?;
        let tx_hash = pending.tx_hash();

        Ok(TaskResult {
            success: true,
            message: format!("Sent 0.001 TEM to {:?}", recipient),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
```

### Example 2: Token Task

```rust
//! Token Approval Task
//!
//! Approves a spender for token transfers.

use crate::tasks::prelude::*;
use crate::tasks::tempo_tokens::TempoTokens;
use alloy::rpc::types::TransactionRequest;
use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct ApproveTokenTask;

impl ApproveTokenTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for ApproveTokenTask {
    fn name(&self) -> &'static str {
        "52_approve_token"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        // Get random token
        let token = TempoTokens::get_random_system_token();

        // Spender (e.g., DEX)
        let spender: Address = "0xdec0000000000000000000000000000000000000".parse()?;

        // Amount (max)
        let amount = U256::MAX;

        // Build approve call: approve(address,uint256)
        let mut calldata = hex::decode("095ea7b3").unwrap();
        calldata.extend_from_slice(&[0u8; 12]);
        calldata.extend_from_slice(spender.as_slice());
        calldata.extend_from_slice(&amount.to_be_bytes::<32>());

        let tx = TransactionRequest::default()
            .to(token.address)
            .input(calldata.into())
            .from(address);

        let pending = client.provider.send_transaction(tx).await?;

        Ok(TaskResult {
            success: true,
            message: format!("Approved {} for {}", token.symbol, spender),
            tx_hash: Some(format!("{:?}", pending.tx_hash())),
        })
    }
}
```

---

## Checklist

Before submitting your task:

- [ ] Task ID is unique
- [ ] File follows naming convention (`tXX_name.rs`)
- [ ] Implements `TempoTask` trait
- [ ] Has proper documentation
- [ ] Includes error handling
- [ ] Tested with `tempo-debug`
- [ ] Registered in all binaries
- [ ] Updated `TASK_CATALOG.md`
- [ ] No compiler warnings
- [ ] Code is formatted (`cargo fmt`)

---

## Getting Help

- Read [ARCHITECTURE.md](../docs/ARCHITECTURE.md)
- Check existing tasks in `src/tasks/`
- Ask in Discord/GitHub Discussions
- Review [TROUBLESHOOTING.md](../docs/TROUBLESHOOTING.md)

---

Happy task development! 🚀

**Last Updated:** 2024-01-30  
**Version:** 0.1.0
