# Compiling Contracts (/docs/builders/smart-contracts/hardhat/compiling)

import { Step, Steps } from 'fumadocs-ui/components/steps';
import { Tab, Tabs } from 'fumadocs-ui/components/tabs';

Compile your Solidity contracts to bytecode and ABI for deployment on RISE.

## Configuration

Ensure your `hardhat.config.ts` has the Solidity compiler configured:

```typescript title="hardhat.config.ts"
import "dotenv/config";
import { defineConfig } from "hardhat/config";
import hardhatToolboxMochaEthers from "@nomicfoundation/hardhat-toolbox-mocha-ethers";

export default defineConfig({
  plugins: [hardhatToolboxMochaEthers],
  solidity: {
    version: "0.8.30",
    settings: {
      optimizer: {
        enabled: true,
        runs: 200
      }
    }
  },
  networks: {
    rise: {
      type: "http",
      url: process.env.RISE_RPC_URL || "https://testnet.riselabs.xyz",
      accounts: process.env.PRIVATE_KEY ? [process.env.PRIVATE_KEY] : [],
      chainId: 11155931
    }
  }
});
```

## Compile

<Steps>
  <Step>
    ### Run Build

    Compile all contracts in the `contracts/` directory:

    <Tabs items={['npm', 'yarn', 'pnpm', 'bun']} groupId="package-manager">
      <Tab value="npm">
        ```bash
        npx hardhat build
        ```
      </Tab>

      <Tab value="yarn">
        ```bash
        yarn hardhat build
        ```
      </Tab>

      <Tab value="pnpm">
        ```bash
        pnpm exec hardhat build
        ```
      </Tab>

      <Tab value="bun">
        ```bash
        bun x hardhat build
        ```
      </Tab>
    </Tabs>
  </Step>

  <Step>
    ### Check Artifacts

    Compilation generates artifacts in the `artifacts/` directory containing:

    * Contract ABI
    * Bytecode
    * Source maps for debugging

    The compiled JSON files can be found at:

    ```
    artifacts/contracts/YourContract.sol/YourContract.json
    ```
  </Step>
</Steps>

## Multiple Compiler Versions

If you have contracts requiring different Solidity versions:

```typescript title="hardhat.config.ts"
export default defineConfig({
  solidity: {
    compilers: [
      { version: "0.8.30" },
      { version: "0.7.6" }
    ]
  }
});
```

## Clean and Recompile

To force a fresh compilation:

<Tabs items={['npm', 'yarn', 'pnpm', 'bun']} groupId="package-manager">
  <Tab value="npm">
    ```bash
    npx hardhat clean
    npx hardhat build
    ```
  </Tab>

  <Tab value="yarn">
    ```bash
    yarn hardhat clean
    yarn hardhat build
    ```
  </Tab>

  <Tab value="pnpm">
    ```bash
    pnpm exec hardhat clean
    pnpm exec hardhat build
    ```
  </Tab>

  <Tab value="bun">
    ```bash
    bun x hardhat clean
    bun x hardhat build
    ```
  </Tab>
</Tabs>

## Next Steps

<Cards>
  <Card title="Testing Contracts" href="/docs/builders/smart-contracts/hardhat/testing" description="Test your compiled contracts" />

  <Card title="Deploying Contracts" href="/docs/builders/smart-contracts/hardhat/deploying" description="Deploy to RISE Testnet" />
</Cards>
