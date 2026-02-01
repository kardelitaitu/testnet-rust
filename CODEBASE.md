# Rust Multi-Chain Testnet Framework - Codebase Reference

## 1. Project Overview

### 1.1 Project Name and Purpose
**Name**: Rust Multi-Chain Testnet Framework

**Purpose**: A modular, high-performance automation framework for testnet interactions across multiple blockchain networks. It provides tools for automated wallet management, task execution, and transaction generation on EVM-compatible chains (Rise, Monad, etc.) and non-EVM chains (Solana, Sui).

### 1.2 Technology Stack
- **Language**: Rust (Latest Stable)
- **Async Runtime**: tokio
- **EVM Interactions**: ethers-rs
- **Database**: SQLite with sqlx
- **Logging**: tracing with tracing-appender
- **Configuration**: serde with TOML
- **Cryptography**: AES-256-GCM, Scrypt

### 1.3 Workspace Structure

```
testnet-framework/
├── core-logic/                     # Shared library (Wallet, Logging, Config, DB)
│   ├── src/
│   │   ├── bin/
│   │   │   └── decrypt_debugger.rs
│   │   ├── config/
│   │   │   └── mod.rs
│   │   ├── database.rs
│   │   ├── lib.rs
│   │   ├── security/
│   │   │   └── mod.rs
│   │   ├── traits/
│   │   │   └── mod.rs
│   │   └── utils/
│   │       ├── browser.rs
│   │       ├── logger.rs
│   │       ├── mod.rs
│   │       ├── proxy_manager.rs
│   │       ├── runner.rs
│   │       └── wallet_manager.rs
│   └── Cargo.toml
├── chains/
│   ├── evm-project/               # Generic EVM template
│   │   ├── src/
│   │   │   ├── config.rs
│   │   │   ├── main.rs
│   │   │   └── spammer/
│   │   │       └── mod.rs
│   │   └── config.toml
│   ├── risechain/                 # RISE Chain implementation
│   │   ├── src/
│   │   │   ├── bin/
│   │   │   │   └── debug_task.rs
│   │   │   ├── config.rs
│   │   │   ├── contracts/
│   │   │   │   └── mod.rs
│   │   │   ├── lib.rs
│   │   │   ├── main.rs
│   │   │   ├── spammer/
│   │   │   │   └── mod.rs
│   │   │   ├── task/
│   │   │   │   ├── check_balance.rs
│   │   │   │   ├── claim_faucet.rs
│   │   │   │   ├── create_meme.rs
│   │   │   │   ├── deploy_contract.rs
│   │   │   │   ├── interact_contract.rs
│   │   │   │   ├── mod.rs
│   │   │   │   ├── self_transfer.rs
│   │   │   │   └── send_meme.rs
│   │   │   └── utils/
│   │   │       ├── gas.rs
│   │   │       └── mod.rs
│   │   ├── config.toml
│   │   ├── debug-task.rs
│   │   └── recompile.rs
│   └── solana-project/            # Solana implementation (WIP)
│       ├── src/
│       │   └── config.rs
│       └── config.toml
├── wallet-json/                   # Encrypted wallet storage (JSON format)
├── proxies.txt                    # Proxy list (ip:port:user:pass)
├── rise.db                        # SQLite database (task metrics, contracts, assets)
├── address.txt                    # Recipient addresses for SendETH task
├── CMD.md                         # Command reference
├── README.md                      # Project documentation
├── Cargo.toml                     # Workspace configuration
└── _clean_and_compile_all.bat     # Build script
```

### 1.4 Prerequisites
- Rust (Latest Stable) - https://rust-lang.org/tools/install
- SQLite (optional, for persistent tracking)
- Windows PowerShell (for build scripts)

---

## 2. Core-Logic Library (`core-logic/`)

The core-logic library provides shared components used by all chain implementations. It is organized into modules for wallet management, proxy management, logging, database persistence, configuration, traits, and execution runners.

### 2.1 Library Entry Point (`lib.rs`)

The `lib.rs` file serves as the entry point for the core-logic library. It re-exports all public modules to provide an ergonomic API for consumers.

**Module Exports:**
```rust
pub mod traits;      // Trait definitions (Spammer, Task, WalletLoader)
pub mod config;      // Configuration structs (SpamConfig, WalletSource, ChainConfig)
pub mod utils;       // Utility modules (wallet_manager, proxy_manager, logger, runner)
pub mod database;    // SQLite database management
pub mod security;    // Cryptographic utilities (wallet decryption)
```

All modules are re-exported publicly, allowing consumers to import with:
```rust
use core_logic::utils::wallet_manager::WalletManager;
use core_logic::traits::Spammer;
```

### 2.2 Wallet Manager (`utils/wallet_manager.rs`)

The wallet manager module handles loading, decryption, and caching of encrypted JSON wallets. It supports multiple blockchain networks (EVM, Solana, SUI) and provides secure memory handling.

#### 2.2.1 DecryptedWallet Struct

The `DecryptedWallet` struct holds decrypted key material for all supported blockchain networks. It implements `ZeroizeOnDrop` for automatic memory sanitization.

```rust
pub struct DecryptedWallet {
    pub mnemonic: String,              // BIP39 mnemonic phrase
    pub evm_private_key: String,       // EVM-compatible private key (hex)
    pub evm_address: String,           // EVM address (checksummed)
    pub sol_private_key: String,       // Solana private key (base58)
    pub sol_address: String,           // Solana address
    pub sui_private_key: String,       # Sui private key
    pub sui_address: String,           # Sui address
}
```

**Security Features:**
- `ZeroizeOnDrop` trait: Automatically zeroes all fields when the struct is dropped
- Custom `Debug` implementation: Redacts all sensitive fields, displaying `***REDACTED***` instead of actual values
- All fields are public for direct access but should be handled with care

**Example Debug Output:**
```
DecryptedWallet {
    mnemonic: ***REDACTED***,
    evm_private_key: ***REDACTED***,
    evm_address: "0x1234...",
    ...
}
```

#### 2.2.2 WalletSource Enum

The `WalletSource` enum represents different ways to load wallet keys.

```rust
pub enum WalletSource {
    JsonFile(PathBuf),    // Encrypted JSON file in wallet-json/ directory
    RawKey(String),       // Raw private key (for fallback or testing)
}
```

**JsonFile Variant**: Points to a JSON file in the `wallet-json/` directory. These files contain encrypted key material with the following structure:
```json
{
    "encrypted": {
        "ciphertext": "hex-encoded...",
        "iv": "hex-encoded...",
        "salt": "hex-encoded...",
        "tag": "hex-encoded..."
    }
}
```

**RawKey Variant**: Contains a plaintext private key string. Used as a fallback when no JSON files are found.

#### 2.2.3 WalletManager Struct

The main manager struct for handling wallet operations.

```rust
pub struct WalletManager {
    sources: Vec<WalletSource>,                    // Sources to load wallets from
    cache: Mutex<HashMap<usize, DecryptedWallet>>, // Cached decrypted wallets by index
}
```

**Key Methods:**

`new(sources: Vec<WalletSource>) -> Self`
- Creates a new WalletManager with the given sources
- Initializes empty cache

`get_wallet(&self, index: usize) -> Result<DecryptedWallet>`
- Retrieves a wallet by index
- If not in cache, decrypts on demand
- Returns error if decryption fails or index is out of bounds

`decrypt_json_wallet(path: &Path, password: &str) -> Result<DecryptedWallet>`
- Reads and decrypts a JSON wallet file
- Uses `SecurityUtils::decrypt_components()` for AES-256-GCM decryption
- Parses the encrypted object from JSON
- Returns populated DecryptedWallet

`load_wallets(&self, password: &str) -> Result<()>`
- Optional method to pre-load and decrypt all wallets
- Populates the cache for faster subsequent access

