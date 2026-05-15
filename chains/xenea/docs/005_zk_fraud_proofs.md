# Shreds (/docs/rise-evm/shreds)

# Shreds

Shreds enable a user experience that feels like the modern web, but on a blockchain. They generate transaction preconfirmations within single-digit milliseconds, reacting to demand in real time. Unlike traditional blockchains, which wait for discrete blocks to process, Shreds are continuous and interrupt-driven.

<div>
  <img src="/shred-latency-light.png" alt="Latency with and without Shreds" className="block dark:hidden" />

  <img src="/shred-latency-dark.png" alt="Latency with and without Shreds" className="hidden dark:block" />
</div>

## Breaking Down Blocks

When posting summaries to L1 and DA, a rollup typically batches multiple L2 blocks together to reduce costs. Since merkleization is not required for every L2 block, we can break L2 blocks (12-second long) into **Shreds** (sub-second long), essentially **mini-blocks without a state root**.

Constructing and validating these Shreds is much faster due to the omission of state root merkleization. Therefore, broadcasting them enables rapid transaction and state confirmations. This improved latency doesn't sacrifice security, as new L2 blocks can only provide unsafe confirmations anyway.

<Mermaid
  chart="sequenceDiagram
		participant User
		participant Mempool
		participant Sequencer
		participant P2P Network
		participant L1

    Note over User,P2P Network: Users receive preconfirmations at the Shred level
    User ->> Mempool: Submit transactions
    Loop Until Reaching Sufficient No. Shreds
        Sequencer->>Mempool: Fetch transactions
        Sequencer->>Sequencer: Build Shred
        Sequencer->>Sequencer: Sign Shred
        Sequencer-->> P2P Network: Broadcast Shred
        P2P Network->> P2P Network: Verify & process Shred
    end

    par Sequencer Proposing L2 Block
	    Sequencer->>Sequencer: Build L2 block with Merkle root
	    Sequencer->>L1: Propose L2 block
	  and P2P Network Verifying L2 Block
	    P2P Network->>P2P Network: Build L2 block with Merkle root
	    P2P Network->>L1: Verify Proposed L2 block
	  end"
/>

[//]: # "![Shred flow](/shred_flow.png)"

***

## What are Shreds?

Shreds partition an L2 block into multiple consecutive, connecting segments. Each Shred for an L2 block is identified via its sequence number. In other words, `block_num` and `seq_num` together can always identify a Shred. The sequence number increases for each Shred and resets when a new L2 block is constructed.

A Shred contains a **ChangeSetRoot** that commits to all state changes made within the Shred. During execution, the sequencer uses the flatDB to access data and records all changes to a **ChangeSet**. The number of entries in a ChangeSet is relatively small compared to the state size because it only holds the changes, therefore the data is sparse and can fit in memory. As a result, constructing the ChangeSetRoot can be efficiently done.

***

## Block Propagation

Broadcasting is done per Shred instead of waiting for the full L2 block. Shreds with invalid signatures are discarded. As peer nodes receive valid Shreds, they optimistically construct a local block and provide preconfirmations to users.

<div>
  <img src="/shred-prop-light.png" alt="Shreds Propagation" className="block dark:hidden" />

  <img src="/shred-prop-dark.png" alt="Shreds Propagation" className="hidden dark:block" />
</div>

The sequencer might also broadcast the ChangeSet within a Shred to its peers. This ChangeSet can be verified against the ChangeSetRoot attached to the Shred's header. Nodes trusting the sequencer can apply the ChangeSet immediately to their local state without re-executing transactions.

***

## Batch Preconfirmations

Preconfirmations are issued per batch via Shreds instead of per individual transaction. Users can receive preconfirmations without waiting for the entire L2 block to be completed. The Shred blocktime can be configured to balance multiple factors, including preconfirmation latency and network bandwidth.

***

## Efficient Merkleization

Merkleization's performance is influenced by both the size of state data and the number of changes (i.e., the size of the ChangeSet). Additionally, batch updates are more efficient than sequential updates.

Merkleization for an L2 block only happens after the last Shred is generated. Accumulating changes over multiple Shreds reduces the number of final keys that need updating, thanks to transaction overlap. The same data is likely to be accessed multiple times across blocks, especially with popular dApps like Uniswap.

Shreds enable RISE to provide instant transaction confirmations while maintaining the security and consistency guarantees of traditional rollups.
