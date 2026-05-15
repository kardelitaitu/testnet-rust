# Compiling Contracts (/docs/builders/smart-contracts/foundry/compiling)

import { Step, Steps } from 'fumadocs-ui/components/steps';

Compile your Solidity contracts using Foundry's `forge build` command.

## Configuration

Configure the Solidity compiler in your `foundry.toml`:

```toml title="foundry.toml"
[profile.default]
src = "src"
out = "out"
libs = ["lib"]
solc = "0.8.30"

# Optimizer settings
optimizer = true
optimizer_runs = 200

[rpc_endpoints]
rise = "https://testnet.riselabs.xyz"

[etherscan]
rise = { key = "", url = "https://explorer.testnet.riselabs.xyz/api" }
```

## Compile

<Steps>
  <Step>
    ### Run Build

    Compile all contracts in the `src/` directory:

    ```bash
    forge build
    ```
  </Step>

  <Step>
    ### Check Artifacts

    Compilation generates artifacts in the `out/` directory containing:

    * Contract ABI
    * Bytecode
    * Metadata

    The compiled JSON files can be found at:

    ```
    out/YourContract.sol/YourContract.json
    ```
  </Step>
</Steps>

## Compiler Options

### Specify Solidity Version

```bash
forge build --use 0.8.30
```

### Enable Optimizer

```bash
forge build --optimize --optimizer-runs 200
```

### Watch Mode

Automatically recompile on file changes:

```bash
forge build --watch
```

## Clean and Rebuild

To force a fresh compilation:

```bash
forge clean
forge build
```

## Check Contract Sizes

Ensure your contracts are within the 24KB size limit:

```bash
forge build --sizes
```

Output shows each contract's size.

## Next Steps

<Cards>
  <Card title="Testing Contracts" href="/docs/builders/smart-contracts/foundry/testing" description="Test your compiled contracts" />

  <Card title="Deploying Contracts" href="/docs/builders/smart-contracts/foundry/deploying" description="Deploy to RISE Testnet" />
</Cards>