#### 2.2.4 Loading Flow

The wallet loading follows this flow:

1. **Discovery Phase**: WalletManager scans `wallet-json/` directory for `.json` files
2. **Source Collection**: Each JSON file becomes a `WalletSource::JsonFile` variant
3. **Fallback Detection**: If no JSON files found, checks for `pv.txt` with raw private keys
4. **Decryption Request**: When wallet is needed, `get_wallet(index)` is called
5. **Cache Check**: If wallet is cached, return immediately
6. **Decryption**: If not cached, call `decrypt_json_wallet()` with password
7. **Caching**: Store decrypted wallet in cache (Mutex-protected HashMap)
8. **Return**: Provide DecryptedWallet to caller

#### 2.2.5 Password Handling

Passwords can be provided in two ways:

1. **Environment Variable**: `WALLET_PASSWORD` environment variable
2. **Interactive Prompt**: If env var is not set, debugger prompts user securely

```rust
fn get_password() -> Result<String> {
    if let Ok(pwd) = std::env::var("WALLET_PASSWORD") {
        Ok(pwd)
    } else {
        // Interactive prompt implementation
        prompt_for_password()
    }
}
```

### 2.3 Proxy Manager (`utils/proxy_manager.rs`)

The proxy manager module handles loading and providing access to proxy configurations from `proxies.txt`.

#### 2.3.1 ProxyConfig Struct

```rust
pub struct ProxyConfig {
    pub url: String,          // Full URL (e.g., "http://ip:port")
    pub username: Option<String>,  // Authentication username
    pub password: Option<String>,  // Authentication password
}
```

#### 2.3.2 ProxyManager Struct

```rust
pub struct ProxyManager;

impl ProxyManager {
    pub fn load_proxies() -> Vec<ProxyConfig> {
        // Main entry point - returns all loaded proxies
    }
}
```

The ProxyManager is stateless - it only has a static method `load_proxies()`.

#### 2.3.3 Proxy File Format

The `proxies.txt` file supports two formats:

**Full Authentication Format:**
```
ip:port:username:password
```

**No Authentication Format:**
```
ip:port
```

**Comments and Empty Lines:**
```
# This is a comment
ip:port:user:pass    # Inline comment
ip:port

192.168.1.1:8080:user:pass
10.0.0.1:3128
```

#### 2.3.4 Parsing Logic

The `load_proxies()` function:

1. Reads `proxies.txt` from working directory
2. Splits file into lines
3. For each non-empty, non-comment line:
   - Split by `:`
   - First two parts (index 0, 1) = host:port
   - Parts 3 and 4 (if present) = username:password
   - Construct `http://host:port` base URL
   - Wrap in ProxyConfig with optional credentials
4. Return Vec<ProxyConfig>

**Example Parsing:**
```
"192.168.1.1:8080:user:pass"
  ↓ split by ':'
  ["192.168.1.1", "8080", "user", "pass"]
  ↓
  url = "http://192.168.1.1:8080"
  username = Some("user")
  password = Some("pass")
```

#### 2.3.5 Graceful Degradation

If `proxies.txt` is missing or unreadable:
- Logs a warning message
- Returns an empty Vec
- Execution continues without proxies

### 2.4 Logger Module (`utils/logger.rs`)

The logger module implements structured, context-aware logging with colored terminal output and file logging. It uses the `tracing` crate for instrumenting code.

#### 2.4.1 ContextData Struct

```rust
pub struct ContextData {
    pub worker_id: Option<String>,      // Identifier for the worker thread
    pub wallet_id: Option<String>,      // Identifier for the wallet being used
    pub proxy_id: Option<String>,       // Identifier for the proxy being used
}
```

ContextData holds the correlation IDs used to track which worker, wallet, and proxy are associated with a log entry.

#### 2.4.2 ContextDiscoveryLayer

A tracing subscriber layer that extracts context from span attributes.

```rust
pub struct ContextDiscoveryLayer;

impl<S> Layer<S> for ContextDiscoveryLayer
where
    S: Subscriber,
{
    // Implementation that extracts ContextData from span fields
    // Allows downstream formatters to access worker/wallet/proxy IDs
}
```

#### 2.4.3 TerminalFormatter

Formats log output for terminal with ANSI colors.

**Output Format:**
```
[WK:XXX][WL:XXX][P:XXX] [StatusColor] Message
```

**Example:**
```
[WK:001][WL:005][P:003] Success Faucet claim completed (B: 12345) in 2.3s
[WK:002][WL:007][P:001] Failed DeployContract error: out of gas
```

**Status Colors:**
- Success → Light Green (`\x1b[92m`)
- Failed → Light Red (`\x1b[91m`)
- Plain text for info/warn/debug levels

#### 2.4.4 FileFormatter

Formats log output for file storage.

**Output Format:**
```
YYYY-MM-DD HH:MM:SS [LEVEL] [WK:...][WL:...][P:...] message
```

**Example:**
```
2024-01-18 15:32:45 [INFO] [WK:001][WL:005][P:003] Spammer started for wallet 005
2024-01-18 15:32:47 [INFO] [WK:001][WL:005][P:003] Success Faucet claim completed (B: 12345) in 2.3s
2024-01-18 15:32:48 [WARN] [WK:002][WL:007][P:001] Failed DeployContract error: out of gas
```

#### 2.4.5 Logger Setup

```rust
pub fn setup_logger() -> Result<()>
```

The setup function:

1. Creates daily rolling file appender in `logs/smart_main.log`
2. Registers ContextDiscoveryLayer for context extraction
3. Registers TerminalFormatter with ANSI colors
4. Registers FileFormatter with timestamps
5. Configures EnvFilter for log level control

**Default Log Level:**
- If `RUST_LOG` env var is set, uses that level
- Otherwise, defaults to INFO level

### 2.5 Database Module (`database.rs`)

The database module provides SQLite-based persistence for task metrics, contract deployments, asset creations, and proxy statistics.

#### 2.5.1 DatabaseManager Struct

```rust
pub struct DatabaseManager {
    pool: SqlitePool,       // sqlx connection pool
}

impl DatabaseManager {
    pub const MAX_CONNECTIONS: u32 = 5;  // Maximum concurrent connections
    
    pub async fn new(path: &str) -> Result<Self> {
        // Initialize pool with max 5 connections
    }
}
```

The DatabaseManager wraps a SQLite connection pool with a maximum of 5 concurrent connections.

#### 2.5.2 Database Schema

**task_metrics Table**
```sql
CREATE TABLE task_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    worker_id TEXT NOT NULL,
    wallet_address TEXT NOT NULL,
    task_name TEXT NOT NULL,
    status TEXT NOT NULL,       -- "success" or "failed"
    message TEXT,
    duration_ms INTEGER,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

Purpose: Tracks all task executions with their outcome, duration, and associated metadata.

**created_counter_contracts Table**
```sql
CREATE TABLE created_counter_contracts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_address TEXT NOT NULL,
    contract_address TEXT NOT NULL,
    chain_id INTEGER,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

Purpose: Records deployed counter contract addresses for later interaction.

