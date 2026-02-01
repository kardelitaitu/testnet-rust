# Network Details (/docs/builders/network-details)

Use the information below to connect and submit transactions to RISE.

| Property        | Testnet                                 |
| --------------- | --------------------------------------- |
| Network Name    | RISE Testnet                            |
| Chain ID        | `11155931`                              |
| RPC URL         | `https://testnet.riselabs.xyz`          |
| Explorer        | `https://explorer.testnet.riselabs.xyz` |
| Currency Symbol | ETH                                     |

<AddToWallet />

## Using with Development Tools

### Hardhat Configuration

```javascript
// hardhat.config.js
module.exports = {
  networks: {
    riseTestnet: {
      url: "https://testnet.riselabs.xyz",
      chainId: 11155931,
      accounts: process.env.PRIVATE_KEY !== undefined ? [process.env.PRIVATE_KEY] : []
    }
  }
};
```

### Foundry Configuration

```toml
# foundry.toml
[profile.default]
src = "src"
out = "out"
libs = ["lib"]

[rpc_endpoints]
rise_testnet = "https://testnet.riselabs.xyz"

[blockscout]
rise_testnet = { key = "", url = "https://explorer.testnet.riselabs.xyz/api" }
```

## Gas & Transaction Details

| Property                      | Value              |
| ----------------------------- | ------------------ |
| Max Gas Limit per Transaction | 16M                |
| Nonce Order                   | Enforced on-chain  |
| Data Storage Fees             | Same as L1         |
| Blocks to Finality            | 259,200 (\~3 days) |

## Getting Testnet ETH

<Card icon={<Droplets className="text-(--rise-purple)" />} title="RISE Faucet" href="https://faucet.testnet.riselabs.xyz" description="Get free testnet ETH and other tokens for testing" />
