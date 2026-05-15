# ZK Fraud Proofs (/docs/rise-evm/zk-fraud-proofs)

# ZK Fraud Proofs

## Motivation

At the beginning, we adopted an optimistic design primarily due to the simplicity of the optimistic approach and the limitations of zero-knowledge (ZK) proving technology. At that time, simulating an EVM machine with ZK was not feasible, and ZK proving was unable to meet the desired throughput demands. Optimistic rollups, on the other hand, offered a simpler and more scalable solution.

### Complicated Interactive Fraud Proofs

However, traditional optimistic rollups rely on **interactive fraud proofs** where validators engage in multiple back-and-forth interactive steps to identify the exact transaction or step where computation diverged. This process is complex to implement correctly, due to:

* **Multi-round Interactions**. Parties must engage in multiple rounds of challenges and responses.
* **State Management**. Tracking disputed ranges, bisection points, and commitments adds significant complexity.
* **Latency Overhead**. Challenge-response cycles can take days or weeks, delaying final settlement.

Although interactive fraud proofs work, the process is complex and time-consuming, and requires significant interactions and is unfriendly to challengers.

### Expensive ZK Proofs

With recent advancements in the ZK ecosystem, we are now able to prove an EVM block in an order of seconds and transitioning to a full ZK rollup is feasible. Nevertheless, we realize that generating validity proofs is not always ideal.

* Validity proofs offer fast finality but the proving performance might not keep up with our execution client on realtime proving. We aim to process 100k TPS at RISE, and ZK proving at this rate is not possible at the moment.
* Verifying a validity proof on the L1 is expensive. If we do this frequently, users have to bear this cost, and thus, increase the transaction cost to users.
* Furthermore, as long as the sequencer behaves honestly, we will never need to use validity proofs. However, generating validity proofs for every transaction incurs an additional cost for users.

### Inability of DA Commitment Challenging with AltDA

Data Availability (DA) is crucial to fraud games, and the security of a rollup. Without DA, it is not possible to ensure the challenge and sequencer are playing a game on the same data. RISE's current design is built upon the battle-tested [OP Stack](https://github.com/ethereum-optimism/optimism) and leverages [EigenDA for its Data Availability (DA) layer](/docs/rise-evm/da). However, this design has a critical security issue that makes the sequencer highly trusted, discouraging users to use RISE.

When rollups use an AltDA layer instead of posting all data to Ethereum L1, they introduce a new challenge: **verifying DA commitments on-chain becomes problematic**.

OP Stack supports two AltDA systems: **Type 0 (Keccak)** commitments, which are simple hashes and can be challenged directly on-chain, and **Type 1 (DA-Service)** commitments, which have a flexible `da_layer_byte ++ payload` structure designed to be handled by the AltDA Server.

