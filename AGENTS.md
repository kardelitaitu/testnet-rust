# AI/Agent Instructions for testnet-framework

This document provides guidance for AI assistants working on the Rust Multi-Chain Testnet Framework codebase. It supplements CODEBASE.md with actionable instructions for common operations.

## 1. Quick Reference

### Essential Commands

```powershell
# Build the entire project
._clean_and_compile_all.bat

# Run debugger to check all wallet balances
$env:WALLET_PASSWORD="password"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml --all

# Run the RISE spammer
$env:WALLET_PASSWORD="password"; .\target_final\debug\rise-project.exe --config chains/risechain/config.toml

# Check code without building
cargo check --workspace

# Format code
cargo fmt
```

### First 3 Things to Check When Debugging

1. **Wallet Password**: Verify `WALLET_PASSWORD` environment variable is set correctly
2. **Config Path**: Ensure `--config` points to the correct TOML file
3. **Database Lock**: Check if `rise.db` is being used by another process

### Common File Locations

| Operation | File Path |
|-----------|-----------|
| Add new task | `chains/risechain/src/task/new_task_name.rs` |
| Modify config | `chains/risechain/src/config.rs` |
| Modify spammer | `chains/risechain/src/spammer/mod.rs` |
| Add new chain | Copy `chains/evm-project/` to new directory |
| Core library | `core-logic/src/` |
| View logs | `logs/smart_main.log` |

---

## 2. Code Conventions

### 2.1 Rust Edition and Style

- **Edition**: Rust 2021 (default for recent stable)
- **Formatting**: Run `cargo fmt` before committing
- **Linting**: Run `cargo clippy` and address warnings
- **MSRV**: Minimum Supported Rust Version is the latest stable

### 2.2 Module Imports

Always use the full module path for clarity:

```rust
// Good
use core_logic::utils::wallet_manager::WalletManager;
use core_logic::traits::Spammer;
use core_logic::config::SpamConfig;

// Avoid
use crate::wallet_manager::*;  // Wildcard imports
```

### 2.3 Error Handling

- Use `anyhow` for application-level error handling
- Use `thiserror` for library-style error types
- Propagate errors with `?` operator
- Add context with `.context()` or `.with_context(|| ...)`

```rust
use anyhow::{Result, Context, anyhow};

// Good
fn load_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .context(format!("Failed to read config from {}", path))?;
    
    toml::from_str(&content)
        .context("Failed to parse config TOML")
}

// Function that can fail
async fn send_transaction(&self) -> Result<TxHash> {
    let receipt = self.provider.send_transaction(&tx, None)
        .await
        .context("Failed to send transaction")?
        .confirmations(1)
        .await
        .context("Failed to confirm transaction")?;
    
    receipt.transaction_hash
        .ok_or_else(|| anyhow!("Transaction dropped from mempool"))
}
```

### 2.4 Async Patterns

- Use `async_trait` for trait definitions that need async methods
- Use `tokio::select!` for cancellation-safe sleeps
- Use `CancellationToken` from `tokio_util::sync` for graceful shutdown
- Avoid blocking calls in async contexts

```rust
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tokio::time::{sleep, Duration};

#[async_trait]
impl Spammer for EvmSpammer {
    async fn start(&self, token: CancellationToken) -> Result<SpammerStats> {
        loop {
            // Check cancellation
            if token.is_cancelled() {
                break;
            }
            
            // Do work
            let result = self.execute_task().await?;
            
            // Sleep with cancellation check
            tokio::select! {
                _ = sleep(Duration::from_millis(1000)) => {}
                _ = token.cancelled() => break,
            }
        }
        Ok(stats)
    }
}
```

### 2.5 Logging Conventions

- Use structured logging with `tracing` crate
- Include context (worker_id, wallet_id, proxy_id) in spans
- Use appropriate log levels (error, warn, info, debug, trace)
- Log at INFO level for major operations, DEBUG for details

```rust
use tracing::{info, warn, error, debug, span, Level};

// Create span with context
let span = span!(Level::INFO, "task", worker_id = %self.wallet_id, task = %task_name);
let _guard = span.enter();

// Log with context
info!(target: "smart_main", "Starting task {}", task_name);
debug!("Transaction hash: {:?}", tx_hash);
warn!("Retrying after error: {}", error);
error!("Task failed permanently: {}", error);
```

