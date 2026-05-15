# RISE EVM Architecture (/docs/rise-evm)

# Architecture

Traditional web servers update state in a continuous, interrupt-driven manner. This means transactions are processed as soon as they arrive. When demand is high, servers implement queueing to manage the load and process requests as resources become available. This is the exact UX we aim to achieve with RISE.

RISE provides developers with exceptional performance and capabilities while delivering near-instant latency and seamless user experience. RISE achieves over 1 GGas/s with millisecond-latency for immediate transactions.

## Components

The figure below details the high-level architecture of the RISE stack.

<div>
  <img src="/architecture-light.png" alt="RISE Stack Architecture" className="block dark:hidden" />

  <img src="/architecture-dark.png" alt="RISE Stack Architecture" className="hidden dark:block" />
</div>

* **Execution**. A revolutionary EVM-compatible execution engine that redefines performance with infinite speed.
  * **Parallel EVM (pevm)**. The ultimate parallel EVM engine
  * **Continuous Block Pipeline (CBP)**. Executing transactions while residing in mempool.
  * **Shreds**. Efficient interrupt-driven block construction.
* **RiseDB**. A custom DB specifically designed for EVM chain states.
* **Data Availability**. A highly performant DA layer with Ethereum fallbacks.
* **ZK Fraud Proofs**. Simpler fraud proofs for better UX.
* **Based Sequencing**. The RISE's plan for decentralization.

## Core Components

The RISE stack delivers exceptional performance through these key components:

### Execution Engine

* **[Continuous Block Pipeline](/docs/rise-evm/cbp)** - Continuous block processing pipeline for optimal throughput
* **[Shreds](/docs/rise-evm/shreds)** - Fast transaction shredding for parallel execution

### Settlement & DA

* **[ZK Fraud Proofs](/docs/rise-evm/zk-fraud-proofs)** - Hybrid rollup approach combining optimistic and ZK proving for efficient fraud resolution
* **[Data Availability](/docs/rise-evm/data-availability)** - Modular DA layer leveraging Celestia and EigenDA for scalability

### Network Layer

* **[Transaction Lifecycle](/docs/rise-evm/tx-lifecycle)** - Complete transaction processing pipeline from submission to finality
* **[Network Participants](/docs/rise-evm/network-participants)** - Overview of RISE network participant types, roles, and hardware requirements

### Future Improvements

* **[Parallel EVM (PEVM)](/docs/rise-evm/pevm)** - The ultimate parallel EVM engine
* **[Based Sequencing](/docs/rise-evm/based-sequencing)** - Decentralized sequencing leveraging Ethereum L1 with cryptographically enforced preconfirmations and rotating gateways

These components work together to deliver over 100,000 TPS with sub-10ms latency while maintaining full EVM compatibility and Ethereum's security guarantees.
