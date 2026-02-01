# Continuous Block Pipeline (/docs/rise-evm/cbp)

# Continuous Block Pipeline (CBP)

## The Problem with Traditional Block Production

In typical Layer 2 systems, block production follows a strictly sequential process, where only a small portion of the total blocktime is spent on actual transaction execution. This inefficiency stems from the traditional flow.

* **Consensus (C)**: Deriving L1 transactions and new block attributes. This phase blocks all other operations.
* **Execution (E)**: Executing derived transactions as well as L2 transactions from mempool. Execution is CPU-intensive.
* **Merkleization (M)**: Sealing the block by computing the new commitment to the state. Merkleization is IO-intensive and increases in cost as state size grows.

<div>
  <img src="/block-pipeline-op-light.png" alt="Block Pipeline OP Stack" className="block dark:hidden" />

  <img src="/block-pipeline-op-dark.png" alt="Block Pipeline OP Stack" className="hidden dark:block" />
</div>

*A typical block pipeline for an L2 with a one-second block time and large state. The block gasLimit will be limited by how much gas can be processed in the time allocated to execution; however, in this system, execution accounts for only a minority of the block-building pipeline. The first block in an epoch has longer consensus time due to L1 and deposit derivation.*

During the block-building pipeline, most of the blocktime is allocated to consensus and merkleization, leaving a tiny portion of total blocktime for execution. This insufficient use of blocktime leads to poor performance on many L2s.

## Execution-Merkleization Separation

To speed up execution time, many Ethereum clients separate their databases for execution and merkleization. During execution, block executors use a flat database (flatDB) to read states. The flatDB provides O(1) time complexity for reading and writing key-value pairs, much faster than the MPT's log-squared complexity, enabling fast block executions.

<Mermaid
  chart="sequenceDiagram
		participant Execution as Execution
		participant FlatDB as FlatDB
		participant Prefetcher as Prefetcher
		participant MPT as MPT
		participant Cache as Cache
		participant Merkle as Merkleization

		par Execution executing transactions
			activate Execution
			loop For every transaction
			 Note over Execution, FlatDB: Execution using data from FlatDB
			 Execution ->> FlatDB: Read execution data
			 activate FlatDB
			 FlatDB -->> Execution: Return requested data
			 Execution ->> Execution: Execute transaction
			 Execution ->> FlatDB: Apply updates
			 deactivate FlatDB
			 Execution ->> Prefetcher: Broadcast changeset
			 activate Prefetcher
			end
			Note over Execution, Merkle: When all transactions are executed
			Execution ->> Merkle: Send ExecutedResult
			deactivate Execution
			activate Merkle
		and Prefetcher fetching trie nodes
			activate MPT
			Prefetcher ->> MPT: Load trie nodes
			MPT -->> Prefetcher: Return requested nodes
			activate Cache
			Prefetcher ->> Cache: Cache trie nodes
		end

		Note over Prefetcher, Merkle: Stop the Prefetcher

		Merkle ->> Prefetcher: Signal stop
		Prefetcher ->> Prefetcher: Stop
		deactivate Prefetcher

		Note over Merkle, MPT: Load missing trie nodes
		Merkle ->> Cache: Read loaded trie nodes
		Cache -->> Merkle: Return cached nodes
		deactivate Cache

		Merkle ->> MPT: Fetch missing nodes from ExecutedResult
		MPT -->> Merkle: Return requested nodes
		deactivate MPT
		Note over Merkle: Calculating root
		Merkle ->> Merkle: Calculate root
		deactivate Merkle"
/>

All changes to keys are recorded in a **ChangeSet**. After block execution completes, the merkleization process updates the MPT with all changes from the ChangeSet to generate the full state commitment.

## RISE's Continuous Block Pipeline

Since execution and merkleization can operate on different databases, the next block's execution can start as soon as the current block's execution finishes without waiting for merkleization. CBP restructures the block building pipeline to reduce execution idle time.

<div>
  <img src="/cbp-light.png" alt="Continuous Block Pipeline" className="block dark:hidden" />

  <img src="/cbp-dark.png" alt="Continuous Block Pipeline" className="hidden dark:block" />
</div>

The idea is simple: we perform execution if there are transactions residing in the mempool. We consider two cases:

### First L2 block in an epoch

For this block, consensus must derive new L1 block information and include L1→L2 deposited transactions. Therefore, execution of this block must be performed after consensus, since deposited transactions are prioritized and might invalidate L2 transactions.

### Other blocks in an epoch

L1 information within an epoch is the same for all blocks, therefore consensus for these blocks mostly depends on the previous block. Since there are no L1-derived transactions in these blocks, execution can safely start as soon as the execution of the previous block finishes.

## Benefits

This approach offers several key benefits:

* **Continuous Execution of Transactions**. The execution thread monitors the mempool for transactions and executes them in multiple block segments, no longer waiting for consensus to request a new block
* **Higher Execution Throughput**. Execution of the next block happens simultaneously with merkleization of the current block, allocating more time for execution
* **Optimized Mempool Processing**. A new mempool structure balances latency and throughput by pre-ordering transactions to minimize shared states and maximize parallel execution

CBP enables RISE to achieve exceptional throughput while maintaining the safety and consistency guarantees expected from modern rollups.
