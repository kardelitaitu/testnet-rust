# 📝 Patch Notes: Testnet Framework

## [2026-02-24] - Active Resource Management Update

### 🚀 Performance & Memory Optimization
*   **Synchronous Wallet Release**: Refactored `ClientLease` to release wallets synchronously using `parking_lot` primitives. This eliminates the overhead of thousands of background `tokio::spawn` tasks previously used for cooldown management.
*   **HTTP Connection Pool Consolidation**: Implemented an LRU-style eviction policy for proxy-specific HTTP clients. Idle connection pools are now automatically closed after 10 minutes, preventing resource exhaustion in long-running sessions.
*   **Active Memory Cleanup Hooks**: Integrated `MemoryOptimizer` with a new hook system. Components can now register custom cleanup logic (e.g., clearing client caches, evicting idle proxies) triggered by memory pressure or periodic intervals.
*   **O(1) Concurrency Control**: Replaced standard `tokio::sync` locks with `parking_lot` in high-frequency paths within `ClientPool`, significantly reducing lock contention on high-core systems like the Ryzen 9 7950x.
*   **Idle Wallet Eviction**: `RobustNonceManager` now tracks the last access time for each wallet and automatically evicts idle state after 1 hour, preventing memory accumulation for thousands of wallets.
*   **Decrypted Wallet Cache Clearing**: Added `clear_cache` to `WalletManager` to release memory held by decrypted private keys and mnemonics after tasks are completed.
*   **Async Cleanup Pipeline**: Upgraded `MemoryOptimizer` to support asynchronous cleanup hooks, allowing complex resource-clearing operations (like database flushing or remote connection closing) to be handled during RAM pressure.
*   **Proxy Health Client Eviction**: Implemented a global cleanup for the proxy health check system, ensuring that temporary HTTP clients used for scanning do not leak memory.

## [2026-02-24] - Strict RAM Management & Provider Sharing

### 🧠 Intelligent Memory Pressure Response
*   **Strict 300MB RAM Target**: Updated the default memory threshold to 300MB to fit within constrained environments.
*   **Emergency Cleanup Triggers**: `MemoryOptimizer` now monitors usage in 5-second intervals and triggers an immediate "Emergency Cleanup" if usage exceeds 85% of the target (255MB).
*   **Tiered Eviction Strategy**: Cleanup hooks now receive an `is_emergency` flag. Under pressure, the system uses aggressive 2-minute idle timeouts for proxies and providers, compared to the standard 10 minutes.

### 🔗 Deep Resource Sharing
*   **RpcClient Sharing (Memory Efficiency Fix)**: Refactored resource sharing to cache the `RpcClient` transport layer instead of the full `Provider`. This ensures that each wallet retains its unique `Signer` (fixing "unknown account" errors) while still sharing connection pools and transport buffers.
*   **Signer-Enabled Provider Wrappers**: Each wallet now creates a lightweight Alloy `Provider` stack that wraps the shared transport, allowing local transaction signing without redundant memory overhead.
*   **Adaptive Resource Eviction**: Optimized `ClientPool` to perform tiered cleanup based on memory pressure, using more aggressive timeouts during emergency periods to stay under the 300MB RAM target.

## [2026-02-24] - Polymorphic Wallet Management

### 🧬 Architecture Evolution
*   **Targeted Decryption**: Refactored `WalletManager` to support chain-specific decryption. When running the Tempo spammer, only EVM keys are extracted from the encrypted JSON and loaded into heap memory, drastically reducing the RAM footprint for multi-chain wallet files.
*   **Chain-Agnostic API**: Introduced the `ChainType` enum and polymorphic methods (`private_key()`, `address()`) to `DecryptedWallet`. This allows the framework to be reused for Solana, SUI, and Aptos without modifying the core wallet logic.
*   **Multi-Chain Cache Segmenting**: Upgraded the internal cache to be chain-aware, ensuring that a single wallet index can be loaded for different blockchain architectures simultaneously without collision.
*   **Backward Compatibility**: Maintained the legacy field structure for `evm_private_key` and others to ensure existing spammers continue to function without breaking changes.

## [2026-02-24] - CPU Efficiency & VPS Optimization

### ⚡ Hot-Path Optimization
*   **Task Sampling Buffer**: Implemented a pre-sampled task queue for workers. Instead of sampling the weighted distribution once per loop, workers now batch-sample 50 tasks at once, significantly reducing RNG overhead.
*   **Pre-Formatted Identifiers**: Worker and wallet string identifiers are now pre-formatted at startup. This eliminates redundant `format!` calls and heap allocations within the high-frequency execution loop.
*   **Shared Gas Manager**: Refactored `TaskContext` to use a pre-allocated, shared `Arc<GasManager>`. This prevents thousands of redundant heap allocations per minute.
*   **Reduced Tracing Overhead**: Removed expensive dynamic `info_span!` creation from the hot loop. Tracing context is now handled via more efficient static logging patterns.

*   **Aggressive DB Batching**: Increased database batch size to 1000 and flush interval to 5000ms. This drastically reduces disk I/O and CPU context switching by only performing SQLite commits once every 5 seconds, ideal for low-spec VPS environments.
*   **Channel Capacity Scaling**: Expanded the async logging channel capacity to 5000 to safely buffer transaction bursts during the longer 5-second flush windows.

## [2026-02-24] - Memory Stability Stabilization
*   **Rollback of mimalloc**: Reverted to the system allocator to resolve unexpected RAM reporting spikes observed during high-concurrency sessions.
*   **Revert of Arc<str> Interning**: Restored standard string handling in the database pipeline to maintain a predictable heap profile.



### 🪵 Logging & Database
*   **Memory-Optimized Logger**: Switched to `MemoryOptimizedLayer` which uses direct `BufWriter` logic for file I/O. This provides more predictable memory usage compared to the standard non-blocking channel appender.
*   **DB Backpressure Control**: Changed `DatabaseManager` fallback strategy to `Drop` during high-load periods. This prevents the internal `mpsc` channel from acting as a memory sink when disk I/O cannot keep up with transaction throughput.
*   **Enhanced Audit Trails**: Integrated `TerminalFormatter` into the memory-optimized stack to maintain high-readability "SUCCESS/FAILED" color-coded logs while reducing memory footprint.

### 🛠️ Core Infrastructure
*   **Public Memory API**: Exported `register_memory_cleanup_hook` and related utilities from `core-logic` to allow modular optimization across different chain adapters.
*   **Thread-Safe Global State**: Migrated global `MEMORY_OPTIMIZER` to `tokio::sync::Mutex` to ensure safe operation across async boundaries during cleanup tasks.