---

## 3. Adding New Tasks (Step-by-Step)

### Step 1: Create Task File

Create a new file in `chains/risechain/src/task/`:

**File: `chains/risechain/src/task/my_new_task.rs`**

```rust
use async_trait::async_trait;
use core_logic::traits::{Task, TaskResult};
use crate::task::TaskContext;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct MyNewTask;

impl MyNewTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for MyNewTask {
    fn name(&self) -> &str {
        "MyNewTask"  // Unique task name for logging
    }
    
    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        // 1. Get necessary components from context
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        
        // 2. Build transaction or query
        // Example: Simple ETH balance check
        
        // 3. Execute with provider/wallet
        let balance = provider.get_balance(wallet.address(), None).await?;
        
        // 4. Format result
        let message = format!("My new task completed. Balance: {:?}", balance);
        
        // 5. Return TaskResult
        Ok(TaskResult {
            success: true,
            message,
            tx_hash: None,  // Set Some(hash) if transaction was sent
        })
    }
}
```

### Step 2: Export Task in `task/mod.rs`

Add to the module exports:

```rust
// In chains/risechain/src/task/mod.rs

pub mod check_balance;
pub mod claim_faucet;
pub mod create_meme;
pub mod deploy_contract;
pub mod interact_contract;
pub mod self_transfer;
pub mod send_meme;
pub mod my_new_task;  // Add this line

pub use self::check_balance::CheckBalanceTask;
pub use self::claim_faucet::ClaimFaucetTask;
pub use self::create_meme::CreateMemeTask;
pub use self::deploy_contract::DeployContractTask;
pub use self::interact_contract::InteractContractTask;
pub use self::self_transfer::SelfTransferTask;
pub use self::send_meme::SendMemeTokenTask;
pub use self::my_new_task::MyNewTask;  // Add this line
```

### Step 3: Add Task to Spammer

In `chains/risechain/src/spammer/mod.rs`, add the task to the tasks vector:

```rust
// In EvmSpammer::new() or initialization

let tasks: Vec<Box<RiseTask>> = vec![
    Box::new(CheckBalanceTask::new()),
    Box::new(ClaimFaucetTask::new()),
    Box::new(DeployContractTask::new()),
    Box::new(InteractContractTask::new()),
    Box::new(SelfTransferTask::new()),
    Box::new(CreateMemeTask::new()),
    Box::new(SendMemeTokenTask::new()),
    Box::new(MyNewTask::new()),  // Add this line
];
```

### Step 4: Update CODEBASE.md

Add the new task to the task reference section in CODEBASE.md:

| Task ID | Name | File | Purpose |
|---------|------|------|---------|
| X | MyNewTask | `my_new_task.rs` | Description of what it does |

### Step 5: Test the Task

```powershell
# Build first
._clean_and_compile_all.bat

# Test with debug_task
$env:WALLET_PASSWORD="password"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml --task X
```

### Common Task Patterns

#### Pattern: Sending ETH

```rust
use ethers::types::{TransactionRequest, U256};

async fn send_eth(&self, ctx: &TaskContext, recipient: &str, amount_wei: U256) -> Result<TaskResult> {
    let tx = TransactionRequest::pay(recipient, amount_wei)
        .from(ctx.wallet.address());
    
    let pending_tx = ctx.provider.send_transaction(tx, None).await?
        .confirmations(1).await?;
    
    let tx_hash = pending_tx.transaction_hash
        .ok_or_else(|| anyhow!("Transaction failed"))?;
    
    Ok(TaskResult {
        success: true,
        message: format!("Sent {} ETH to {}", amount_wei, recipient),
        tx_hash: Some(tx_hash.to_string()),
    })
}
```

#### Pattern: Deploying Contract

