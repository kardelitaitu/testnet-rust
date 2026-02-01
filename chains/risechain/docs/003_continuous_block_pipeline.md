# Transaction Lifecycle (/docs/rise-evm/tx-lifecycle)

# Transaction Lifecycle

RISE's transaction lifecycle facilitates a streamlined process, designed to reduce latency to as low as possible. Unlike other blockchains, RISE aims to provide near-instant responses to users' transactions.

<div>
  <img src="/tx-lifecycle-light.png" alt="Transaction Lifecycle" className="block dark:hidden" />

  <img src="/tx-lifecycle-dark.png" alt="Transaction Lifecycle" className="hidden dark:block" />
</div>

## 1. Transaction Preparation & Broadcasting

The lifecycle of a transaction starts with a user creating and signing a transaction, and submitting it to an RPC node via their frontend.

## 2. P2P Propagation

After receiving the transaction from the user, the RPC node performs sanity checks and then broadcasts this transaction to the sequencer using the P2P network.

## 3. Mempool Pre-Execution

As described in the [CBP](cbp), the transaction is pre-executed as soon as it lands in the sequencer's mempool. The CBP makes use of idle time to execute transactions while other tasks are happening. This is one of the ways we reduce end-to-end latency for transaction processing.

## 4. Shred Inclusion

Pending transactions (residing in the mempool) are included in a **Shred**. Shreds partition a canonical L2 block into multiple consecutive yet separately-verifiable segments. Shreds allow an L2 block to be incrementally constructed, with each Shred serving as a batch of preconfirmations for transactions it contains.

Importantly, each Shred does not require merkleization, allowing RISE to reduce its latency to just a few milliseconds.

Pending transactions are pre-executed while sitting in the mempool, therefore, at the time of Shred inclusion, we can reuse the pre-executed results from the previous step.

## 5. Shred Propagation & Early Updates

The sequencer broadcasts a Shred to other nodes via the P2P network after it is created. As soon as a node receives a new Shred, it immediately executes transactions within this Shred (or applies changes provided by the Shred) to get the latest state of the network.

This enables faster state synchronization across the network and a quicker response to the user's transaction. At this point, the receipt for the transaction is available and can be returned to the user.

## 6. L2 Block Inclusion

After a predefined period of time (L2 blocktime), Shreds are batched together to create a canonical L2 block. At this time, merkleization is done to seal the L2 block.

## 7. L1 Block Inclusion

Periodically, L2 blocks are batch-submitted to the DA layer and the L1 for finalizing. At this stage, transactions are considered safe (if no fraud challenge is triggered).

## Key Benefits

* **Pre-execution**: Transactions are executed before inclusion, reducing confirmation time
* **Shred-based confirmations**: Users get confirmations in milliseconds, not seconds
* **Early state updates**: Nodes update state immediately upon receiving Shreds
* **Optimized pipeline**: Each step is optimized to minimize latency while maintaining security

This lifecycle enables RISE to provide the instant responsiveness users expect while maintaining the security and decentralization properties of a proper rollup.
