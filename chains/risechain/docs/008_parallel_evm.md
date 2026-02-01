# Parallel EVM (PEVM) (/docs/rise-evm/pevm)





# Parallel EVM (PEVM)

Note the PEVM is not currently live on RISE & is a future improvement. We have already published the PEVM implementation in [GitHub](https://github.com/risechain/pevm) for reference.
With our current architecture still able to achieve 1 GGas/s with sub 3 millisecond-execution.

## Motivation

### The Sequential Execution Bottleneck

Ethereum's execution model processes transactions sequentially by design. This sequential constraint stems from a fundamental requirement for distributed consensus: **all network participants must compute identical results**. While this sequential guarantee ensures correctness, it creates a severe performance limitation. Modern CPUs contain 8 to 16 cores, yet the EVM execution path utilizes only a single core. The remaining cores are idle while waiting for the current transaction to complete.

Additionally, independent transactions that could be executed in parallel are still executed in a sequential manner, creating unnecessary latency for users and limiting the network throughput. As a result, there exists a performance gap between Ethereum (and its rollups) and competitors like Solana:

* Ethereum and its rollups combined are processing around only [200-300 TPS](https://rollup.wtf/) across 50+ rollups.
* In contrast, Solana consistently produces 1000-2000 TPS, about 10 times larger than that of all rollups combined.

### EVM Parallelization Challenges

Parallel execution for blockchains has gained prominence with Aptos, Sui, and Solana, but EVM-compatible implementations face unique challenges. Early attempts to parallelize EVM execution, notably by Polygon and Sei using [Block-STM](https://arxiv.org/abs/2203.06871), showed limited gains, mainly due to:

1. Lack of EVM-specific optimizations tailored to Ethereum's state access patterns.
2. Implementation limitations in languages like Go (with garbage collection pauses).
3. Overhead from synchronization mechanisms that negate parallelism benefits.

### Slow State Root Calculation

Beyond transaction execution, the full block processing pipeline faces a secondary bottleneck. After executing all transactions, nodes must calculate the Merkle root of the new state (the state root). This computation can be as expensive or even more expensive than transaction execution itself. If state root calculation exceeds the block time, there will be less time left for execution, causing a performance loss.

[RISE parallel EVM (pevm)](https://github.com/risechain/pevm) sets to address these limitations through a ground-up redesign focused on the EVM's unique characteristics that efficiently **executes transactions, broadcasts results, and calculates the state root in parallel**; tightly implemented in Rust for minimal runtime overheads.

***

## Technical Overview

### What is pevm?

pevm is a revolutionary execution engine that enables concurrent processing of EVM transactions while maintaining deterministic outcomes. By distributing transaction execution across multiple CPU cores, pevm dramatically increases throughput and reduces latency compared to traditional sequential execution. Key features include:

* Optimistic execution of transactions in parallel.
* Detection of transaction dependencies and conflicts to ensure deterministic outcomes.
* Compatibility with existing sequential executors for easy integration and performance boosts.

### Optimistic Concurrent Execution

pevm is built upon the foundation of [Block-STM's](https://arxiv.org/abs/2203.06871) optimistic concurrent execution: rather than predicting dependencies, pevm assumes transactions are independent, executes them in parallel, then validates the execution afterward. The engine dynamically identifies transaction parallelism without requiring developers to change anything (like explicitly declaring access states in Solana and Sui). Regardless, dApps do need to innovate new parallel designs to get more out of the parallel engine, like [Sharded AMM](https://arxiv.org/abs/2406.05568) and RISE's upcoming novel CLOB; just like how multiprocessors gave rise to the design of multithreaded programs.

Overall, this strategy trades computational work (re-executing conflicting transactions) for parallelism. On blocks with numerous independent transactions, re-execution overhead is minimal because few conflicts occur. On blocks with sequential dependencies (e.g., multiple transactions sequentially updating the same contract), re-execution occurs but the system gracefully degrades to near-sequential performance, avoiding the overhead of parallel coordination.

### EVM-Specific Innovations

pevm's contribution is not just the Block-STM itself, but rather its adaptation to EVM's specific challenges.

#### Lazy Updates

All EVM transactions in the same block read and write to the same beneficiary account for gas payments, making all transactions interdependent by default. pevm addresses this by utilizing lazy updates for this account.
We mock the balance on gas payment reads to avoid registering a state dependency and only evaluate it at the end of the block or when there is an explicit read. We apply the same technique to other common scenarios such as raw ETH or ERC20 transfers.
This enables the ability to parallelize transfers from and to the same address, with only a minor post-processing latency for lazy evaluations.

#### Mempool Preprocessing

Unlike previous rollups that ordered transactions by first-come-first-served or gas auctions, RISE innovates a new mempool structure that balances latency and throughput. The goal is to pre-order transactions to z shared states and maximise parallel execution. This has a relatively similar effect as the local fee market on Solana, where congested contracts & states are more expensive regarding gas & latency. Since the number of threads to execute transactions is much smaller than our intended TPS, we can still arrange dedicated threads to execute high-traffic contract interactions sequentially and others in parallel in other threads.

***

## Architecture Design

Blockchain execution must be deterministic so that network participants agree on blocks and state transitions. Therefore, parallel execution must arrive at the same outcome as sequential execution. Having race conditions that affect execution results would break consensus.

### Legacy Architecture

RISE pevm started out with Block-STM's optimistic execution, with a collaborative scheduler and a multi-version data structure to detect state conflicts and re-execute transactions accordingly. pevm comprises several interacting layers:

* **Scheduler** manages and distributes tasks to worker threads. It maintains atomic counters for execution and validation task indices, allowing worker threads to claim tasks without conflict. Besides, the scheduler tracks transaction status (ready for execution, currently executing, awaiting validation, validated, or aborting) and manages transaction incarnation numbers (re-execution counts).
* **Worker Threads** are executor agents. In pevm, multiple worker threads operate in parallel and independently. Each worker thread executes a sequence of tasks assigned by the scheduler: execution tasks and validation tasks. Workers do not directly synchronize with each other; all coordination occurs through the scheduler and multi-version memory structures.
* **Multi-Version Memory (MvMemory)** is the central data for conflict detection. MvMemory preserves a complete history of all writes, indexed by transaction index. For each location, MvMemory tracks which transaction wrote what value and in what order. When a transaction reads a location, it retrieves the most recent value written by any lower-indexed transaction. This versioning enables validation: after a transaction executes, validation can determine whether the specific transactions that wrote to each read location have changed.

<img alt="Legacy pevm Architecture" src={__img0} placeholder="blur" />

We made several contributions fine-tuned for EVM. For instance, all EVM transactions in the same block read and write to the beneficiary account for gas payment, making all transactions interdependent by default. RISE pevm addresses this by lazy-updating the beneficiary balance. Rather than writing actual beneficiary balance changes during execution, pevm defers this update at the end of block execution. Similarly, for raw ETH transfers to non-contract addresses, it defers both sender and recipient balance updates. These lazily-accumulated values are accumulated throughout the block and evaluated once at execution completion, or on-demand if explicitly read.

However, the legacy architecture has a limitation: it wraps the [revm](https://github.com/bluealloy/revm) EVM implementation as a black box. The custom database interface (VmDB) intercepts reads and writes but cannot optimize the internal execution flow.

### Early Performance Benchmarks

Although the legacy pevm is in pre-alpha stage, [early benchmarks](https://github.com/risechain/pevm/tree/main/crates/pevm/benches) already show promising results:

* For large blocks with few dependencies, Uniswap swaps saw a 22x improvement in execution speed.
* On average, pevm is around 2x faster than typical sequential execution for a variety of Ethereum blocks.
* The max speed-up is around 4x for a block with few dependencies.
* For L2's with large blocks, pevm is expected to consistently surpass 5x improvement in execution speed.

***

## Future Works: pevm Evolution

As we worked on our [continuous block pipeline](/docs/rise-evm/cbp), [shreds](/docs/rise-evm/shreds), and [Reth's parallel sparse trie](https://github.com/paradigmxyz/reth/tree/v1.9.3/crates/trie/sparse), we eventually found ways to innovate Parallel EVM way beyond what BlockSTM originally proposed. The ultimate goal is to achieve 10 Gigagas/s and beyond, making RISE pevm the fastest EVM execution engine available.

<img alt="New pevm Architecture" src={__img1} placeholder="blur" />

The new architecture aims to accelerate the legacy architecture through the following optimizations.

### Inline Parallel-Aware EVM

Rather than wrapping [revm](https://github.com/bluealloy/revm), the new architecture implements an EVM interpreter specifically designed for parallel execution. The inline interpreter sets to minimize VM overheads, and enable efficient sharing of read-only bytecode and state across worker threads.

### Shred Integration

As [shreds](/docs/rise-evm/shreds) are becoming more mature, we will add shreds to broadcast pending states per committed transactions in realtime, enabling fullnodes and dApps to observe state changes in realtime. Furthermore, each shred contains a state-diff from the previous state, making it possible for following nodes to build a transaction dependency graph (i.e, DAG), further accelerate re-execution performance.

### Sparse Trie for State Root Calculation

Computing the Merkle root of all state changes is computationally expensive and traditionally blocks the critical path. The new design employs a sparse trie to accelerate state root calculation. Rather than computing a state root after all transactions complete, the system progressively updates the trie as transactions validate, with background worker threads computing Merkle proofs in parallel. This reduces the overhead of state root calculation from the critical path, leaving more time for execution.

### Extended Resource-Aware Scheduler

We extend the scheduler to also schedule shred committing and multiproof tasks, with a new design that is highly resource-aware. The new scheduler evolves beyond distributing execution and validation tasks to coordinating a richer task set: **execution, validation, multiproof generation, and shred commitment**. It becomes *resource-aware* by analyzing CPU and memory usage, dynamically adjusting task priorities and worker thread assignments.