The problem is, the existing OP Stack implementation [does not support AltDA challenge other than Type 0 (Keccak)](https://specs.optimism.io/experimental/alt-da.html?highlight=altDA#data-availability-challenge-contract). This creates a verification gap, the network must trust the AltDA Server's claim about data availability, making fraud proofs not possible, which in turn violates the security of the rollup.

***

Therefore, we consider a hybrid approach in which we still adopt the optimistic design but utilizing ZK to generate validity proofs for a state commitment if challenged.

***

## ZK Fraud Proofs with OP Succinct Lite

RISE's ZK Fraud Proofs are made possible by [OP Succinct Lite](https://github.com/succinctlabs/op-succinct). OP Succinct Lite allow us to resolve any dispute in a single round, without the interaction requirement between the challengers and the proposer. This is more efficient than the interactive bisecting game mentioned above. Moreover, OP Succinct Lite supports AltDA like EigenDA or Celestia, which is the perfect match for RISE.

### The Workflow

The following diagram depicts the simplified flow of the hybrid approach.

<Mermaid
  chart="sequenceDiagram
	autonumber
	##participant User


	box Sequencer
		participant Sequencer
		participant Prover as Prover Network
	end

	box P2P Network
		participant Challenger
	end


	box L1 & DA
		participant DA
		participant Bridge as Rollup Bridge
		participant Verifier as Verifier Contract
	end

	##User ->> Sequencer: Submit transactions
	Sequencer ->> Sequencer: Build blocks
	Note over Sequencer, Bridge: Proposing new state
	par Proposer proposing new state
		Sequencer ->> Bridge: Proposer new statRoot
		Sequencer ->> DA: Publish transaction batches
	and Sequencer broacasting new blocks
		Sequencer ->> Challenger: Broadcast block
		Challenger -->> Bridge: Sync new state
		Challenger ->> Challenger: Verify block (by re-executing)
	and Sequencer broadcasting witness to Prover
		Sequencer ->> Prover: Publish witness
	end

	alt New state is valid
		Bridge ->> Bridge: Wait for challenge period to end
	else New state is invalid
		Note over Prover, Verifier: Fraud challenge
		Challenger ->> Bridge: Submit fraud challenge
		Prover -->> Bridge: Listen for challenge
		Prover ->> Prover: Generate validity proofs
		Prover ->> Bridge: Submit proofs
		Bridge ->> Verifier: Verify proofs
	end"
/>

The sequencer publishes state commitments without proof similar to traditional optimistic rollups. If anyone detects an invalid state commitment, they can initiate a fraud challenge. Once challenged, the sequencer is responsible for generating a ZK validity proof demonstrating that the state transition was correct. Failures in providing a valid ZK proof in time will lead the sequencer to be slashed, and the corresponding commitment is considered invalid.

The system's elegance lies in its economics: **most of the time (99.9999%), validity proofs are never needed**. This means users avoid bearing the costs associated with proof generation and verification.

### Implications

The ZK fraud proof approach offers a more efficient, secure, and user-friendly experience.

* **Shorter Challenge Period**. Validity proofs are only required once we have a challenge. If a challenge is invoked, the sequencer then has an additional window to submit the required validity proof. The additional window time should be on an order of the maximum proving time for the sake of security.
* **Simpler and Robust Fraud Mechanism**. ZK proofs appear to be more robust than interactive fraud proofs and there are several ZK rollups that have been running on the mainnet. With this approach, a challenger can just focus on keeping up with the chain progress and identifying the incorrect state transition (same as the re-executing fraud proofs), no other interaction is required.
* **Cost Saving**. The cost for users is the same as in an optimistic rollup and operational costs are lower than a ZK rollup. While ZK rollups have to bear the cost of generating validity proofs for every state transition, even if there is no transaction; this is not required in a hybrid mode. As a result, users do not have to pay extra costs of validity proof generation and verification.
* **AltDA**. OP Succinct Lite's support for EigenDA means RISE can achieve the cost and scalability benefits of off-chain DA while maintaining on-chain verifiability through ZK proofs. No trust on the sequencer is required for security.

***

## The Path Forward: ZK Rollup

RISE is designed to evolve toward a full ZK rollup as ZK proving technology matures and becomes more cost-effective. Rather than attempting to jump directly to pure ZK proving, we follow a phased approach that allows us to validate performance, optimize systems, and maintain user experience at each stage.

### Phase 1: ZK Fraud Proofs (Current)

RISE currently operates as a hybrid rollup using ZK fraud proofs for security. The sequencer publishes state commitments optimistically, and ZK proofs are generated only when disputes arise. This phase delivers fast, low-cost transactions in the honest case, cryptographic security guarantees through on-demand ZK proving, and economic efficiency where users do not bear proving costs for normal operation.

### Phase 2: Proactive Proving

<Mermaid
  chart="---
config:
  layout: dagre
---
stateDiagram-v2
    [*] --> Unchallenged: Game Created with Output Root
    Unchallenged --> Challenged: challenge() with bond
    Unchallenged --> UnchallengedProven: prove() with ZK proof
    Challenged --> ChallengedProven: prove() with ZK proof
    UnchallengedProven --> DefenderWins: resolve() after timeout*
    ChallengedProven --> DefenderWins: resolve() after timeout*
    Challenged --> ChallengerWins: resolve() after prove timeout
    DefenderWins --> [*]: Bond returned to proposer
    ChallengerWins --> [*]: Bond transferred to challenger"
/>

***Figure**. Lifecycle of a fraud game.* (\*: *resolve can only be processed if the parent game is already resolved*).

As ZK proving technology continues to advance and costs decrease, RISE will transition to **proactive proving**, where the sequencer voluntarily submits ZK proofs for state commitments even without fraud challenges. This is a critical transitional phase for several reasons.

First, transactions achieve cryptographic finality as soon as the proof is verified on L1, rather than waiting for a challenge window to pass, delivering faster finality to users.

Second, this phase allows RISE to operationally validate ZK proving performance at scale without being dependent on it for security: running proofs continuously reveals performance bottlenecks and allows optimization before full ZK commitments.

Third, users experience faster finality incrementally without a sudden transition. In this phase, fraud proof challenges still serve as a security backup: if a sequencer fails to submit a proof, the fraud proof mechanism activates. This provides a graceful fallback while allowing real-world testing of proving infrastructure at scale.

### Phase 3: Full ZK Rollup

Once proving costs are sufficiently reduced and performance meets RISE's throughput demands, the network will transition to a **full ZK rollup**. At this point, validity proofs become mandatory for every state transition, delivering instant cryptographic finality as standard. The fraud proof mechanism is no longer needed since cryptographic correctness is always proven. Security is guaranteed cryptographically rather than through economic assumptions.

The transition between phases is not time-bound but rather tied to technological maturity and cost-effectiveness of ZK proving. RISE will remain in Phase 1 until Phase 2 becomes practical, and will remain in Phase 2 until Phase 3 becomes economically viable. This phased approach ensures RISE maintains optimal performance and user experience at every stage of evolution.