**created_assets Table**
```sql
CREATE TABLE created_assets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_address TEXT NOT NULL,
    asset_address TEXT NOT NULL,
    asset_type TEXT NOT NULL,   -- "token" or "nft"
    name TEXT,
    symbol TEXT,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

Purpose: Records created asset addresses (tokens, NFTs) with metadata.

**proxy_stats Table**
```sql
CREATE TABLE proxy_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    proxy_url TEXT NOT NULL UNIQUE,
    success_count INTEGER DEFAULT 0,
    fail_count INTEGER DEFAULT 0
);
```

Purpose: Tracks proxy reliability for rotation decisions.

#### 2.5.3 Key Methods

`log_task_result(&self, result: TaskResult, task_name: &str, wallet_address: &str, worker_id: &str, duration_ms: u64) -> Result<()>`

Records a task execution to the database:
- Inserts into `task_metrics` table
- Stores wallet address, task name, status, message, duration

`log_counter_contract_creation(&self, wallet_address: &str, contract_address: &str, chain_id: u64) -> Result<()>`

Records a deployed counter contract:
- Inserts into `created_counter_contracts` table
- Used by InteractContractTask to find contracts to interact with

`log_asset_creation(&self, wallet_address: &str, asset_address: &str, asset_type: &str, name: &str, symbol: &str) -> Result<()>`

Records a created asset (token or NFT):
- Inserts into `created_assets` table
- Used by SendMemeTokenTask to find tokens to send

`update_proxy_stats(&self, proxy_url: &str, success: bool) -> Result<()>`

Updates proxy statistics atomically:
- Uses `ON CONFLICT` for upsert semantics
- Increments success_count or fail_count
- Tracks proxy reliability for rotation

`get_assets_by_type(&self, wallet_address: &str, asset_type: &str) -> Result<Vec<AssetRecord>>`

Query assets by wallet and type:
- Returns all assets of given type for a wallet
- Used by tasks that need to find existing assets

`get_deployed_counter_contracts(&self, wallet_address: &str) -> Result<Vec<ContractRecord>>`

Query deployed counter contracts:
- Returns all counter contracts deployed by a wallet
- Used by InteractContractTask

### 2.6 Traits Module (`traits/mod.rs`)

The traits module defines core trait-based patterns for extensibility. This enables the framework to support different spammer implementations and task types.

#### 2.6.1 Spammer Trait

The main trait for spam/task workers.

```rust
#[async_trait]
pub trait Spammer: Send + Sync {
    fn new(config: SpamConfig) -> Self
    where
        Self: Sized;
    
    async fn start(&self, cancellation_token: CancellationToken) -> Result<SpammerStats>;
    
    fn stop(&self);
}
```

**Methods:**
- `new(config)`: Constructor that takes spam configuration
- `start(token)`: Starts the spammer loop, returns stats on completion
- `stop()`: Signals the spammer to stop (sets internal flag)

**Usage:**
```rust
let spammer: Box<dyn Spammer> = Box::new(EvmSpammer::new(config));
let stats = spammer.start(token).await;
```

#### 2.6.2 Task Trait

Generic task trait for defining executable tasks.

```rust
#[async_trait]
pub trait Task<Ctx>: Send + Sync {
    fn name(&self) -> &str;
    
    async fn run(&self, ctx: Ctx) -> Result<TaskResult>;
}
```

**Type Parameters:**
- `Ctx`: Context type passed to the task (e.g., `TaskContext` for EVM tasks)

**Methods:**
- `name()`: Returns task identifier for logging
- `run(ctx)`: Executes the task with given context

**Usage:**
```rust
let task: Box<dyn Task<TaskContext>> = Box::new(CheckBalanceTask);
let result = task.run(ctx).await?;
```

#### 2.6.3 WalletLoader Trait

Trait for abstracting wallet loading.

```rust
pub trait WalletLoader {
    fn load_wallets(&self) -> Result<Vec<DecryptedWallet>>;
}
```

#### 2.6.4 Supporting Structs

**SpammerStats**
```rust
pub struct SpammerStats {
    pub success: u64,    // Number of successful operations
    pub failed: u64,     // Number of failed operations
}
```

**TaskResult**
```rust
pub struct TaskResult {
    pub success: bool,                   // Whether task succeeded
    pub message: String,                 // Human-readable status message
    pub tx_hash: Option<String>,         // Transaction hash if applicable
}
```

#### 2.6.5 Design Patterns

**Trait Objects for Extensibility:**
```rust
// Different implementations can be used interchangeably
let spammers: Vec<Box<dyn Spammer>> = vec![
    Box::new(EvmSpammer::new(config1)),
    Box::new(SolanaSpammer::new(config2)),
];
```

**Generic Context:**
```rust
// Task is generic over context type
impl Task<TaskContext> for CheckBalanceTask { ... }
impl Task<SolanaContext> for SolanaBalanceTask { ... }
```

**Async Traits:**
The `async_trait` crate enables ergonomic async trait implementation without boxing.

### 2.7 Config Module (`config/mod.rs`)

The config module defines configuration structs parsed from TOML files.

#### 2.7.1 SpamConfig Struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpamConfig {
    pub rpc_url: String,              // Ethereum RPC endpoint
    pub chain_id: u64,                // Chain ID (e.g., 1 for mainnet, 11155111 for Sepolia)
    pub target_tps: u32,              // Target transactions per second
    pub duration_seconds: Option<u64>, // Optional: run for specified duration
    pub wallet_source: WalletSource,  // Where to load wallets from
}
```

**Example TOML:**
```toml
rpc_url = "https://rpc.sepolia.org"
chain_id = 11155111
target_tps = 10
duration_seconds = 3600
```

#### 2.7.2 WalletSource Enum

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "path")]
pub enum WalletSource {
    #[serde(rename = "file")]
    File {
        path: String,
        encrypted: bool,
    },
    #[serde(rename = "env")]
    Env {
        key: String,
    },
}
```

**File Variant:**
```toml
[wallet]
type = "file"
path = "wallet-json"
encrypted = true
```

**Env Variant:**
```toml
[wallet]
type = "env"
key = "PRIVATE_KEY"
```

#### 2.7.3 ProxyConfig Struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}
```

#### 2.7.4 ChainConfig Struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub name: String,
    pub rpc_endpoint: String,
    pub chain_id: u64,
}
```

### 2.8 Runner Module (`runner.rs`)

The runner module orchestrates concurrent task execution with graceful shutdown.

#### 2.8.1 WorkerRunner Struct

```rust
pub struct WorkerRunner;

impl WorkerRunner {
    pub async fn run_spammers(spammers: Vec<Box<dyn Spammer>>) -> Result<SpammerStats> {
        // Main entry point for running multiple spammers concurrently
    }
}
```

#### 2.8.2 Execution Flow

```rust
pub async fn run_spammers(spammers: Vec<Box<dyn Spammer>>) -> Result<SpammerStats> {
    // 1. Create JoinSet for managing spawned tasks
    let mut join_set = JoinSet::new();
    
    // 2. Create CancellationToken for graceful shutdown
    let cancellation_token = CancellationToken::new();
    
    // 3. Spawn Ctrl+C listener
    let child_token = cancellation_token.clone();
    join_set.spawn(async move {
        // Listen for Ctrl+C signal
        // On signal, cancel the token
    });
    
    // 4. Spawn each spammer as a concurrent task
    for (i, spammer) in spammers.into_iter().enumerate() {
        let token = cancellation_token.clone();
        let worker_id = format!("{:03}", i);
        
        join_set.spawn(async move {
            // Create tracing span with worker_id
            let span = span!(Info, "worker", worker_id);
            
            // Call spammer.start() with cancellation token
            let stats = spammer.start(token).await?;
            
            Ok(stats)
        });
    }
    
    // 5. Collect results from all workers
    let mut total_success = 0;
    let mut total_failed = 0;
    
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(stats_result) => {
                total_success += stats_result.success;
                total_failed += stats_result.failed;
            }
            Err(e) => {
                // Handle task panic/error
            }
        }
    }
    
    // 6. Log final aggregated statistics
    info!("All workers completed. Success: {}, Failed: {}", 
          total_success, total_failed);
    
    Ok(SpammerStats { success: total_success, failed: total_failed })
}
```

#### 2.8.3 Shutdown Behavior

1. **Ctrl+C Detection**: A dedicated task listens for OS interrupt signals
2. **Cancellation Propagation**: On interrupt, `cancellation_token.cancel()` is called
3. **Task Termination**: All spammer tasks check `token.is_cancelled()` during execution
4. **Graceful Wait**: JoinSet waits for all tasks to complete (or be cancelled)
5. **Stats Aggregation**: Combines stats from all completed workers
6. **Final Logging**: Reports total success/failure counts with duration

### 2.9 Security Module (`security/mod.rs`)

The security module provides cryptographic utilities for wallet decryption.

#### 2.9.1 SecurityUtils Struct

```rust
pub struct SecurityUtils;

