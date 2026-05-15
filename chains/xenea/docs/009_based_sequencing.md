# Based Sequencing (/docs/rise-evm/based-sequencing)



# Based Sequencing

## The Sequencer Problem

At the core of every rollup is the sequencer, a critical entity that manages transaction flow on the rollup, ensuring efficient and secure interaction with L1.

<div>
  <img src="/centralized-sequencer-light.png" alt="Centralized Sequencers" className="block dark:hidden" />

  <img src="/centralized-sequencer-dark.png" alt="Centralized Sequencers" className="hidden dark:block" />
</div>

The sequencer fetches and orders incoming transactions from the mempool into blocks (similar to block proposers in regular blockchains). It also creates L2 checkpoints by posting the latest state commitment to L1. It acts as an intermediary between the L2 and L1, performing the following functions.

* **Ordering.** The sequencer receives transactions from users on the L2, orders them, and includes them in blocks.
* **Execution.** The sequencer executes the transactions and updates the state of the rollup.
* **Batching.** The sequencer groups transactions into batches, compresses the data, and submits these batches to Ethereum.
* **Preconfirmation**. The sequencer advertises an RPC endpoint for users to submit transactions to. As a response, the sequencer issues soft confirmations on user transactions.

### Centralization Risks

Traditional rollups favor centralized sequencers because they offer high performance on dedicated hardware, low latency for better user experience. Low latency is crucial for applications that require quick transaction confirmations, such as perpetual exchanges or lending platforms. Last but not least, a rollup with centralized sequencers is simple to implement, mainly because no consensus is required.

However, a centralized sequencer becomes a **single point of failure**, making the entire rollup vulnerable to technical failures, attacks, or manipulations. A centralized sequencer has the absolute power to censor transactions, reorder transactions to favor their own interests, go offline and halt the rollup entirely, or extract bad MEV from users. This raises concerns about censorship resistance. If they refuse to process certain types of transactions, perhaps due to regulatory pressure or self-interest, users may find themselves unable to execute essential operations within the ecosystem. The impact of censorship undermines the fundamental principles of a permissionless network.

***

## Based Sequencing: Leveraging Ethereum's Security

