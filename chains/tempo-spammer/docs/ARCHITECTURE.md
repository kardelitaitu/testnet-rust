# Architecture Overview

High-level architecture and design of the tempo-spammer.

## Table of Contents
- [System Overview](#system-overview)
- [Component Architecture](#component-architecture)
- [Data Flow](#data-flow)
- [Concurrency Model](#concurrency-model)
- [State Management](#state-management)
- [Security Architecture](#security-architecture)
- [Performance Design](#performance-design)
- [Scalability](#scalability)
- [Failure Handling](#failure-handling)

---

## System Overview

The tempo-spammer is a high-performance transaction spammer for the Tempo blockchain, designed for:

- **High Throughput**: 50+ concurrent workers
- **Reliability**: Automatic retries and failover
- **Observability**: Comprehensive logging and metrics
- **Flexibility**: 50+ pluggable task implementations

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                        Binary Layer                          │
├─────────────────────────────────────────────────────────────┤
│  tempo-spammer │ tempo-debug │ tempo-runner │ tempo-sequence│
└──────────────────┬──────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│                     Client Pool Layer                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │   Wallet 1  │  │   Wallet 2  │  │   Wallet N  │         │
│  │  + Proxy 1  │  │  + Proxy 2  │  │  + Proxy N  │         │
│  └─────────────┘  └─────────────┘  └─────────────┘         │
│                                                              │
│  Features:                                                   │
│  • RAII-based wallet leasing                                 │
│  • Proxy health checking & rotation                          │
│  • 4-second cooldown between uses                           │
└──────────────────┬──────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│                    Client Layer                              │
│                                                              │
│  ┌─────────────────────────────────────────┐                │
│  │           TempoClient                   │                │
│  │  ┌─────────────┐    ┌─────────────┐    │                │
│  │  │   Provider  │    │   Signer    │    │                │
│  │  │  (Alloy)    │    │ (PrivateKey)│    │                │
│  │  └─────────────┘    └─────────────┘    │                │
│  │  ┌─────────────┐    ┌─────────────┐    │                │
│  │  │NonceManager │    │Proxy Config │    │                │
│  │  │  (Optional) │    │  (Optional) │    │                │
│  │  └─────────────┘    └─────────────┘    │                │
│  └─────────────────────────────────────────┘                │
│                                                              │
│  Features:                                                   │
│  • Automatic retry with exponential backoff                  │
│  • Connection pooling                                        │
│  • Nonce caching                                             │
└──────────────────┬──────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│                     Task Layer                               │
│                                                              │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐       │
│  │ Task 01  │ │ Task 02  │ │ Task 03  │ │ Task 50  │       │
│  │ Deploy   │ │ Faucet   │ │ Send     │ │ Storm    │       │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘       │
│                                                              │
│  All tasks implement TempoTask trait:                        │
│  • name() -> &'static str                                    │
│  • run(&TaskContext) -> Result<TaskResult>                  │
└──────────────────┬──────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│                    Support Layer                             │
│                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │   Config    │  │  Database   │  │    Logs     │         │
│  │   (TOML)    │  │  (SQLite)   │  │ (Tracing)   │         │
│  └─────────────┘  └─────────────┘  └─────────────┘         │
│                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │ProxyHealth  │  │ GasManager  │  │  Utilities  │         │
│  │  (Health)   │  │   (Fees)    │  │  (Helpers)  │         │
│  └─────────────┘  └─────────────┘  └─────────────┘         │
└─────────────────────────────────────────────────────────────┘
```

---

## Component Architecture

### 1. Binary Layer

**Purpose**: Entry points for different use cases

| Binary | Purpose | Use Case |
|--------|---------|----------|
| `tempo-spammer` | Main multi-worker spammer | Production load testing |
| `tempo-debug` | Single task testing | Development & debugging |
| `tempo-runner` | Sequential execution | Controlled testing |
| `tempo-sequence` | Sequence execution | Ordered task execution |
| `debug_proxy` | Proxy diagnostics | Proxy troubleshooting |

### 2. Client Pool Layer

**Purpose**: Manage multiple wallets with concurrency control

**Key Components:**
- **Wallet Manager**: Loads and decrypts wallet keys
- **Client Cache**: Stores created clients for reuse
- **Locked Set**: Tracks which wallets are in use
- **Proxy Assignment**: Rotates proxies across wallets

**Design Pattern**: RAII (Resource Acquisition Is Initialization)

```rust
// Usage pattern:
if let Some(lease) = pool.try_acquire_client().await {
    // Use client
    let result = task.run(&ctx).await;
} // Automatically released after 4s cooldown
```

### 3. Client Layer

**Purpose**: Blockchain interaction with resilience

**Key Components:**
- **Provider**: Alloy RPC client with retry logic
- **Signer**: Local wallet for transaction signing
- **Nonce Manager**: Optional nonce caching
- **Proxy Config**: HTTP proxy settings

**Resilience Features:**
- Exponential backoff (5 retries, 100ms-2000ms)
- Connection pooling
- Automatic nonce synchronization

### 4. Task Layer

**Purpose**: Pluggable task system

**Architecture:**
```rust
#[async_trait]
pub trait TempoTask: Send + Sync {
    fn name(&self) -> &'static str;
    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult>;
}
```

**Task Categories:**
- Core (01-10): Basic operations
- Token (21-30): Token ecosystem
- Batch (24-27, 43-44): Multi-transactions
- Multi-Send (28-33): Distribution
- Advanced (45-50): Viral mechanics

### 5. Support Layer

**Purpose**: Infrastructure and utilities

**Components:**
- **Config**: TOML-based configuration
- **Database**: SQLite persistence
- **Logging**: Structured tracing
- **Proxy Health**: Health checking & banning
- **Gas Manager**: Fee estimation

---

## Data Flow

### Transaction Submission Flow

```
1. Worker Loop
   ↓
2. Select Task (weighted random)
   ↓
3. Acquire Client (ClientPool)
   ├─ Check available wallets
   ├─ Filter banned proxies
   ├─ Random selection
   └─ Lock wallet
   ↓
4. Create TaskContext
   ├─ Clone client
   ├─ Add config
   ├─ Add database reference
   └─ Add gas manager
   ↓
5. Execute Task
   ├─ Build transaction
   ├─ Get nonce (cache or RPC)
   ├─ Sign transaction
   ├─ Submit to RPC
   └─ Wait for receipt
   ↓
6. Process Result
   ├─ Log to console
   ├─ Log to database
   └─ Update metrics
   ↓
7. Release Client
   └─ 4-second cooldown
```

### Database Logging Flow

```
Task Execution
   ↓
Success/Failure
   ↓
Log to SQLite
   ├─ task_metrics table
   │  ├─ worker_id
   │  ├─ wallet_address
   │  ├─ task_name
   │  ├─ status
   │  ├─ duration_ms
   │  └─ timestamp
   │
   ├─ created_assets (if applicable)
   │  ├─ wallet_address
   │  ├─ asset_address
   │  ├─ asset_type
   │  └─ metadata
   │
   └─ created_counter_contracts (if applicable)
      ├─ wallet_address
      ├─ contract_address
      └─ chain_id
```

---

## Concurrency Model

### Thread Safety Strategy

| Component | Synchronization | Pattern |
|-----------|----------------|---------|
| ClientPool | `RwLock` + `Mutex` | Many readers, exclusive writer |
| NonceManager | `Mutex` | Exclusive access |
| ProxyBanlist | `RwLock` | Read-heavy, occasional writes |
| Database | Connection pool | 5 max connections |

### Worker Concurrency

```rust
// Spawn workers
for i in 0..worker_count {
    let pool = pool.clone();
    let token = cancellation_token.clone();
    
    tokio::spawn(async move {
        worker_loop(i, pool, token).await
    });
}

// Each worker:
loop {
    // 1. Acquire client (may wait if none available)
    let lease = pool.try_acquire_client().await?;
    
    // 2. Select and execute task
    let task = select_weighted_task(&tasks);
    let result = task.run(&ctx).await;
    
    // 3. Log and release
    log_result(&result).await;
    // Client auto-released when lease drops
}
```

### Resource Limits

| Resource | Limit | Purpose |
|----------|-------|---------|
| Workers | Configurable | Concurrency control |
| DB Connections | 5 | Prevent SQLite locks |
| Proxy Concurrent | 50 | Health check limit |
| HTTP Timeout | 30s | Prevent hung requests |
| Task Timeout | 180s | Prevent stuck tasks |

---

## State Management

### State Types

#### 1. Ephemeral State (In-Memory)

**Nonce Cache:**
```rust
pub struct NonceManager {
    nonces: Mutex<HashMap<Address, u64>>, // address -> next_nonce
}
```

**Locked Wallets:**
```rust
pub struct ClientPool {
    locked_wallets: Mutex<HashSet<usize>>, // indices of in-use wallets
}
```

**Proxy Banlist:**
```rust
pub struct ProxyBanlist {
    banned: RwLock<HashMap<usize, Instant>>, // proxy_idx -> ban_time
}
```

#### 2. Persistent State (SQLite)

**Task Metrics:**
```sql
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

**Created Assets:**
```sql
CREATE TABLE created_assets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_address TEXT NOT NULL,
    asset_address TEXT NOT NULL,
    asset_type TEXT NOT NULL,
    name TEXT,
    symbol TEXT,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### State Consistency

**Nonce Synchronization:**
```
1. First call: Cache miss → Fetch from RPC → Store in cache
2. Subsequent: Cache hit → Return nonce → Increment cache
3. On "nonce too low" error: Reset cache → Force RPC fetch
```

**Wallet Leasing:**
```
1. Worker requests client
2. Pool checks locked_wallets set
3. If available: Add to locked set → Return lease
4. On lease drop: Remove from locked set after 4s
```

---

## Security Architecture

### Key Management

**Wallet Storage:**
- Encrypted JSON files (AES-256-GCM)
- Password from environment variable
- Never logged or exposed
- Zeroized on drop

**Access Pattern:**
```
WALLET_PASSWORD (env) → Decrypt JSON → PrivateKey → Signer
```

### Data Protection

**Sensitive Data:**
- Private keys: Never logged
- Passwords: Environment only
- Proxy credentials: Memory only
- Database: Local file, no remote access

**Logging:**
```rust
// Safe - only logs address
info!("Wallet: {:?}", client.address());

// Unsafe - NEVER do this
info!("Private key: {:?}", private_key); // DON'T
```

### Network Security

**Proxy Support:**
- HTTP/HTTPS proxies
- Optional authentication
- Health checking prevents using bad proxies

**RPC Communication:**
- TLS encryption (HTTPS)
- No certificate pinning (flexibility)
- Retry logic handles transient failures

---

## Performance Design

### Optimization Strategies

#### 1. Connection Pooling

```rust
// HTTP clients cached per proxy
http_clients: RwLock<HashMap<Option<String>, reqwest::Client>>

// Benefits:
// - Connection reuse
// - Reduced TCP handshake overhead
// - Better throughput
```

#### 2. Nonce Caching

```rust
// Without caching:
// 1. Get nonce from RPC (100ms)
// 2. Send transaction (50ms)
// Total: 150ms per tx

// With caching:
// 1. Get nonce from memory (1μs)
// 2. Send transaction (50ms)
// Total: 50ms per tx
// → 3x improvement
```

#### 3. Lazy Initialization

```rust
// Clients created on first use
pub async fn get_or_create_client(&self, index: usize) -> Result<TempoClient> {
    // Check cache first
    if let Some(client) = clients.read().await.get(&index) {
        return Ok(client.clone());
    }
    
    // Create if not exists
    let client = create_client().await?;
    clients.write().await.insert(index, client.clone());
    Ok(client)
}
```

#### 4. Batched Operations

Tasks 24-27, 43-44 implement batch operations:
- Multiple operations in single transaction
- Reduced gas costs
- Atomic execution

### Performance Metrics

**Target TPS (Transactions Per Second):**

| Workers | Expected TPS | Latency |
|---------|--------------|---------|
| 1 | 5-10 | 100-200ms |
| 10 | 50-100 | 100-200ms |
| 50 | 200-400 | 150-300ms |
| 100 | 400-800 | 200-500ms |

**Factors affecting TPS:**
- Network latency
- RPC rate limits
- Gas price (affects confirmation time)
- Task complexity
- Proxy performance

---

## Scalability

### Horizontal Scaling

**Current Limitations:**
- Single process
- SQLite database (single writer)
- Limited by wallet count

**Scaling Strategies:**

1. **Multiple Instances:**
   ```bash
   # Run multiple spammer instances
   # Each with subset of wallets
   ./spammer --wallets 0-10 &
   ./spammer --wallets 11-20 &
   ./spammer --wallets 21-30 &
   ```

2. **Database Sharding:**
   - Each instance uses separate database
   - Aggregate metrics externally

3. **RPC Load Balancing:**
   - Multiple RPC endpoints
   - Round-robin or health-based selection

### Vertical Scaling

**Resource Limits:**

| Resource | Limit | Bottleneck |
|----------|-------|------------|
| CPU | 100% | Signature generation |
| Memory | ~500MB | Client cache |
| Network | 100Mbps | RPC throughput |
| File Descriptors | 10k | HTTP connections |

**Optimization:**
- Increase `worker_count` until CPU saturated
- Monitor memory usage
- Use connection pooling

---

## Failure Handling

### Failure Types

#### 1. Transaction Failures

**Nonce Too Low:**
```rust
match error {
    ProviderError::NonceTooLow => {
        client.reset_nonce_cache().await;
        retry_transaction().await
    }
    _ => Err(error),
}
```

**Insufficient Funds:**
- Log error
- Continue with other tasks
- Consider claiming from faucet

**Revert:**
- Log revert reason
- Mark task as failed
- Update database

#### 2. Network Failures

**RPC Timeout:**
- Retry with exponential backoff
- Switch to backup RPC (if configured)
- Log for monitoring

**Proxy Failure:**
- Ban proxy for 30 minutes
- Retry with different proxy
- Fall back to direct connection

#### 3. System Failures

**Database Lock:**
- Retry with backoff
- Log warning
- Continue without logging

**Memory Exhaustion:**
- Limit worker count
- Clear caches periodically
- Monitor and alert

### Recovery Strategies

**Automatic:**
- Nonce cache reset
- Proxy rotation
- Transaction retry

**Manual:**
- Restart spammer
- Clear database locks
- Reset proxy banlist

**Graceful Degradation:**
- Continue without database
- Use direct connection (no proxy)
- Reduce worker count

---

## Design Decisions

### Why SQLite?

**Pros:**
- Zero configuration
- Single file
- Good enough for metrics
- No external dependencies

**Cons:**
- Single writer
- Not distributed
- Limited concurrency

**Alternative Considered:** PostgreSQL
- Rejected: Too complex for local metrics

### Why Alloy?

**Pros:**
- Modern Rust Ethereum library
- Async-first design
- Good performance
- Active development

**Cons:**
- Newer (less mature than ethers-rs)
- API changes

**Alternative Considered:** ethers-rs
- Rejected: Slower, blocking API

### Why RAII for Wallet Leasing?

**Benefits:**
- Automatic cleanup
- No manual release calls
- Exception-safe

**Trade-offs:**
- 4-second cooldown adds latency
- But prevents nonce races

---

## Future Architecture

### Planned Improvements

1. **WebSocket Support:**
   - Real-time transaction monitoring
   - Reduced latency

2. **Distributed Mode:**
   - Multiple instances coordination
   - Centralized metrics

3. **Plugin System:**
   - Dynamic task loading
   - Hot reloading

4. **REST API:**
   - Remote control
   - Status monitoring

5. **Metrics Export:**
   - Prometheus integration
   - Grafana dashboards

---

**Last Updated:** 2024-01-30  
**Version:** 0.1.0