impl SecurityUtils {
    pub fn decrypt_components(
        ciphertext_hex: &str,
        iv_hex: &str,
        salt_hex: &str,
        tag_hex: &str,
        password: &str,
    ) -> Result<String> {
        // Decrypts wallet components using AES-256-GCM
    }
}
```

#### 2.9.2 Decryption Specifications

| Parameter | Value |
|-----------|-------|
| **Algorithm** | AES-256-GCM |
| **Key Derivation** | Scrypt |
| **Scrypt Parameters** | N=16384, r=8, p=1, dkLen=32 |
| **Input Format** | Hex-encoded ciphertext, IV, salt, authentication tag |
| **Output** | Decrypted key as UTF-8 String |

**Decryption Flow:**

1. **Hex Decoding**: Convert hex-encoded inputs to bytes
2. **Key Derivation**: Use Scrypt with password and salt to derive 32-byte key
3. **AES Decryption**: Use AES-256-GCM with derived key and IV
4. **Authentication**: Verify GCM authentication tag
5. **Output**: Return decrypted bytes as UTF-8 String

**Input JSON Structure:**
```json
{
    "encrypted": {
        "ciphertext": "a1b2c3d4...",
        "iv": "e5f6g7h8...",
        "salt": "i9j0k1l2...",
        "tag": "m3n4o5p6..."
    }
}
```

---

## 3. Chain Implementations

### 3.1 RiseChain Implementation (`chains/risechain/`)

RiseChain is a production-ready implementation for the RISE testnet. It includes a comprehensive task system, gas management, and database persistence.

#### 3.1.1 Main Entry Point (`src/main.rs`)

The main entry point orchestrates initialization and launches the spammer workers.

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // 1. Parse CLI arguments for config path
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "chains/risechain/config.toml".to_string());
    
    // 2. Load RiseConfig from TOML
    let rise_config = RiseConfig::from_path(&config_path)?;
    
    // 3. Initialize WalletManager
    let wallet_password = get_password()?;
    let wallet_manager = WalletManager::new(wallet_sources);
    let wallet_manager = Arc::new(wallet_manager);
    
    // 4. Initialize ProxyManager
    let proxies = ProxyManager::load_proxies();
    
    // 5. Initialize DatabaseManager
    let db_manager = DatabaseManager::new("rise.db").await?;
    let db_manager = Arc::new(db_manager);
    
    // 6. Create EvmSpammer for each wallet
    let mut spammers: Vec<Box<dyn Spammer>> = Vec::new();
    
    for wallet_id in 0..wallet_count {
        // Decrypt wallet
        let wallet = wallet_manager.get_wallet(wallet_id)?;
        
        // Assign random proxy if available
        let proxy = proxies.choose(&mut rand::thread_rng()).cloned();
        let proxy_id = proxy.as_ref()
            .map(|p| format_proxy_id(p))
            .unwrap_or_else(|| "NONE".to_string());
        
        // Create EvmSpammer instance
        let spammer = EvmSpammer {
            config: spam_config,
            rise_config: rise_config.clone(),
            wallet,
            wallet_id: format!("{:03}", wallet_id),
            proxy_id,
            proxy_url: proxy.map(|p| p.url),
            db: Some(db_manager.clone()),
            gas_manager: Arc::new(GasManager::new(provider.clone())),
        };
        
        spammers.push(Box::new(spammer));
    }
    
    // 7. Run all spammers concurrently
    let stats = WorkerRunner::run_spammers(spammers).await?;
    
    Ok(())
}
```

#### 3.1.2 Configuration (`src/config.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiseConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    pub private_key_file: String,
    pub tps: u32,
    pub worker_amount: Option<usize>,
    pub min_delay_ms: Option<u64>,
    pub max_delay_ms: Option<u64>,
    pub proxies: Option<Vec<ProxyConfig>>,
}

impl RiseConfig {
    pub fn from_path(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|e| anyhow!("Config parse error: {}", e))
    }
}
```

**Example `config.toml`:**
```toml
rpc_url = "https://rpc.risechain.testnet"
chain_id = 11155111
private_key_file = "wallet-json"
tps = 5
worker_amount = 10
min_delay_ms = 1000
max_delay_ms = 5000

[proxies]
# Optional proxy configuration
```

#### 3.1.3 Spammer Implementation (`src/spammer/mod.rs`)

```rust
pub struct EvmSpammer {
    pub config: SpamConfig,
    pub rise_config: RiseConfig,
    pub wallet: DecryptedWallet,
    pub wallet_id: String,
    pub proxy_id: String,
    pub proxy_url: Option<String>,
    pub db: Option<Arc<DatabaseManager>>,
    pub gas_manager: Arc<GasManager>,
    pub tasks: Vec<Box<RiseTask>>,
}

type RiseTask = dyn Task<TaskContext>;

#[async_trait]
impl Spammer for EvmSpammer {
    async fn start(&self, cancellation_token: CancellationToken) -> Result<SpammerStats> {
        let mut success = 0;
        let mut failed = 0;
        
        // Create ethers provider
        let provider = create_provider(&self.config.rpc_url, self.proxy_url.as_ref());
        
        // Create signer
        let signer = self.wallet.evm_private_key.parse::<LocalWallet>()?;
        let chain_id = self.config.chain_id;
        
        // Create TaskContext
        let ctx = TaskContext {
            provider: provider.clone(),
            wallet: signer.clone(),
            config: self.rise_config.clone(),
            proxy: self.proxy_url.clone(),
            db: self.db.clone(),
            gas_manager: self.gas_manager.clone(),
        };
        
        // Infinite loop until cancelled
        while !cancellation_token.is_cancelled() {
            // Select random task
            let task_index = rand::thread_rng().gen_range(0..self.tasks.len());
            let task = &self.tasks[task_index];
            
            // Execute task
            let start_time = Instant::now();
            match task.run(ctx.clone()).await {
                Ok(result) => {
                    let duration = start_time.elapsed();
                    
                    if result.success {
                        success += 1;
                        // Log success with colors
                        info!(target: "smart_main", 
                            "[WK:{}][WL:{}][P:{}] \x1b[92mSuccess\x1b[0m {} {} (B: {}) in {:.1?}",
                            worker_id, wallet_id, proxy_id,
                            task.name(), result.message,
                            block_number, duration
                        );
                    } else {
                        failed += 1;
                        // Log failure
                        warn!(target: "smart_main",
                            "[WK:{}][WL:{}][P:{}] \x1b[91mFailed\x1b[0m {}: {}",
                            worker_id, wallet_id, proxy_id,
                            task.name(), result.message
                        );
                    }
                    
                    // Log to database
                    if let Some(db) = &self.db {
                        db.log_task_result(
                            result.clone(),
                            task.name(),
                            &signer.address().to_string(),
                            &self.wallet_id,
                            duration.as_millis() as u64,
                        ).await?;
                    }
                }
                Err(e) => {
                    failed += 1;
                    warn!(target: "smart_main",
                        "[WK:{}][WL:{}][P:{}] \x1b[91mFailed\x1b[0m {}: {}",
                        worker_id, wallet_id, proxy_id,
                        task.name(), e
                    );
                }
            }
            
            // Sleep with cancellation check
            let delay_ms = rand::thread_rng()
                .gen_range(self.rise_config.min_delay_ms.unwrap_or(0)..self.rise_config.max_delay_ms.unwrap_or(1000));
            
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                _ = cancellation_token.cancelled() => break,
            }
        }
        
