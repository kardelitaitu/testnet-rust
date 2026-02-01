# Tempo
Documentation, integration guides, and protocol specifications

Welcome to Tempo![](https://docs.tempo.xyz/#welcome-to-tempo)
-------------------------------------------------------------

Tempo is a general-purpose blockchain optimized for payments. Tempo is designed to be a low-cost, high-throughput blockchain with user and developer features that we believe should be core to a modern payment system.

Tempo was designed in close collaboration with an exceptional group of [design partners](https://tempo.xyz/ecosystem) who are helping to validate the system against real payment workloads.

Whether you're new to stablecoins, ready to start building, or looking for partners to help you integrate, these docs will help you get started with Tempo.

[

Learn About Stablecoins

Start here to understand what they are, why they matter, and the real-world payment use cases they enable.

](https://docs.tempo.xyz/learn)
[

Integrate Tempo Testnet

Connect to the testnet, get faucet funds, and start building with our SDKs and guides.

](https://docs.tempo.xyz/quickstart/integrate-tempo)
[

Build With Partners

Work with our ecosystem partners for stablecoin issuance, custody, compliance, orchestration, and infrastructure.

](https://docs.tempo.xyz/learn/partners)
[

Get In Touch

Connect with the Tempo team to discuss partnerships, integration opportunities, or opportunities to collaborate.

](mailto:partners@tempo.xyz)

Testnet Migration[](https://docs.tempo.xyz/#testnet-migration)
--------------------------------------------------------------

We've launched a new testnet to better align with our mainnet release candidate and provide faster feature release cycles. The old testnet will be deprecated on **March 8th, 2025**.

**What you need to do:**

1.  **Update your RPC URL** to `https://rpc.moderato.tempo.xyz`
2.  **Update your chain ID** to `42431`
3.  **Redeploy any contracts** to the new testnet
4.  **Reset any databases or indexers** that depend on old testnet data

See [Connection Details](https://docs.tempo.xyz/quickstart/connection-details#direct-connection-details) for the full configuration.