```rust
use ethers::contract::ContractFactory;
use ethers::types::Bytes;

async fn deploy_contract(&self, ctx: &TaskContext, bytecode: Bytes, abi: &str) -> Result<TaskResult> {
    let factory = ContractFactory::new(abi.to_owned(), bytecode, ctx.provider.clone(), ctx.wallet.clone());
    
    let deployer = factory.deploy::<(), _>(())?;
    let tx = deployer.tx.clone();
    let mut deployer = deployer.send().await?;
    
    let receipt = deployerconfirmations(1).await?;
    let contract_address = receipt.contract_address
        .ok_or_else(|| anyhow!("Deployment failed"))?;
    
    // Log to database if available
    if let Some(db) = &ctx.db {
        db.log_counter_contract_creation(
            &ctx.wallet.address().to_string(),
            &contract_address.to_string(),
            ctx.config.chain_id,
        ).await?;
    }
    
    Ok(TaskResult {
        success: true,
        message: format!("Contract deployed at {}", contract_address),
        tx_hash: Some(receipt.transaction_hash.to_string()),
    })
}
```

#### Pattern: Interacting with Contract

```rust
use ethers::contract::Contract;
use ethers::providers::Middleware;

async fn interact_contract(&self, ctx: &TaskContext, contract_address: &str, abi: &str) -> Result<TaskResult> {
    let contract = Contract::new(
        contract_address.parse::<Address>()?,
        abi.to_owned(),
        ctx.provider.clone(),
        ctx.wallet.clone(),
    );
    
    let call = contract.method::<_, u256>("increment", ())?;
    let pending_tx = call.send().await?;
    let receipt = pending_tx.confirmations(1).await?;
    
    let tx_hash = receipt.transaction_hash;
    
    Ok(TaskResult {
        success: true,
        message: "Contract incremented".to_string(),
        tx_hash: Some(tx_hash.to_string()),
    })
}
```

#### Pattern: Querying Database

```rust
async fn get_my_contracts(&self, ctx: &TaskContext) -> Result<Vec<ContractRecord>> {
    if let Some(db) = &ctx.db {
        let wallet_addr = ctx.wallet.address().to_string();
        db.get_deployed_counter_contracts(&wallet_addr).await
    } else {
        Ok(Vec::new())
    }
}
```

---

## 4. Adding New Chains (Step-by-Step)

### Step 1: Copy EVM Template

```powershell
# From project root
cp -r chains/evm-project chains/my-new-chain
```

### Step 2: Update Cargo.toml

Add the new crate to the workspace:

```toml
# In root Cargo.toml
[workspace]
members = [
    "core-logic",
    "chains/evm-project",
    "chains/risechain",
    "chains/my-new-chain",  # Add this line
]
```

### Step 3: Update New Crate's Cargo.toml

```toml
# In chains/my-new-chain/Cargo.toml
[package]
name = "my-new-chain"
version = "0.1.0"
edition = "2021"

[dependencies]
core-logic = { path = "../../core-logic" }
ethers = "2.0"
tokio = { version = "1", features = ["full"] }
# Add other dependencies as needed
```

### Step 4: Create Config.toml

Create `chains/my-new-chain/config.toml`:

```toml
rpc_url = "https://rpc.my-new-chain.testnet"
chain_id = 12345
private_key_file = "wallet-json"
tps = 10
```

### Step 5: Implement Configuration

Update `src/config.rs` with chain-specific fields:

```rust
// chains/my-new-chain/src/config.rs
use serde::Deserialize;
use anyhow::Result;

#[derive(Debug, Clone, Deserialize)]
pub struct MyChainConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    pub private_key_file: String,
    pub tps: u32,
    // Add chain-specific fields here
    pub custom_param: Option<String>,
}

impl MyChainConfig {
    pub fn from_path(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|e| anyhow!("{}", e))
    }
}
```

### Step 6: Implement Spammer

Update `src/spammer/mod.rs` with chain-specific logic:

```rust
// chains/my-new-chain/src/spammer/mod.rs

use async_trait::async_trait;
use core_logic::traits::{Spammer, SpammerStats, Task, TaskResult, SpamConfig};
use core_logic::utils::{WalletManager, ProxyManager, WorkerRunner};
use core_logic::config::WalletSource;
use tokio_util::sync::CancellationToken;
use std::sync::Arc;

pub struct MyChainSpammer {
    pub config: SpamConfig,
    pub wallet: DecryptedWallet,
    pub wallet_id: String,
    pub proxy_url: Option<String>,
    pub chain_config: MyChainConfig,  // Add chain-specific config
}

#[async_trait]
impl Spammer for MyChainSpammer {
    async fn start(&self, token: CancellationToken) -> Result<SpammerStats> {
        // Implement chain-specific spam logic
        // This is where you differ from the template
        
        let mut success = 0;
        let mut failed = 0;
        
        while !token.is_cancelled() {
            // Your spam logic here
            // For now, just a placeholder
            success += 1;
            
            // Rate limiting
            let delay_ms = 1000 / self.config.target_tps.max(1);
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {}
                _ = token.cancelled() => break,
            }
        }
        
        Ok(SpammerStats { success, failed })
    }
    
    fn stop(&self) {
        // Implementation
    }
}
```

### Step 7: Update Main Entry Point

Update `src/main.rs` to use your new components:

```rust
// chains/my-new-chain/src/main.rs

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "chains/my-new-chain/config.toml".to_string());
    
    let chain_config = MyChainConfig::from_path(&config_path)?;
    let wallet_sources = load_wallet_sources()?;
    let wallet_manager = Arc::new(WalletManager::new(wallet_sources));
    let proxies = ProxyManager::load_proxies();
    
    // Create spammers
    let mut spammers: Vec<Box<dyn Spammer>> = Vec::new();
    
    for i in 0..wallet_count {
        let wallet = wallet_manager.get_wallet(i)?;
        let proxy = proxies.get(i % proxies.len()).cloned();
        
        let spammer = MyChainSpammer {
            config: SpamConfig {
                rpc_url: chain_config.rpc_url.clone(),
                chain_id: chain_config.chain_id,
                target_tps: chain_config.tps,
                wallet_source: WalletSource::File {
                    path: chain_config.private_key_file.clone(),
                    encrypted: true,
                },
            },
            wallet,
            wallet_id: format!("{:03}", i),
            proxy_url: proxy.map(|p| p.url),
            chain_config: chain_config.clone(),
        };
        
        spammers.push(Box::new(spammer));
    }
    
    WorkerRunner::run_spammers(spammers).await?;
    
    Ok(())
}
```

### Step 8: Update Documentation

1. Add the new chain to CODEBASE.md section 3.2
2. Add build/run commands to CMD.md
3. Document chain-specific configuration in CODEBASE.md section 4

### Step 9: Build and Test

```powershell
# Build the new chain
cargo build -p my-new-chain

# Run with config
$env:WALLET_PASSWORD="pwd"; .\target_final\debug\my-new-chain.exe --config chains/my-new-chain/config.toml
```

---

## 5. Testing Approach

### 5.1 Testing Individual Tasks

Use the debug_task binary to test individual tasks:

```powershell
# Check balance for wallet 0
$env:WALLET_PASSWORD="pwd"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml --task 1

# Check all wallet balances
$env:WALLET_PASSWORD="pwd"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml --all

# Interactive mode (prompts for action)
$env:WALLET_PASSWORD="pwd"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml
```

### 5.2 Verifying Balance Changes

1. Check initial balance with `--all`
2. Run a task that sends ETH (task 0)
3. Check balance again with `--all`
4. Verify the balance decreased by the expected amount

### 5.3 Checking Database Records

```powershell
# Use sqlite3 CLI (if available)
sqlite3 rise.db "SELECT * FROM task_metrics ORDER BY id DESC LIMIT 10;"
sqlite3 rise.db "SELECT * FROM created_counter_contracts;"
sqlite3 rise.db "SELECT * FROM created_assets;"
```

### 5.4 Testing with Single Wallet

1. Temporarily set `worker_amount = 1` in config.toml
2. Run the spammer
3. Easier to trace and debug

### 5.5 Build Verification

```powershell
# Check code without building binaries
cargo check --workspace

# Build all binaries
cargo build --workspace

# Run clippy for linting
cargo clippy --workspace
```

### 5.6 Test Checklist

Before running a full spammer test:

- [ ] Wallet password is set correctly
- [ ] Config file is valid TOML
- [ ] Proxies file format is correct (if using proxies)
- [ ] Database file is not locked
- [ ] Sufficient ETH balance for gas
- [ ] Build completes without errors
- [ ] Debug task runs successfully