        Ok(SpammerStats { success, failed })
    }
    
    fn stop(&self) {
        // Implementation for stop signal
    }
}
```

#### 3.1.4 Task System (`src/task/`)

All tasks implement the `Task<TaskContext>` trait from core-logic.

**TaskContext Struct:**
```rust
pub struct TaskContext {
    pub provider: Provider<Http>,              // Ethereum JSON-RPC provider
    pub wallet: LocalWallet,                   // Signed wallet for transactions
    pub config: RiseConfig,                    // Full configuration
    pub proxy: Option<String>,                 // Optional proxy URL
    pub db: Option<Arc<DatabaseManager>>,     // Database reference
    pub gas_manager: Arc<GasManager>,         // Gas fee manager
}
```

##### 3.1.4.1 CheckBalanceTask (`check_balance.rs`)

**Purpose**: Queries and reports the wallet's ETH balance.

**Task Name**: `"CheckBalance"`

**Execution Flow:**
1. Create Balance request using `provider.get_balance()`
2. Await response from RPC
3. Format balance as ETH (wei to ether conversion)
4. Return TaskResult with balance information

**Input**: None (uses wallet from context)

**Output**:
```rust
TaskResult {
    success: true,
    message: format!("Balance: {} ETH", balance_eth),
    tx_hash: None,
}
```

##### 3.1.4.2 ClaimFaucetTask / SendETH (`claim_faucet.rs`)

**Purpose**: Sends 10-15% of wallet balance to a random recipient from address.txt.

**Task Name**: `"Faucet"`

**Execution Flow:**
1. Read recipient addresses from `address.txt`
2. Select random recipient
3. Get current wallet balance
4. Calculate send amount (10-15% of balance)
5. Build TransactionRequest (to, value, data)
6. Get gas fees from GasManager
7. Send transaction with `provider.send_transaction()`
8. Await transaction receipt

**Input**: `address.txt` file with one address per line

**Output**:
```rust
TaskResult {
    success: true,
    message: format!("Sent {} ETH to {}", amount_eth, recipient),
    tx_hash: Some(tx_hash),
}
```

**Gas**: Standard transfer (21,000 gas limit)

##### 3.1.4.3 DeployContractTask (`deploy_contract.rs`)

**Purpose**: Deploys the Counter contract to the blockchain.

**Task Name**: `"DeployContract"`

**Execution Flow:**
1. Get Counter bytecode from `contracts/mod.rs`
2. Build contract deployment transaction
3. Get gas fees from GasManager
4. Send deployment transaction
5. Wait for receipt
6. Extract contract address from receipt
7. Log contract to database

**Input**: Counter bytecode (hardcoded in contracts/mod.rs)

**Output**:
```rust
TaskResult {
    success: true,
    message: format!("Counter deployed at {}", contract_address),
    tx_hash: Some(tx_hash),
}
```

**Gas**: Deploy limit (1,200,000 gas)

##### 3.1.4.4 InteractContractTask (`interact_contract.rs`)

**Purpose**: Calls `increment()` on deployed Counter contracts.

**Task Name**: `"InteractContract"`

**Execution Flow:**
1. Query database for Counter contracts deployed by this wallet
2. If no contracts, skip or trigger deploy task
3. Select random deployed contract
4. Create contract instance with ABI
5. Build increment transaction
6. Get gas fees from GasManager
7. Send transaction
8. Wait for receipt

**Input**: Database query for `created_counter_contracts` table

**Output**:
```rust
TaskResult {
    success: true,
    message: format!("Counter incremented at {}", contract_address),
    tx_hash: Some(tx_hash),
}
```

**Gas**: Counter interact limit (50,000 gas)

##### 3.1.4.5 SelfTransferTask (`self_transfer.rs`)

**Purpose**: Sends 0 ETH to self to test transaction execution.

**Task Name**: `"SelfTransfer"`

**Execution Flow:**
1. Build TransactionRequest (to = wallet address, value = 0)
2. Get gas fees from GasManager
3. Send transaction
4. Wait for receipt

**Input**: None

**Output**:
```rust
TaskResult {
    success: true,
    message: "Self-transfer completed".to_string(),
    tx_hash: Some(tx_hash),
}
```

**Gas**: Standard transfer (21,000 gas)

##### 3.1.4.6 CreateMemeTask (`create_meme.rs`)

**Purpose**: Deploys an ERC-20 meme token with random name and symbol.

**Task Name**: `"CreateMeme"`

**Execution Flow:**
1. Generate random name and symbol
2. Get meme token bytecode from `contracts/mod.rs`
3. Build constructor arguments (name, symbol)
4. Build deployment transaction
5. Get gas fees from GasManager
6. Send transaction
7. Wait for receipt
8. Log asset to database

**Input**: None (generates random metadata)

**Output**:
```rust
TaskResult {
    success: true,
    message: format!("Meme token {} ({}) deployed at {}", name, symbol, address),
    tx_hash: Some(tx_hash),
}
```

**Gas**: Deploy limit (1,200,000 gas)

##### 3.1.4.7 SendMemeTokenTask (`send_meme.rs`)

**Purpose**: Sends 1% of owned meme tokens to a random recipient.

**Task Name**: `"SendMeme"`

**Execution Flow:**
1. Query database for meme tokens created by this wallet
2. If no tokens, skip or trigger create task
3. Select random token
4. Read recipient addresses from `address.txt`
5. Select random recipient
6. Calculate send amount (1% of balance)
7. Build ERC-20 transfer transaction
8. Get gas fees from GasManager
9. Send transaction
10. Wait for receipt

**Input**: Database query for `created_assets` table, `address.txt`

**Output**:
```rust
TaskResult {
    success: true,
    message: format!("Sent {} {} to {}", amount, symbol, recipient),
    tx_hash: Some(tx_hash),
}
```

**Gas**: Meme send limit (100,000 gas)

#### 3.1.5 Gas Manager (`src/utils/gas.rs`)

```rust
pub struct GasManager {
    provider: Arc<Provider<Http>>,
    max_gwei: f64,        // Default: 0.000000009 (90 Wei)
    priority_gwei: f64,   // Default: 0.000000001 (1 Wei)
}

impl GasManager {
    pub const LIMIT_DEPLOY: U256 = U256::from(1_200_000);
    pub const LIMIT_TRANSFER: U256 = U256::from(21_000);
    pub const LIMIT_COUNTER_INTERACT: U256 = U256::from(50_000);
    pub const LIMIT_SEND_MEME: U256 = U256::from(100_000);
    
    pub async fn get_fees(&self) -> Result<(U256, U256)> {
        // Fetch latest block for base_fee_per_gas
        let block = self.provider.get_block(BlockNumber::Latest).await?;
        let base_fee = block.base_fee_per_gas.unwrap_or_default();
        
        // Calculate priority fee in Wei
        let priority_fee_wei = parse_units(self.priority_gwei, "gwei")?;
        
        // Calculate max fee: base_fee + priority_fee
        let max_fee_wei = base_fee + priority_fee_wei;
        
        // Cap at configured maximum
        let max_configured = parse_units(self.max_gwei, "gwei")?;
        let final_max = max_fee_wei.min(max_configured);
        
        Ok((final_max, priority_fee_wei))
    }
    
