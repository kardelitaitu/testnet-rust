# Network Participants (/docs/rise-evm/network-participants)

# Network Participants

Traditional blockchains requiring every node to re-execute all transactions face significant drawbacks that hinder scalability, decentralization, and efficiency. This creates a problem when a blockchain wants to scale up its performance: **it needs to scale up the specs for all nodes in the network**. This is not wanted as fewer participants are able to join the network.

RISE's architecture features specialized network participants with distinct roles and hardware requirements, designed to enable scalability without requiring all nodes to re-execute transactions. Together with different optimization strategies, nodes are aided by the sequencer when performing state updates, further lowering hardware requirements to minimums.

<div>
  <img src="/network-participants-light.png" alt="Network Architecture" className="block dark:hidden" />

  <img src="/network-participants-dark.png" alt="Network Architecture" className="hidden dark:block" />
</div>

In this doc, we use Capitalized words to denote distinct types of network participants. This convention helps clearly differentiate key roles such as **Sequencers**, **Replicas**, **Fullnodes**, **Challengers**, and **Provers** emphasizing their specific responsibilities within the network.

***

## Participants

### Sequencers

Sequencers serve as the network's core, ordering and processing transactions, then batching them for L1 submission. They leverage optimized execution engines (including [pevm](/docs/rise-evm/pevm), [CBP](/docs/rise-evm/cbp) and [shreds](/docs/rise-evm/shreds)) to achieve high performance. In RISE's based sequencing model, sequencers are [gateways](/docs/rise-evm/based-sequencing) operated by Ethereum validators.

### Replicas

Replicas synchronize with the chain by applying state-diffs from the Sequencer rather than re-executing transactions. This approach allows indexing services, block explorers, and archival nodes to operate on commodity hardware while relying on fraud proofs for security verification. Replicas maintain the full chain state without the computational overhead of re-execution.

### Fullnodes

Fullnodes re-execute transactions to independently verify state updates, requiring higher hardware specs than Replicas but lower than Sequencers. They benefit from metadata (e.g, state-diff, dependency DAG) provided during synchronization, allowing them to verify state changes more efficiently than re-execution. Fullnodes provide a security checkpoint for the network because they can detect and challenge invalid state transitions.

### Challengers

As an L2, we also need a special party to submit challenges to the L1 when it detects a misbehavior of the Sequencer. This party is called **Challenger**. A Challenger must maintain a Fullnode to be able to re-execute transactions provided by the Sequencer. It only requires a single honest Challenger to maintain the security of the L2 chain. This economic incentive model ensures that even if most participants are dishonest, one honest Challenger can protect all users.

### Provers

Provers generate validity proofs using specialized hardware accelerators (FPGA/GPU) when fraud challenges occur. They operate only when needed, activated only when the Sequencer misbehaves, making them cost-effective participants. Provers don't need to run continuously; they can be brought online on-demand as disputes arise.

***

## Hardware Specs

Sequencers consume the most resources because they must execute all transactions with high performance to maintain network throughput. Replicas are optimized for accessibility, requiring minimal hardware since they avoid transaction re-execution. Fullnodes and Challengers sit in the middle, requiring enough resources for independent verification. Provers operate on-demand with specialized accelerators, making them cost-effective despite their hardware needs.

| Node Type       | Sync Method            | Security                       | Hardware Requirements                      |
| --------------- | ---------------------- | ------------------------------ | ------------------------------------------ |
| **Sequencers**  | Self-execution         | High                           | 32GB RAM                                   |
| **Replica**     | State-diff appliance   | Low, depending on fraud proofs | 8-16GB RAM                                 |
| **Fullnodes**   | Re-execution with aids | High, same as the Sequencer    | 16-32GB RAM                                |
| **Challengers** | Same as Fullnodes      | High, same as Fullnodes        | 16-32GB RAM                                |
| **Provers**     | Trusting the Sequencer | N/A                            | Depending on proving services, rarely used |

The diversity in hardware requirements ensures RISE can accommodate participants ranging from individual operators running lightweight Replicas to infrastructure providers operating high-performance Sequencers, all contributing to the security and resilience of the network.