---

## 6. Security Rules

### 6.1 Never Commit Secrets

Always add these to `.gitignore`:

```
# Secrets - NEVER commit
.env
*.pem
*.key
pv.txt
proxies.txt
wallet-json/
!wallet-json/.gitkeep

# Database
*.db
*.db-journal

# Logs
logs/
*.log

# Build artifacts
target_final/
Cargo.lock
```

### 6.2 Use Zeroize for Sensitive Data

Always use `ZeroizeOnDrop` for structs containing private keys, mnemonics, or passwords:

```rust
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(ZeroizeOnDrop)]
#[zeroize(drop)]
pub struct SensitiveData {
    pub private_key: String,
    pub mnemonic: String,
    pub password: String,
}
```

### 6.3 Never Log Sensitive Data

**BAD** (never do this):
```rust
info!("Wallet private key: {}", private_key);
info!("Password: {}", password);
info!("RPC URL with key: {}", rpc_url);
```

**GOOD** (always do this):
```rust
info!("Wallet loaded successfully");
debug!("Wallet address: {}", address);
info!("Using RPC endpoint");
```

### 6.4 Password Handling

Always use environment variable with fallback to secure prompt:

```rust
fn get_wallet_password() -> Result<String> {
    if let Ok(pwd) = std::env::var("WALLET_PASSWORD") {
        Ok(pwd)
    } else {
        // Use secure prompt (no echo)
        rpassword::prompt_password("Enter wallet password: ")
            .context("Failed to read password")
    }
}
```

### 6.5 Don't Expose RPC URLs with Keys

If your RPC URL includes an API key, mask it in logs:

```rust
// Mask the API key part of the URL
let masked_url = rpc_url
    .replace(|c: char| !c.is_alphanumeric() && c != ':' && c != '/' && c != '.', "***");
info!("Connecting to RPC: {}", masked_url);
```

### 6.6 Periodic Security Checks

```powershell
# Check for vulnerabilities in dependencies
cargo audit

# Check for outdated dependencies
cargo outdated
```

### 6.7 Memory Safety

- Use `Mutex` for shared state
- Use `Arc` for shared references
- Always check for race conditions
- Use `tokio::sync` primitives for async contexts

---

## 7. Common Patterns

### 7.1 Pattern: Adding a Simple Transfer Task

```rust
// chains/risechain/src/task/simple_transfer.rs

use async_trait::async_trait;
use core_logic::traits::{Task, TaskResult};
use crate::task::TaskContext;
use ethers::types::{TransactionRequest, U256};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct SimpleTransferTask;

impl SimpleTransferTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for SimpleTransferTask {
    fn name(&self) -> &str {
        "SimpleTransfer"
    }
    
    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        // Build transfer transaction
        let tx = TransactionRequest::pay(
            "0xRecipientAddressHere".parse::<Address>()?,
            U256::from(1000000000000000u64),  // 0.001 ETH
        )
        .from(ctx.wallet.address());
        
        // Send transaction
        let pending = ctx.provider.send_transaction(tx, None).await?
            .confirmations(1).await?;
        
        let tx_hash = pending.transaction_hash
            .ok_or_else(|| anyhow!("Transaction failed"))?;
        
        Ok(TaskResult {
            success: true,
            message: format!("Transfer sent: {}", tx_hash),
            tx_hash: Some(tx_hash.to_string()),
        })
    }
}
```

### 7.2 Pattern: Modifying Gas Configuration

```rust
// Create custom GasManager with different limits

#[derive(Clone)]
pub struct CustomGasManager {
    provider: Arc<Provider<Http>>,
    max_gwei: f64,
    priority_gwei: f64,
}

impl CustomGasManager {
    pub fn new(provider: Arc<Provider<Http>>) -> Self {
        Self {
            provider,
            max_gwei: 0.000000050,  // 50 Gwei max
            priority_gwei: 0.000000002,  // 2 Gwei priority
        }
    }
    
    pub async fn get_fees(&self) -> Result<(U256, U256)> {
        let block = self.provider.get_block(BlockNumber::Latest).await?;
        let base_fee = block.base_fee_per_gas.unwrap_or_default();
        
        let priority_fee = parse_units(self.priority_gwei, "gwei")?;
        let max_fee = base_fee + priority_fee;
        let max_configured = parse_units(self.max_gwei, "gwei")?;
        
        Ok((max_fee.min(max_configured), priority_fee))
    }
}
```