    pub fn get_limit(&self, transaction_type: &str) -> U256 {
        match transaction_type {
            "deploy" => Self::LIMIT_DEPLOY,
            "transfer" => Self::LIMIT_TRANSFER,
            "counter_interact" => Self::LIMIT_COUNTER_INTERACT,
            "send_meme" => Self::LIMIT_SEND_MEME,
            _ => Self::LIMIT_TRANSFER,
        }
    }
}
```

**EIP-1559 Fee Calculation:**
1. Fetch latest block to get `base_fee_per_gas`
2. Add priority fee to base fee for max fee
3. Cap at configured `max_gwei` to prevent overpaying
4. Return (max_fee, priority_fee) tuple

#### 3.1.6 Contract Definitions (`src/contracts/mod.rs`)

**Counter Contract:**
- Simple incrementer contract
- Has `increment()` function that increases stored value
- Bytecode and ABI for deployment and interaction

**Meme Token Contract:**
- Basic ERC-20 token
- Constructor takes name and symbol
- Standard transfer functionality

#### 3.1.7 Debug Task (`src/bin/debug_task.rs`)

The debug task binary provides interactive CLI for testing individual tasks.

```rust
#[derive(Parser)]
#[command(name = "debug_task")]
struct Args {
    #[arg(short, long, default_value = "chains/risechain/config.toml")]
    config: String,
    
    #[arg(short, long)]
    all: bool,  // Check all wallets
    
    #[arg(short, long)]
    task: Option<usize>,  // Run specific task (0-6)
}

fn main() -> Result<()> {
    // 1. Load config
    // 2. Decrypt wallet(s)
    // 3. If --all: check balance for all wallets
    // 4. If --task N: run specific task
    // 5. If neither: prompt for interactive use
}
```

**Usage Examples:**
```powershell
# Check balance for all wallets
$env:WALLET_PASSWORD="pwd"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml --all

# Run specific task (e.g., task 1 = CheckBalance)
.\target_final\debug\debug_task.exe --config chains/risechain/config.toml --task 1

# Interactive mode (prompts)
$env:WALLET_PASSWORD="pwd"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml
```

#### 3.1.8 Task Index

| Index | Task Name | File | Purpose |
|-------|-----------|------|---------|
| 0 | Faucet | `claim_faucet.rs` | Send 10-15% balance to random address |
| 1 | CheckBalance | `check_balance.rs` | Query wallet ETH balance |
| 2 | DeployContract | `deploy_contract.rs` | Deploy Counter contract |
| 3 | InteractContract | `interact_contract.rs` | Call increment() on Counter |
| 4 | SelfTransfer | `self_transfer.rs` | Send 0 ETH to self |
| 5 | CreateMeme | `create_meme.rs` | Deploy ERC-20 meme token |
| 6 | SendMeme | `send_meme.rs` | Send 1% of meme tokens |

### 3.2 EVM-Project Template (`chains/evm-project/`)

EVM-Project is a generic template/skeleton for creating new EVM chain implementations. It demonstrates the pattern without implementing full business logic.

#### 3.2.1 Comparison with RiseChain

| Aspect | EVM-Project (Generic) | RiseChain (Production) |
|--------|----------------------|------------------------|
| **Purpose** | Template/skeleton for new chains | Full-featured for RISE testnet |
| **Task System** | No tasks (mock spam loop) | 7 different task types |
| **Complexity** | ~100 lines core logic | ~500+ lines with tasks |
| **Gas Management** | Not implemented | Full GasManager with EIP-1559 |
| **Database** | Not implemented | SQLite persistence |
| **Configuration** | 4 fields only | Extended with delays, workers |
| **Proxy Assignment** | Round-robin | Random selection |

#### 3.2.2 Simplified Main (`src/main.rs`)

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // 1. Parse CLI args
    let config_path = std::env::args().nth(1).unwrap();
    
    // 2. Load simple EVM config
    let config = load_config(&config_path)?;
    
    // 3. Load wallets and proxies
    let wallets = load_wallets()?;
    let proxies = ProxyManager::load_proxies();
    
    // 4. Create generic spammers
    let mut spammers: Vec<Box<dyn Spammer>> = Vec::new();
    
    for (i, wallet) in wallets.iter().enumerate() {
        let proxy = proxies.get(i % proxies.len()).cloned();
        
        let spammer = EvmSpammer {
            config: config.clone(),
            wallet: wallet.clone(),
            wallet_id: format!("{:03}", i),
            proxy_url: proxy.map(|p| p.url),
        };
        
        spammers.push(Box::new(spammer));
    }
    
    // 5. Run spammers
    WorkerRunner::run_spammers(spammers).await?;
    
    Ok(())
}
```

#### 3.2.3 Mock Spammer (`src/spammer/mod.rs`)

```rust
pub struct EvmSpammer {
    config: SpamConfig,
    wallet: DecryptedWallet,
    wallet_id: String,
    proxy_url: Option<String>,
}

#[async_trait]
impl Spammer for EvmSpammer {
    async fn start(&self, cancellation_token: CancellationToken) -> Result<SpammerStats> {
        let mut count = 0;
        
        while !cancellation_token.is_cancelled() {
            // Mock spam loop - just increment counter
            // In real implementation, would send actual transactions
            
            count += 1;
            
            info!("Mock spam iteration {} for wallet {}", count, self.wallet_id);
            
            // Rate limiting based on TPS
            let delay_ms = 1000 / self.config.target_tps.max(1);
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        
        Ok(SpammerStats { success: count, failed: 0 })
    }
}
```

#### 3.2.4 Minimal Config (`src/config.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    pub private_key_file: String,
    pub tps: u32,
}

impl EvmConfig {
    pub fn from_path(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|e| anyhow!("{}", e))
    }
}
```

#### 3.2.5 Purpose and Usage

EVM-Project serves as a **starting template**. To make it production-ready for a new chain:

1. Copy `chains/evm-project` to new directory
2. Add task system (reference RiseChain's `src/task/`)
3. Implement actual transaction signing (`SignerMiddleware`)
4. Add chain-specific utilities (gas estimation, contract ABIs)
5. Add database persistence if needed
6. Extend configuration with chain-specific parameters

---

## 4. Configuration Files

### 4.1 RiseChain Config (`chains/risechain/config.toml`)

```toml
# RISE Chain Configuration

# RPC Configuration
rpc_url = "https://rpc.risechain.testnet"
chain_id = 11155111

# Wallet Configuration
private_key_file = "wallet-json"

# Performance Configuration
tps = 5                              # Target transactions per second
worker_amount = 10                   # Number of concurrent workers (optional, defaults to wallet count)

# Delay Configuration (milliseconds)
min_delay_ms = 1000                  # Minimum delay between tasks
max_delay_ms = 5000                  # Maximum delay between tasks

# Proxy Configuration (optional)
# If omitted, will load from proxies.txt
# [[proxies]]
# url = "http://ip:port"
# username = "user"
# password = "pass"
```

### 4.2 EVM-Project Config (`chains/evm-project/config.toml`)

```toml
# Generic EVM Configuration

rpc_url = "https://rpc.example-chain.testnet"
chain_id = 11155111
private_key_file = "wallet-json"
tps = 10
```

### 4.3 Proxies File (`proxies.txt`)

**Format Specification:**
```
# Comments start with #
# Empty lines are ignored

# Full authentication
ip:port:username:password

# No authentication
ip:port

# Examples:
192.168.1.1:8080:user:pass
10.0.0.1:3128
proxy.example.com:443:user:pass
```

**Parsing Rules:**
- Split each line by `:`
- Parts 0-1: host and port (required)
- Parts 2-3: username and password (optional)
- Lines starting with `#` are comments
- Empty lines are skipped

### 4.4 Address File (`address.txt`)

**Format:** One Ethereum address per line

```
0x1234567890123456789012345678901234567890
0x0987654321098765432109876543210987654321
0xabcd...
```

**Used By:** SendETH / Faucet task for selecting random recipients

### 4.5 Environment Variables

| Variable | Purpose | Required |
|----------|---------|----------|
| `WALLET_PASSWORD` | Password to decrypt JSON wallets | Yes (unless using raw keys) |
| `RUST_BACKTRACE=1` | Enable stack traces for crashes | No (debug only) |
| `RUST_LOG=debug` | Enable verbose logging | No (defaults to INFO) |

