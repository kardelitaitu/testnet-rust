# Verifying Contracts (/docs/builders/smart-contracts/foundry/verifying)

import { Step, Steps } from 'fumadocs-ui/components/steps';

Verify your deployed contracts on the RISE Testnet Explorer to make the source code publicly viewable.

## Configuration

Configure verification in your `foundry.toml`:

```toml title="foundry.toml"
[profile.default]
src = "src"
out = "out"
libs = ["lib"]
solc = "0.8.30"

[rpc_endpoints]
rise = "https://testnet.riselabs.xyz"
```

<Callout type="info">
  RISE uses Blockscout for contract verification. You don't need an API key - just specify `--verifier blockscout` when verifying.
</Callout>

## Deploy and Verify

The easiest way is to deploy and verify in a single command:

```bash
forge create \
  --rpc-url https://testnet.riselabs.xyz \
  --private-key $PRIVATE_KEY \
  src/Counter.sol:Counter \
  --verify \
  --verifier blockscout \
  --verifier-url https://explorer.testnet.riselabs.xyz/api/
```

## Verify Existing Contract

To verify an already deployed contract:

```bash
forge verify-contract \
  --rpc-url https://testnet.riselabs.xyz \
  <DEPLOYED_CONTRACT_ADDRESS> \
  src/Counter.sol:Counter \
  --verifier blockscout \
  --verifier-url https://explorer.testnet.riselabs.xyz/api/
```

### With Constructor Arguments

If your contract has constructor arguments:

```bash
forge verify-contract \
  --rpc-url https://testnet.riselabs.xyz \
  <DEPLOYED_CONTRACT_ADDRESS> \
  src/Lock.sol:Lock \
  --verifier blockscout \
  --verifier-url https://explorer.testnet.riselabs.xyz/api/ \
  --constructor-args $(cast abi-encode "constructor(uint256)" 1706745600)
```

## Using Config Aliases

If you configured the RPC endpoint in `foundry.toml`, you can use the alias:

```bash
forge verify-contract \
  --rpc-url rise \
  <DEPLOYED_CONTRACT_ADDRESS> \
  src/Counter.sol:Counter \
  --verifier blockscout \
  --verifier-url https://explorer.testnet.riselabs.xyz/api/
```

## Verify via Script

Verify contracts deployed via scripts by adding the `--verify` flag:

```bash
forge script script/Counter.s.sol:CounterScript \
  --rpc-url https://testnet.riselabs.xyz \
  --broadcast \
  --verify \
  --verifier blockscout \
  --verifier-url https://explorer.testnet.riselabs.xyz/api/
```

## Important Notes

* The RISE explorer uses Blockscout, so always specify `--verifier blockscout`
* Constructor arguments are ABI-encoded - use `cast abi-encode` to encode them
* Never store private keys in source code

## View Verified Contract

After verification, view your contract on the [RISE Testnet Explorer](https://explorer.testnet.riselabs.xyz). The "Contract" tab will show your source code and allow direct interaction with contract functions.