### 7.3 Pattern: Adding Database Logging

```rust
async fn log_to_database(
    db: &Option<Arc<DatabaseManager>>,
    task_name: &str,
    wallet_address: &str,
    result: &TaskResult,
    duration_ms: u64,
) -> Result<()> {
    if let Some(database) = db {
        database.log_task_result(
            result.clone(),
            task_name,
            wallet_address,
            "001",  // worker_id
            duration_ms,
        ).await?;
    }
    Ok(())
}
```

### 7.4 Pattern: Using Proxies in Requests

```rust
use ethers::providers::{Http, Provider};
use ethers::providers::ProxyMiddleware;
use std::sync::Arc;

fn create_provider_with_proxy(rpc_url: &str, proxy_url: Option<&str>) -> Arc<Provider<Http>> {
    if let Some(proxy) = proxy_url {
        // Create HTTP client with proxy
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(proxy).unwrap())
            .build()
            .unwrap();
        
        let transport = ethers::transports::Http::new_with_client(rpc_url, client);
        Arc::new(Provider::new(transport))
    } else {
        let transport = ethers::transports::Http::new(rpc_url);
        Arc::new(Provider::new(transport))
    }
}
```

### 7.5 Pattern: Random Selection from List

```rust
use rand::seq::SliceRandom;

fn select_random_item<T>(items: &[T]) -> Option<&T> {
    items.choose(&mut rand::thread_rng())
}

fn select_random_address(addresses: &[String]) -> Option<&String> {
    addresses.choose(&mut rand::thread_rng())
}

// Usage
if let Some(recipient) = select_random_address(&addresses) {
    // Use recipient
}
```

### 7.6 Pattern: Graceful Shutdown with Signal Handling

```rust
use tokio_util::sync::CancellationToken;
use tokio::signal;

async fn setup_shutdown_handler(token: CancellationToken) {
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
        token.cancel();
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    let token = CancellationToken::new();
    let token_clone = token.clone();
    
    // Setup Ctrl+C handler
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
        token_clone.cancel();
    });
    
    // Run spammers
    let stats = WorkerRunner::run_spammers(spammers).await?;
    
    Ok(())
}
```

---

## 8. Troubleshooting Guide

### 8.1 Build Errors

**Error: "could not compile... due to previous compilation error"**

**Solution**: Clean and rebuild
```powershell
._clean_and_compile_all.bat
```

**Error: "file lock" or "cannot open file"**

**Solution**: Close any processes using the files, then rebuild

**Error: "dependency conflicts"**

**Solution**: Update dependencies
```powershell
cargo update
```

### 8.2 Runtime Panics

**Error: "thread 'main' panicked at..."

**Solution**: Enable backtraces
```powershell
$env:RUST_BACKTRACE=1; $env:RUST_LOG=debug; .\target_final\debug\rise-project.exe
```

**Common causes**:
- Invalid wallet password
- Invalid RPC URL
- Database locked by another process

### 8.3 Database Lock Errors

**Error: "database is locked"**

**Solutions**:
1. Close any other processes using the database
2. Increase `MAX_CONNECTIONS` in DatabaseManager
3. Use `sqlite3 rise.db ".timeout 5000"` to set timeout

### 8.4 Wallet Decryption Failures

**Error: "Invalid password" or "decryption failed"**

**Solutions**:
1. Verify `WALLET_PASSWORD` environment variable is set
2. Check password is correct (case-sensitive)
3. Verify wallet JSON file is valid
4. Check wallet file hasn't been corrupted

### 8.5 Transaction Failures

**Error: "out of gas" or "intrinsic gas too low"**

**Solutions**:
1. Increase gas limit in GasManager
2. Check transaction data size
3. Verify contract bytecode doesn't have issues

**Error: "nonce too low" or "already known"**