[Based sequencing](https://ethresear.ch/t/based-rollups-superpowers-from-l1-sequencing/15016) removes the centralized sequencer entirely. Instead, L1 block proposers, the same entities responsible for building Ethereum blocks, serve as sequencers for the rollup.

### The Philosophy

Rather than building a separate L2 consensus mechanism, RISE leverages Ethereum's existing, battle-tested block building pipeline. RISE inherits Ethereum proven liveness and censorship resistance properties backed by around 1M+ validators with no additional consensus layer.

### How It Works

<div>
  <img src="/based-sequencing-light.png" alt="General paradigm of based sequencing" className="block dark:hidden" />

  <img src="/based-sequencing-dark.png" alt="General paradigm of based sequencing" className="hidden dark:block" />
</div>

Users submit transactions to the current L1 proposer. The L1 proposer orders these transactions alongside regular L1 transactions. The rollup's execution node processes these transactions off-chain and generates a new state commitment.

## Based Preconfirmations

While based sequencing solves centralization, it introduces a new problem: **latency**. Users must wait for L1 blocks to be included (typically 12 seconds) before seeing transaction confirmation, even though the sequencer is already part of L1. This is not wanted in a rollup as users are familiar with fast confirmations given by centralized sequencers.

RISE solves this with **based preconfirmations**: cryptographically enforced promises from Ethereum proposers that certain events will happen in upcoming blocks.

### What Are Preconfirmations?

A **preconfirmation** (or **preconf**) is a collateral-backed promise by the current proposer (or a **gateway**, whomever the proposer delegates to) that certain events will happen at a specific future timestamp.

The fundamental shift is moving from **trust-based systems** to **a more trustless and cryptographically enforced system**. Instead of trusting a centralized sequencer, RISE uses Ethereum-enforced slashing mechanisms, and collateral requirements to reduce transaction latency from 12 seconds to milliseconds while maintaining security guarantees.

### Gateway

Issuing preconfs adds many requirements to L1 proposers. To mitigate this sophistication, in our design, we assume the proposer delegates sequencing right to a sophisticated entity called a gateway (*a.k.a* a **preconfer**). Gateways function as intermediaries connecting Ethereum's block-building market to the L2. They serve as sequencers while providing L1-secured preconfs to users. Unlike traditional sequencers, gateways face slashing penalties for not honoring preconfs.

<img alt="Gateway" src={__img0} placeholder="blur" />

The delegation is somewhat similar to how most proposers today delegate their block building tasks to builders. The delegation of preconf rights can occur through on-chain or off-chain mechanisms. We consider the off-chain approach (i.e, via the existing PBS pipeline) in this design.

#### Registration and Collateral

A registry contract is deployed for any L1 validators to sign up to become a sequencer for the RISE rollup. Upon registration, L1 validators also need to stake some collateral. This collateral requirement ensures that L1 validators are economically discouraged to misbehave as this will result in their collateral being slashed.

#### Slashing

The system penalizes two categories of violations: **liveness faults** (when preconfs were not included and the proposer's slot was missed) and **safety faults** (when preconfs were not included despite the proposer's slot being available). Both trigger slashing events that penalize the misbehaving validator's collateral.

#### Gateway Delegation and PBS Integration

Proposers delegate sequencing rights to gateways through existing PBS (Proposer-Builder Separation) pipelines. This architecture mirrors builder relationships in current Ethereum MEV workflows, ensuring compatibility with the existing block-building ecosystem.

#### Batch Processing with Shreds

Preconfs are issued in batches using [shreds](/docs/rise-evm/shreds). Batch preconfs inherently align with the operational efficiency of rollups, which are designed to process transactions in bulk. By issuing a single preconf for multiple transactions, rollups can potentially achieve lower overhead per transaction compared to providing individual preconf for each user. Shreds are currently variable in time to balance between throughput and latency.

### Preconf Propagation

When users submit transactions, they send them to any node, which forwards them to the current gateway. The gateway orders these transactions into a shred, executes the shred, and broadcasts the resulting shred through the P2P network. This design ensures that preconfs are visible across the network in near realtime, giving users and applications strong assurance that their transactions will be included/executed. More details about shred propagation can be found [here](/docs/rise-evm/shreds#block-propagation).

### Fallback Gateway Mechanism

To maintain liveness and availability, RISE employs a fallback gateway mechanism. This ensures the network continues functioning during gateway unavailability and addresses two critical scenarios:

* **Cold-start problem**. In the early state of Phase 3 (see below), there might be only a few L1 validators opted into sequencing, creating gaps between preconf slots. A fallback gateway ensures continuous sequencing availability during this transition period.
* **Liveness failures**. When an active gateway experiences operational issues that prevent rollup progression, a fallback sequencer steps in immediately rather than waiting for the full sequencing window to expire. The system monitors the maximum duration between consecutive state commitments. If no L2 state commitment is published within this period, the next gateway in rotation can take over immediately.

This approach balances safety during early phases with the decentralization goals of the long-term vision, ensuring users always have confirmed transactions while progressively removing central dependencies.

***

## Implementation Roadmap

RISE implements based sequencing in three phases, progressively decentralizing the network while maintaining operational maturity throughout the transition.

### Phase 1: The Taste

The initial phase extends the current RISE sequencer to incorporate gateway functionalities. This phase proves that the gateway architecture works in practice and validates all necessary components before broader adoption.

Key activities include implementing L1 interactions (slashing and delegation mechanisms), issuing L1-secured execution preconfs as shreds, testing gateway component maturity and reliability, and validating software stability under production conditions.

By the end of this phase, a single gateway provides fast, L1-secured preconfs to users, slashing and delegation mechanisms are battle-tested, the system proves capable of handling transaction volume and latency requirements, and infrastructure is ready for decentralized participation.

### Phase 2: The Aligning

This phase serves as a transitional phase before full permissionlessness. Multiple whitelisted gateways operate in a **round-robin rotation system**. RISE will remain in this phase long enough to ensure everything works correctly, the system is robust, and operational processes are mature before moving to full permissionlessness. During this phase, a default gateway (operated by RISE) serves as a fallback gateway to ensure the rollup's liveness.

#### How Rotating Works

Multiple gateways take turns to sequence the rollup. Each gateway operates during a fixed sequencing window (measured in L1 blocks) before handing off to the next gateway. Gateways rotate sequencing duties with each receiving equal opportunity to sequence during its window. The selection mechanism is fully deterministic and transparent on-chain: anyone can identify the current and next gateway using a deterministic formula derived from the L1 Registry contract.

#### Efficient Handover

The system ensures smooth handover through two mechanisms.

* **Shred streaming** provides realtime block segments that deliver near-instantaneous state updates between gateways.
* **Mempool synchronization** maintains a direct communication channel between current and the next gateways so they have consistent transaction pool views, allowing the next gateway to pre-process transactions not yet processed by the current gateway for instantaneous handover.

#### Enhanced Liveness and Censorship Resistance

This phase enhances the rollup's robustness in several ways. Multiple whitelisted but independent gateways share sequencing duties for decentralization, backup gateways prevent downtime if the current gateway goes offline, no single entity controls transaction ordering for censorship resistance, and the system survives individual gateway failures through redundancy. Rotation prevents any single entity from dominating block building over time, building long-term censorship capabilities.

### Phase 3: The Basedening

The final phase removes gateway whitelisting entirely for full permissionless participation. Any Ethereum validator can become a gateway by delegating block-building rights through collateral staking. Gateways compete freely for users and transaction ordering, economic incentives align as validators earn fee portions for including rollup transactions. The economic incentives are straightforward: when proposers propose blocks, they include rollup transactions and receive fee revenue as compensation for sequencing.

By this phase, rollup decentralization grows with Ethereum's validator set, no single entity can have full control over gateway participation, users benefit from free market competition among sequencers, and full permissionless composability with Ethereum is achieved.

## The Ultimate Vision

When fully implemented, the boundary between RISE and Ethereum becomes smaller. RISE becomes not just a fast L2, but a natural extension of Ethereum itself, offering security with L1-enforced slashing, L1-secured near realtime preconf rather than waiting for L1 confirmation, full composability with Ethereum's block-building market.
