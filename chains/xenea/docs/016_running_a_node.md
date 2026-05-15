# Get Started (/docs/builders/smart-contracts/hardhat/get-started)

import { Step, Steps } from 'fumadocs-ui/components/steps';
import { Tab, Tabs } from 'fumadocs-ui/components/tabs';

[Hardhat](https://hardhat.org) is a flexible and extensible development environment for Ethereum software. It helps you write, test, debug, and deploy your smart contracts with ease.

## Prerequisites

* [Node.js](https://nodejs.org/) v22 or later
* A package manager like npm, yarn, pnpm, or bun
* A wallet with testnet ETH from the [RISE Faucet](https://faucet.testnet.riselabs.xyz/)

## Create a Project

<Steps>
  <Step>
    ### Initialize Hardhat Project

    Create a new directory and initialize a Hardhat project:

    ```bash
    mkdir my-rise-project
    cd my-rise-project
    ```

    Initialize Hardhat with the setup wizard:

    <Tabs items={['npm', 'yarn', 'pnpm', 'bun']} groupId="package-manager">
      <Tab value="npm">
        ```bash
        npx hardhat --init
        ```
      </Tab>

      <Tab value="yarn">
        ```bash
        yarn dlx hardhat --init
        ```
      </Tab>

      <Tab value="pnpm">
        ```bash
        pnpm dlx hardhat --init
        ```
      </Tab>

      <Tab value="bun">
        ```bash
        bun x hardhat --init
        ```
      </Tab>
    </Tabs>

    When prompted, select:

    1. **Version**: "Hardhat 3 Beta"
    2. **Path**: current directory (`.`)
    3. **Project type**: "A minimal Hardhat project"
    4. **Install dependencies**: true

    This will create a complete project structure with all necessary dependencies.
  </Step>

  <Step>
    ### Configure for RISE

    Update your `hardhat.config.ts` to add the RISE Testnet:

    ```typescript title="hardhat.config.ts" {11-16}
    import hardhatToolboxViemPlugin from "@nomicfoundation/hardhat-toolbox-viem";
    import { defineConfig } from "hardhat/config";

    export default defineConfig({
      plugins: [hardhatToolboxViemPlugin],
      solidity: {
        version: "0.8.30",
      },
      networks: {
        riseTestnet: {
          type: "http",
          url: "https://testnet.riselabs.xyz",
          accounts: ["<YOUR_PRIVATE_KEY>"],
          chainId: 11155931
        }
      }
    });
    ```

    <Callout type="warn">
      Never commit private keys directly in config files. In the next step, you'll learn how to use Configuration Variables to keep this secure.
    </Callout>
  </Step>

  <Step>
    ### Secure Your Private Key

    Use Hardhat's Configuration Variables (keystore) to store your private key securely:

    <Tabs items={['npm', 'yarn', 'pnpm', 'bun']} groupId="package-manager">
      <Tab value="npm">
        ```bash
        npx hardhat keystore set RISE_PRIVATE_KEY
        ```
      </Tab>

      <Tab value="yarn">
        ```bash
        yarn hardhat keystore set RISE_PRIVATE_KEY
        ```
      </Tab>

      <Tab value="pnpm">
        ```bash
        pnpm exec hardhat keystore set RISE_PRIVATE_KEY
        ```
      </Tab>

      <Tab value="bun">
        ```bash
        bun x hardhat keystore set RISE_PRIVATE_KEY
        ```
      </Tab>
    </Tabs>

    Then update your config to use the variable:

    ```typescript title="hardhat.config.ts" {2,13}
    import hardhatToolboxViemPlugin from "@nomicfoundation/hardhat-toolbox-viem";
    import { configVariable, defineConfig } from "hardhat/config";

    export default defineConfig({
      plugins: [hardhatToolboxViemPlugin],
      solidity: {
        version: "0.8.30",
      },
      networks: {
        riseTestnet: {
          type: "http",
          url: "https://testnet.riselabs.xyz",
          accounts: [configVariable("RISE_PRIVATE_KEY")],
          chainId: 11155931
        }
      }
    });
    ```

    <Callout title="Alternative: Using .env files">
      If you prefer to use a `.env` file with the `dotenv` package instead of Hardhat's keystore, you can do so, but **this is not recommended** as it's less secure. Make sure to never commit your `.env` file to version control by adding it to `.gitignore`.

      1. Create a `.env` file: `PRIVATE_KEY=your_private_key_here`
      2. Install dotenv: `npm install dotenv`
      3. Add to the top of your config: `require("dotenv").config();`
      4. Update accounts: `accounts: process.env.PRIVATE_KEY ? [process.env.PRIVATE_KEY] : []`
    </Callout>
  </Step>

  <Step>
    ### Create a Smart Contract

    Make sure you're in your Hardhat project's root directory. Then create a `contracts` directory and add a simple Counter contract:

    ```bash
    mkdir contracts
    ```

    Create `contracts/Counter.sol`:

    ```solidity title="contracts/Counter.sol"
    // SPDX-License-Identifier: UNLICENSED
    pragma solidity ^0.8.30;

    contract Counter {
        uint256 public number;

        function setNumber(uint256 newNumber) public {
            number = newNumber;
        }

        function increment() public {
            number++;
        }
    }
    ```
  </Step>

  <Step>
    ### Compile the Contract

    Compile your contract to verify everything is working:

    <Tabs items={['npm', 'yarn', 'pnpm', 'bun']} groupId="package-manager">
      <Tab value="npm">
        ```bash
        npx hardhat compile
        ```
      </Tab>

      <Tab value="yarn">
        ```bash
        yarn hardhat compile
        ```
      </Tab>

      <Tab value="pnpm">
        ```bash
        pnpm exec hardhat compile
        ```
      </Tab>

      <Tab value="bun">
        ```bash
        bun x hardhat compile
        ```
      </Tab>
    </Tabs>

    This generates artifacts in the `artifacts/` directory.
  </Step>
</Steps>

## Next Steps

<Cards>
  <Card title="Compiling Contracts" href="/docs/builders/smart-contracts/hardhat/compiling" description="Learn about compilation options" />

  <Card title="Testing Contracts" href="/docs/builders/smart-contracts/hardhat/testing" description="Write and run tests" />

  <Card title="Deploying Contracts" href="/docs/builders/smart-contracts/hardhat/deploying" description="Deploy to RISE Testnet" />

  <Card title="Verifying Contracts" href="/docs/builders/smart-contracts/hardhat/verifying" description="Verify on the explorer" />
</Cards>