**Solutions**:
1. Wait for pending transactions to confirm
2. Manually reset nonce (advanced)

**Error: "insufficient funds"**

**Solutions**:
1. Check wallet ETH balance
2. Reduce gas price in GasManager
3. Ensure enough ETH for gas + value

### 8.6 Proxy Failures

**Error: "connection refused" or "timeout"**

**Solutions**:
1. Verify proxy format in `proxies.txt`
2. Check proxy credentials
3. Test proxy manually
4. Try without proxies to isolate issue

### 8.7 RPC Errors

**Error: "rate limited" or "quota exceeded"**

**Solutions**:
1. Reduce TPS in config
2. Add delays between requests
3. Use backup RPC endpoints

**Error: "network error" or "connection refused"**

**Solutions**:
1. Verify RPC URL is correct
2. Check network connectivity
3. Try different RPC endpoint

---

## 9. File Modification Checklist

Before finalizing any changes, complete this checklist:

### Code Changes

- [ ] Run `cargo fmt` on modified code
- [ ] Run `cargo check --workspace` - no errors
- [ ] Run `cargo clippy --workspace` - no warnings
- [ ] Verify code compiles with `cargo build --workspace`

### Documentation Updates

- [ ] Update `CODEBASE.md` with new components
- [ ] Update `AGENTS.md` if adding new patterns
- [ ] Update `CMD.md` if adding new commands
- [ ] Update `README.md` if adding new features

### Testing

- [ ] Test with debug_task before spammer
- [ ] Test with single wallet first
- [ ] Verify database logging works
- [ ] Verify logging output is correct

### Security Review

- [ ] No private keys in logs
- [ ] No passwords in logs
- [ ] Sensitive data uses ZeroizeOnDrop
- [ ] No secrets in .git (check .gitignore)

### Git (if applicable)

- [ ] Review changes with `git diff`
- [ ] Add new files to git tracking
- [ ] Commit message follows project conventions

---

## 10. Project Conventions

### 10.1 Commit Message Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types**:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Formatting changes
- `refactor`: Code restructuring
- `test`: Adding tests
- `chore`: Maintenance tasks

**Example**:
```
feat(task): Add CreateMemeTask for ERC-20 token deployment

Implements task ID 5 for deploying meme tokens with random names.
Uses standard ERC-20 bytecode from contracts/mod.rs.

Closes #10
```

### 10.2 Code Style

- Use rustfmt default settings
- Prefer explicit over implicit
- Document public APIs with doc comments
- Use TODO comments for future work (`// TODO: ...`)

### 10.3 Naming Conventions

| Item | Convention | Example |
|------|------------|---------|
| Structs | PascalCase | `EvmSpammer`, `GasManager` |
| Functions | snake_case | `load_wallets()`, `get_fees()` |
| Variables | snake_case | `wallet_manager`, `tx_hash` |
| Constants | SCREAMING_SCREAMING_SNAKE_CASE | `MAX_CONNECTIONS` |
| Modules | snake_case | `wallet_manager`, `proxy_manager` |
| Task names | CamelCase | `"CheckBalance"`, `"SendMeme"` |

### 10.4 Error Messages

- Be descriptive but concise
- Include relevant context
- Start with lowercase
- Don't end with period

```rust
// Good
Err(anyhow!("Failed to parse config from {}", path))

// Bad
Err(anyhow!("Error parsing config file."))
```

---

## 11. Quick Start for New Contributors

1. **Read CODEBASE.md** - Understand the architecture
2. **Read AGENTS.md** - Know the conventions
3. **Run a build** - Verify your environment
4. **Run debug_task** - Verify wallet decryption
5. **Pick a small task** - Fix a TODO or add a simple feature
6. **Test thoroughly** - Use debug_task before spammer
7. **Submit changes** - Follow commit message format

---

## 12. Contact and Support

For issues with the framework:

1. Check this guide (AGENTS.md)
2. Check CODEBASE.md for architecture
3. Check CMD.md for commands
4. Check logs in `logs/smart_main.log`
5. Enable `RUST_BACKTRACE=1` for detailed errors

---

**Document Version**: 1.0
**Last Updated**: 2024-01-18
**Maintained By**: AI Assistants working on testnet-framework