---

## 5. Data Flow Diagrams

### 5.1 Wallet Decryption Flow

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Startup                       │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│              WalletManager::new()                            │
│  1. Scan wallet-json/ directory                             │
│  2. Find all .json files                                    │
│  3. Create WalletSource::JsonFile for each                  │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│           get_wallet(index, password)                        │
│  1. Check cache (Mutex<HashMap>)                            │
│  2. If cached → return immediately                           │
│  3. If not cached → decrypt_json_wallet()                   │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│           decrypt_json_wallet(path, password)                │
│  1. Read JSON file                                          │
│  2. Extract encrypted.ciphertext, iv, salt, tag             │
│  3. Call SecurityUtils::decrypt_components()                │
│     ┌─────────────────────────────────────┐                 │
│     │ SecurityUtils::decrypt_components() │                 │
│     │  - Hex decode inputs                │                 │
│     │  - Scrypt key derivation            │                 │
│     │    N=16384, r=8, p=1, dkLen=32     │                 │
│     │  - AES-256-GCM decryption           │                 │
│     │  - Verify authentication tag        │                 │
│     └─────────────────────────────────────┘                 │
│  4. Parse decrypted JSON to DecryptedWallet                 │
│  5. Store in cache                                          │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                    DecryptedWallet                           │
│  - mnemonic, evm_private_key, evm_address                   │
│  - sol_private_key, sol_address                             │
│  - sui_private_key, sui_address                             │
│  - ZeroizeOnDrop for memory sanitization                    │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 Task Execution Flow

```
┌─────────────────────────────────────────────────────────────┐
│                      main()                                  │
│  1. Load config.toml                                        │
│  2. Decrypt wallets (WALLET_PASSWORD env var)               │
│  3. Load proxies.txt                                        │
│  4. Initialize SQLite database (rise.db)                    │
│  5. Create EvmSpammer for each wallet                       │
│  6. WorkerRunner::run_spammers()                            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│              WorkerRunner::run_spammers()                    │
│  1. Create JoinSet for task management                      │
│  2. Create CancellationToken                                │
│  3. Spawn Ctrl+C listener task                              │
│  4. For each spammer:                                       │
│     - Create tracing span with worker_id                    │
│     - Spawn spammer.start(token) on JoinSet                 │
│  5. While loop: JoinSet::join_next()                        │
│  6. Aggregate success/failed counts                         │
│  7. Return final SpammerStats                               │
└──────────────────────────┬──────────────────────────────────┘
                              │
          ┌───────────────────┼───────────────────┐
          │                   │                   │
          ▼                   ▼                   ▼
   ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
   │  Spammer #1  │   │  Spammer #2  │   │  Spammer #N  │
   │  (Wallet 1)  │   │  (Wallet 2)  │   │  (Wallet N)  │
   └──────────────┘   └──────────────┘   └──────────────┘
          │                   │                   │
          ▼                   ▼                   ▼
   ┌──────────────────────────────────────────────────────┐
   │           EvmSpammer::start()                        │
   │  Loop forever until cancelled:                       │
   │    1. Check cancellation_token.is_cancelled()        │
   │    2. Randomly select task from tasks vector         │
   │    3. Build TaskContext (provider, wallet, config)   │
   │    4. Execute task.run(ctx).await                    │
   │    5. On success: log to DB, emit success log        │
   │    6. On failure: log error, emit failed log         │
   │    7. Sleep delay_ms with tokio::select!             │
   └──────────────────────────────────────────────────────┘
                         │         │         │
        ┌────────────────┼─────────┼─────────┐
        │                │         │         │
        ▼                ▼         ▼         ▼
   ┌─────────┐    ┌──────────┐ ┌───────┐ ┌───────────┐
   │sendETH  │    │checkBal  │ │deploy │ │interact   │
   │(Faucet) │    │          │ │       │ │(Counter)  │
   └─────────┘    └──────────┘ └───────┘ └───────────┘
                                              │
                                              ▼
                                        ┌───────────┐
                                        │  Database │
                                        │ (rise.db) │
                                        └───────────┘
                                        │           │
                                        ▼           ▼
                                   task_metrics  created_counter
                                   created_assets  contracts
```

### 5.3 Concurrent Execution Model

```
┌─────────────────────────────────────────────────────────────┐
│                    Main Thread (tokio::main)                 │
│                                                              │
│  WorkerRunner::run_spammers()                               │
│         │                                                    │
│         ▼                                                    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  JoinSet::new()                                     │    │
│  │  + CancellationToken::new()                         │    │
│  └─────────────────────────────────────────────────────┘    │
│         │                                                    │
│         ▼                                                    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Spawn Ctrl+C Listener                              │    │
│  │  ┌─────────────────────────────────────────────┐    │    │
│  │  │ async move {                                 │    │    │
│  │  │   tokio::signal::ctrl_c().await;            │    │    │
│  │  │   cancellation_token.cancel();              │    │    │
│  │  │ }                                            │    │    │
│  │  └─────────────────────────────────────────────┘    │    │
│  └─────────────────────────────────────────────────────┘    │
│         │                                                    │
│         ▼                                                    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Spawn N Spammer Tasks (one per wallet)             │    │
│  │                                                     │    │
│  │  for wallet in wallets {                            │    │
│  │    join_set.spawn(async move {                      │    │
│  │      spammer.start(cancellation_token).await        │    │
│  │    });                                              │    │
│  │  }                                                  │    │
│  │                                                     │    │
│  │  Each task runs EvmSpammer::start() in parallel    │    │
│  └─────────────────────────────────────────────────────┘    │
│         │                                                    │
│         ▼                                                    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Wait for Completion                                │    │
│  │                                                     │    │
│  │  while let Some(result) = join_set.join_next().await│    │
│  │    Aggregate stats                                  │    │
│  │                                                     │    │
│  └─────────────────────────────────────────────────────┘    │
│         │                                                    │
│         ▼                                                    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Final Stats                                        │    │
│  │  Print: Success: X, Failed: Y, Duration: Z          │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
         │
         │ Each spammer task:
         ▼
   ┌─────────────────────────────────────────────────────┐
   │           EvmSpammer Task (per wallet)               │
   │                                                      │
   │  loop {                                              │
   │    if cancellation_token.is_cancelled() { break }   │
   │                                                      │
   │    // Select and execute task                        │
   │    let task = select_random_task();                  │
   │    let result = task.run(ctx).await;                 │
   │                                                      │
   │    // Log result                                     │
   │    log_result(result);                               │
   │                                                      │
   │    // Sleep with cancellation check                  │
   │    tokio::select! {                                  │
   │      _ = tokio::time::sleep(delay) => {}             │
   │      _ = cancellation_token.cancelled() => break    │
   │    }                                                 │
   │  }                                                   │
   │                                                      │
   └─────────────────────────────────────────────────────┘
```

---

## 6. Security Considerations

### 6.1 Encryption Specifications

**Algorithm**: AES-256-GCM

**Key Derivation**: Scrypt
- N (cost parameter): 16384
- r (block size): 8
- p (parallelization): 1
- dkLen (derived key length): 32 bytes

**Authentication**: GCM authentication tag (16 bytes)

**Input Format**:
- Hex-encoded ciphertext
- Hex-encoded IV (12 bytes)
- Hex-encoded salt (used in key derivation)
- Hex-encoded authentication tag (16 bytes)

### 6.2 Memory Security

**ZeroizeOnDrop Trait**:
The `DecryptedWallet` struct implements `ZeroizeOnDrop` to automatically sanitize memory when the struct goes out of scope.

```rust
use zeroize::ZeroizeOnDrop;

#[derive(ZeroizeOnDrop)]
pub struct DecryptedWallet {
    pub mnemonic: String,
    pub evm_private_key: String,
    // ... other fields
}
```

