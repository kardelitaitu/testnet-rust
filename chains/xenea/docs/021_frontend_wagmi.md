# Get Started (/docs/builders/smart-contracts/foundry/get-started)

import { Step, Steps } from 'fumadocs-ui/components/steps';

[Foundry](https://book.getfoundry.sh/) is a blazing fast, portable, and modular toolkit for Ethereum application development written in Rust.

## Prerequisites

* A wallet with testnet ETH from the [RISE Faucet](https://faucet.testnet.riselabs.xyz/)

## Install Foundry

```bash
curl -L https://foundry.paradigm.xyz | bash
foundryup
```

This installs `forge`, `cast`, `anvil`, and `chisel`.

## Create a Project

<Steps>
  <Step>
    ### Initialize Project

    Create a new Foundry project:

    ```bash
    forge init my-rise-project
    cd my-rise-project
    ```

    This creates a project with:

    * `src/` - Your smart contracts
    * `test/` - Your tests
    * `script/` - Deployment scripts
    * `foundry.toml` - Configuration file
  </Step>

  <Step>
    ### Configure for RISE

    Update your `foundry.toml` to add the RISE network:

    ```toml title="foundry.toml"
    [profile.default]
    src = "src"
    out = "out"
    libs = ["lib"]
    solc = "0.8.30"

    [rpc_endpoints]
    rise = "https://testnet.riselabs.xyz"
    ```
  </Step>

  <Step>
    ### Set Environment Variables

    Create a `.env` file for your private key:

    ```bash title=".env"
    PRIVATE_KEY=your_private_key_here
    ```

    Load it in your shell:

    ```bash
    source .env
    ```

    **Never commit your `.env` file to version control.**
  </Step>

  <Step>
    ### Review the Default Contract

    Foundry creates a default `Counter.sol` contract:

    ```solidity title="src/Counter.sol"
    // SPDX-License-Identifier: UNLICENSED
    pragma solidity ^0.8.13;

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
    ### Build the Project

    Compile your contracts:

    ```bash
    forge build
    ```

    This generates artifacts in the `out/` directory.
  </Step>
</Steps>

## Next Steps

<Cards>
  <Card title="Compiling Contracts" href="/docs/builders/smart-contracts/foundry/compiling" description="Learn about compilation options" />

  <Card title="Testing Contracts" href="/docs/builders/smart-contracts/foundry/testing" description="Write and run tests" />

  <Card title="Deploying Contracts" href="/docs/builders/smart-contracts/foundry/deploying" description="Deploy to RISE Testnet" />

  <Card title="Verifying Contracts" href="/docs/builders/smart-contracts/foundry/verifying" description="Verify on the explorer" />
</Cards>
