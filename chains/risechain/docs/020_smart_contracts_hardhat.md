# Verifying Contracts (/docs/builders/smart-contracts/hardhat/verifying)

import { Step, Steps } from 'fumadocs-ui/components/steps';
import { Tab, Tabs } from 'fumadocs-ui/components/tabs';

Verify your deployed contracts on the RISE Testnet Explorer so users can view and interact with your contract's source code.

## Prerequisites

* Contract deployed to RISE (see [Deploying](/docs/builders/smart-contracts/hardhat/deploying))
* Contract address from deployment

## Verify with Hardhat Ignition

<Steps>
  <Step>
    ### Configure Verification

    Add the RISE Explorer configuration to your `hardhat.config.ts`:

    ```typescript title="hardhat.config.ts" {3,17-24}
    import hardhatToolboxViemPlugin from "@nomicfoundation/hardhat-toolbox-viem";
    import { configVariable, defineConfig } from "hardhat/config";
    import "@nomicfoundation/hardhat-verify";

    export default defineConfig({
      plugins: [hardhatToolboxViemPlugin],
      solidity: {
        version: "0.8.30",
      },
      networks: {
        rise: {
          type: "http",
          url: "https://testnet.riselabs.xyz",
          accounts: [configVariable("RISE_PRIVATE_KEY")],
          chainId: 11155931
        }
      },
      verify: {
        blockscout: {
          networks: {
            rise: "https://explorer.testnet.riselabs.xyz/api"
          }
        }
      }
    });
    ```
  </Step>

  <Step>
    ### Verify with Ignition

    If you deployed with Hardhat Ignition, simply add the `--verify` flag:

    <Tabs items={['npm', 'yarn', 'pnpm', 'bun']} groupId="package-manager">
      <Tab value="npm">
        ```bash
        npx hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet --verify
        ```
      </Tab>

      <Tab value="yarn">
        ```bash
        yarn hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet --verify
        ```
      </Tab>

      <Tab value="pnpm">
        ```bash
        pnpm exec hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet --verify
        ```
      </Tab>

      <Tab value="bun">
        ```bash
        bun x hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet --verify
        ```
      </Tab>
    </Tabs>

    <Callout type="info">
      Since you already deployed the contract, Ignition won't re-deploy it. It will only submit the source code for verification.
    </Callout>
  </Step>

  <Step>
    ### Check Verification

    You'll see output indicating successful verification with a link to view the contract on the RISE Explorer.

    Visit [explorer.testnet.riselabs.xyz](https://explorer.testnet.riselabs.xyz) and search for your contract address to see the verified source code.
  </Step>
</Steps>

## Troubleshooting

### Already Verified

If you see "Already Verified", the contract source code has already been submitted. No further action needed.

### Wrong Constructor Arguments

If verification fails, double-check your constructor arguments match exactly what was used during deployment.

### Compiler Version Mismatch

Make sure the Solidity version in your `hardhat.config` matches the version used in your contract.

## Next Steps

Your verified contract is now publicly viewable on the RISE Explorer. Users can:

* Read the source code
* Interact with contract functions directly through the explorer
* View contract events and transactions
