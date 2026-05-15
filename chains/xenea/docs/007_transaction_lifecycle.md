# Data Availability (/docs/rise-evm/data-availability)

# Data Availability

## Motivation

Data availability (DA) guarantees that all the necessary information to reconstruct the state of a rollup is available to anyone who needs it. This is crucial for the security of rollups, as it allows anyone to independently verify as well as challenge the validity of transactions and the state of the rollup. Furthermore, DA ensures that users can still access their funds and withdraw from the rollup even if the rollup itself (i.e, the sequencer) goes offline.

Ethereum introduced a new DA layer (blobs) to complement to the old `calldata` DA in its [Dencun hardfork](https://ethereum.org/en/roadmap/dencun/) with EIP-4844. Blobs enable cheaper DA costs (compared to `calldata`) as blobs’ data is transient and will be expired in around 18 days. This in turn helps reduce the overall cost for a rollup. However, the current blob throughput is limited. At the current state, Ethereum targets 6 blobs per block, with the maximum of 9 blobs. This translates to the target throughput of 64KB/s. The blob throughput is expected to (theoretically) be 8x after the Fukasa upgrade (expected late 2025), this will scale the target throughput to 512KB/s through PeerDAS v1.x. However, even with the upgrade, this throughput is insufficient for a high-load rollup like RISE.

***

## EigenDA

We use [EigenDA](https://www.eigenda.xyz/) as the main DA layer for our rollup in the normal case. EigenDA’s Mainnet currently supports a throughput of [100 MB/s](https://x.com/0xkydo/status/1950571973790363737), with the average confirmation of 5s. With this impressive throughput, EigenDA is able to provide sufficient throughput for RISE to operate upon.

<Mermaid
  chart="sequenceDiagram
	autonumber

	participant Rollup
	participant Blobstream as EigenDA

	box L1
		participant Blob as Ethereum's Blob
		participant Bridge as Bridge Contract
	end

	alt
		Note over Rollup, Bridge: Using EigenDA as DA
		Rollup ->> Blobstream: Post data
		Blobstream -->> Rollup: Return DA commitment
		Rollup ->> Bridge: Propose new state + DA commitment
	else
		Note over Rollup, Bridge: Ethereum's Blob Fallback
		Rollup ->> Blob: Post data
		Rollup ->> Bridge: Propose new state
	end"
/>

EigenDA also offers good Ethereum alignment as it leverages restaking through EigenLayer to achieve native Ethereum integration through EigenLayer, directly leveraging Ethereum's validator set via restaking. EigenDA uses Ethereum for operator registration, dispute resolution, and settlement, with no separate consensus layer.

***

## Ethereum Blob Fallback

In the event of EigenDA unavailability, the rollup can fall back to posting transactions to Ethereum’s blobs. This helps maintain the rollup’s liveness and ensure users’ funds not getting stuck in the rollup’s bridge.

Ethereum fallback is triggered whenever the `op-batcher` receives an error from EigenDA or fails to receive any acknowledgement from EigenDA, or in the case where the batcher does not have enough funds to pay EigenDA transaction fees.

After issues with EigenDA have been addressed, the `op-batcher` will switch back to EigenDA as the main DA layer.
