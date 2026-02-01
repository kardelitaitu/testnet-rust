# Remix IDE (/docs/builders/smart-contracts/remix)

import { Step, Steps } from 'fumadocs-ui/components/steps';

[Remix IDE](https://remix.ethereum.org) is a browser-based development environment for writing, compiling, and deploying smart contracts. No installation required.

## Prerequisites

* A Web3 wallet (MetaMask, Rabby or similar) with RISE Testnet configured
* Testnet ETH from the [RISE Faucet](https://faucet.testnet.riselabs.xyz/)

## Deploy a Contract

<Steps>
  <Step>
    ### Create the Contract

    Open [Remix IDE](https://remix.ethereum.org) and create a new file called `Counter.sol` in the File Explorer. Add the following code:

    ```solidity title="Counter.sol"
    // SPDX-License-Identifier: MIT
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

    1. Click the **Solidity Compiler** tab (in the left sidebar)
    2. Select compiler version `0.8.30` or higher
    3. Click **Compile Counter.sol**

    <Callout type="info">
      Enable **Auto compile** in the compiler settings for a better development experience. Your contracts will automatically compile as you make changes.
    </Callout>
  </Step>

  <Step>
    ### Deploy to RISE Testnet

    1. Click the **Deploy & Run Transactions** tab (in the left sidebar)
    2. In the **Environment** dropdown, select **Injected Provider - MetaMask (or whichever wallet you have)**
    3. Your wallet will prompt you to connect - approve the connection
    4. **Important**: Make sure your wallet is connected to RISE Testnet (Chain ID: 11155931)
    5. Ensure `Counter` is selected in the **Contract** dropdown
    6. Click **Deploy**
    7. Confirm the transaction in your wallet
  </Step>

  <Step>
    ### Interact with the Contract

    Once deployed, your contract will appear under **Deployed Contracts**:

    * Click `number` to read the current value (starts at 0)
    * Enter a value and click `setNumber` to set a new number
    * Click `increment` to increase the number by 1

    Each write operation (`setNumber`, `increment`) will require a transaction confirmation in your wallet.
  </Step>
</Steps>

## View on Explorer

After deployment, you can view your contract on the [RISE Testnet Explorer](https://explorer.testnet.riselabs.xyz) by searching for the contract address shown in Remix.