**Benefits**:
- No manual memory clearing required
- Automatic when struct is dropped, moved, or shadowed
- Prevents sensitive data from remaining in memory

### 6.3 Debug Output Redaction

The `Debug` implementation for `DecryptedWallet` redacts all sensitive fields:

```rust
impl fmt::Debug for DecryptedWallet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecryptedWallet")
            .field("mnemonic", &"***REDACTED***")
            .field("evm_private_key", &"***REDACTED***")
            .field("evm_address", &self.evm_address)
            // ... other public fields
            .finish()
    }
}
```

### 6.4 Password Handling

**Environment Variable**:
```bash
$env:WALLET_PASSWORD="your_password"
```

**Interactive Prompt** (if env var not set):
- Uses secure input (no echo)
- Password not displayed on screen
- Only used for decryption, not stored

### 6.5 Sensitive Data Exposure Prevention

**Never Log**:
- Private keys
- Mnemonic phrases
- Passwords
- Unencrypted wallet files
- RPC URLs with authentication

**Always Use**:
- `***REDACTED***` in debug output
- Masked proxy credentials
- Checksummed addresses (not private keys)

### 6.6 Best Practices

1. **Environment Variables**: Always use `WALLET_PASSWORD` env var instead of hardcoding
2. **Zeroize**: Always use `ZeroizeOnDrop` for structs containing sensitive data
3. **Debug Impl**: Always redact sensitive fields in `Debug` implementations
4. **Memory Management**: Keep decrypted wallets in memory only as long as needed
5. **File Permissions**: Set restrictive permissions on wallet files (600)
6. **No Secrets in Logs**: Verify logs don't contain private keys or passwords

---

## 7. Build and Run Reference

### 7.1 Build Commands

**Full Workspace Build**:
```powershell
._clean_and_compile_all.bat
```

This script:
1. Cleans `target_final` directory
2. Compiles all workspace members in parallel
3. Moves binaries to `target_final/debug`

**Check Logic Only**:
```powershell
cargo check --workspace
```

**Build Specific Crate**:
```powershell
cargo build -p risechain
cargo build -p evm-project
```

### 7.2 Run Commands

**Run Debugger (Check All Balances)**:
```powershell
$env:WALLET_PASSWORD="password"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml --all
```

**Run Debugger (Check Single Wallet)**:
```powershell
$env:WALLET_PASSWORD="password"; .\target_final\debug\debug_task.exe --config chains/risechain/config.toml
```

**Run Debugger (Run Specific Task)**:
```powershell
.\target_final\debug\debug_task.exe --config chains/risechain/config.toml --task 1
```

**Run RISE Spammer**:
```powershell
$env:WALLET_PASSWORD="password"; .\target_final\debug\rise-project.exe --config chains/risechain/config.toml
```

**Run EVM Spammer**:
```powershell
$env:WALLET_PASSWORD="password"; .\target_final\debug\evm-project.exe --config chains/evm-project/config.toml
```

### 7.3 Environment Variables

```powershell
# Set wallet password
$env:WALLET_PASSWORD="your_password"

# Enable backtraces
$env:RUST_BACKTRACE=1

# Verbose logging
$env:RUST_LOG="debug"
```

### 7.4 Common Issues and Solutions

**Issue: Wallet Decryption Fails**
```
Error: Invalid password or corrupted wallet file
```
**Solution**: Verify `WALLET_PASSWORD` is correct and wallet file is valid JSON

**Issue: Build Fails with File Locks**
```
error: could not compile ... (due to previous compilation error)
```
**Solution**: Run `._clean_and_compile_all.bat` to clean and rebuild

**Issue: Database Lock Errors**
```
error: database is locked
```
**Solution**: Ensure no other processes are using rise.db; increase MAX_CONNECTIONS if needed

**Issue: Proxy Authentication Fails**
```
Error: proxy connection refused
```
**Solution**: Verify proxy format in `proxies.txt` and credentials

**Issue: Out of Gas**
```
Error: intrinsic gas too low
```
**Solution**: Increase gas limits in GasManager or adjust transaction parameters

### 7.5 File Locations

| File | Location |
|------|----------|
| Built binaries | `target_final/debug/*.exe` |
| Logs | `logs/smart_main.log` |
| Database | `rise.db` (root directory) |
| Proxies | `proxies.txt` (root directory) |
| Wallets | `wallet-json/*.json` |
| Addresses | `address.txt` (root directory) |

---

## 8. Task Reference

### 8.1 Task Configuration

| Task ID | Name | File | Gas Limit | Input | Output |
|---------|------|------|-----------|-------|--------|
| 0 | Faucet | `claim_faucet.rs` | 21,000 | address.txt | tx_hash |
| 1 | CheckBalance | `check_balance.rs` | 0 (query) | none | balance |
| 2 | DeployContract | `deploy_contract.rs` | 1,200,000 | bytecode | tx_hash, address |
| 3 | InteractContract | `interact_contract.rs` | 50,000 | DB query | tx_hash |
| 4 | SelfTransfer | `self_transfer.rs` | 21,000 | none | tx_hash |
| 5 | CreateMeme | `create_meme.rs` | 1,200,000 | random name/symbol | tx_hash, address |
| 6 | SendMeme | `send_meme.rs` | 100,000 | DB query, address.txt | tx_hash |

### 8.2 Task Selection

Tasks are selected randomly from the available tasks vector:

```rust
let task_index = rand::thread_rng().gen_range(0..self.tasks.len());
let task = &self.tasks[task_index];
```

### 8.3 Task Result Structure

```rust
pub struct TaskResult {
    pub success: bool,              // Whether task succeeded
    pub message: String,            // Human-readable status
    pub tx_hash: Option<String>,    // Transaction hash if applicable
}
```

---

## 9. Database Reference

### 9.1 Connection Pool

```rust
pub struct DatabaseManager {
    pool: SqlitePool,
}

impl DatabaseManager {
    pub const MAX_CONNECTIONS: u32 = 5;
}
```

### 9.2 Query Examples

**Log Task Result**:
```rust
db.log_task_result(
    result,           // TaskResult
    task.name(),      // &str
    wallet_address,   // &str
    worker_id,        // &str
    duration_ms,      // u64
).await?;
```

**Get Assets by Type**:
```rust
let tokens = db.get_assets_by_type(wallet_address, "token").await?;
for token in tokens {
    println!("{} ({}) at {}", token.name, token.symbol, token.address);
}
```

**Update Proxy Stats**:
```rust
// On success
db.update_proxy_stats(proxy_url, true).await?;

// On failure
db.update_proxy_stats(proxy_url, false).await?;
```

---

## 10. Logging Reference

### 10.1 Log Levels

- **ERROR**: Application errors, failed operations
- **WARN**: Warning conditions, recoverable errors
- **INFO**: Informational, major events (task start/stop)
- **DEBUG**: Detailed information for debugging
- **TRACE**: Finest-grained information

### 10.2 Log Format (Terminal)

```
[WK:XXX][WL:XXX][P:XXX] [StatusColor] Message
```

### 10.3 Log Format (File)

```
YYYY-MM-DD HH:MM:SS [LEVEL] [WK:...][WL:...][P:...] message
```

### 10.4 Context Fields

| Field | Abbreviation | Description |
|-------|--------------|-------------|
| worker_id | WK | Spammer worker identifier |
| wallet_id | WL | Wallet being used |
| proxy_id | P | Proxy being used |

### 10.5 Status Colors

| Status | ANSI Code | Color |
|--------|-----------|-------|
| Success | `\x1b[92m` | Light Green |
| Failed | `\x1b[91m` | Light Red |
| Info | (none) | White/Gray |
| Warning | (none) | White/Gray |

---
