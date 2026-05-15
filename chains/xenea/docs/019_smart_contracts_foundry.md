# Deploying Contracts (/docs/builders/smart-contracts/hardhat/deploying)

import { Step, Steps } from 'fumadocs-ui/components/steps';
import { Tab, Tabs } from 'fumadocs-ui/components/tabs';

Deploy your compiled smart contracts to the RISE Testnet using Hardhat Ignition, our official deployment plugin.

## Prerequisites

* Hardhat project configured for RISE (see [Get Started](/docs/builders/smart-contracts/hardhat/get-started))
* Testnet ETH from the [RISE Faucet](https://faucet.testnet.riselabs.xyz/)
* Private key configured with Configuration Variables

## Deploy with Hardhat Ignition

<Steps>
  <Step>
    ### Write a Deployment Module

    Hardhat Ignition uses modules to describe deployments. Create `ignition/modules/Counter.ts`:

    ```typescript title="ignition/modules/Counter.ts"
    import { buildModule } from "@nomicfoundation/hardhat-ignition/modules";

    export default buildModule("CounterModule", (m) => {
      const counter = m.contract("Counter");

      return { counter };
    });
    ```

    This module deploys an instance of the `Counter` contract. You can also call functions after deployment:

    ```typescript title="ignition/modules/Counter.ts"
    import { buildModule } from "@nomicfoundation/hardhat-ignition/modules";

    export default buildModule("CounterModule", (m) => {
      const counter = m.contract("Counter");

      // Call a function after deployment
      m.call(counter, "setNumber", [42n]);

      return { counter };
    });
    ```
  </Step>

  <Step>
    ### Test Your Module Locally

    Before deploying to a live network, test the module locally:

    <Tabs items={['npm', 'yarn', 'pnpm', 'bun']} groupId="package-manager">
      <Tab value="npm">
        ```bash
        npx hardhat ignition deploy ignition/modules/Counter.ts
        ```
      </Tab>

      <Tab value="yarn">
        ```bash
        yarn hardhat ignition deploy ignition/modules/Counter.ts
        ```
      </Tab>

      <Tab value="pnpm">
        ```bash
        pnpm exec hardhat ignition deploy ignition/modules/Counter.ts
        ```
      </Tab>

      <Tab value="bun">
        ```bash
        bun x hardhat ignition deploy ignition/modules/Counter.ts
        ```
      </Tab>
    </Tabs>

    This simulates the deployment in a local network to verify everything works.
  </Step>

  <Step>
    ### Deploy to RISE Testnet

    Deploy your contract to RISE:

    <Tabs items={['npm', 'yarn', 'pnpm', 'bun']} groupId="package-manager">
      <Tab value="npm">
        ```bash
        npx hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet
        ```
      </Tab>

      <Tab value="yarn">
        ```bash
        yarn hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet
        ```
      </Tab>

      <Tab value="pnpm">
        ```bash
        pnpm exec hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet
        ```
      </Tab>

      <Tab value="bun">
        ```bash
        bun x hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet
        ```
      </Tab>
    </Tabs>

    You'll see output indicating successful deployment with the contract address.

    <Callout type="info">
      Ignition remembers deployment state. If you run the same deployment again, nothing will happen because the module was already executed.
    </Callout>
  </Step>

  <Step>
    ### View on Explorer

    View your deployed contract on the [RISE Testnet Explorer](https://explorer.testnet.riselabs.xyz) by searching for the contract address from the deployment output.
  </Step>
</Steps>

## Deploy with Constructor Arguments

If your contract has constructor parameters:

```typescript title="ignition/modules/Counter.ts"
import { buildModule } from "@nomicfoundation/hardhat-ignition/modules";

export default buildModule("CounterModule", (m) => {
  const initialValue = m.getParameter("initialValue", 42n);

  const counter = m.contract("Counter", [initialValue]);

  return { counter };
});
```

Deploy with parameters:

<Tabs items={['npm', 'yarn', 'pnpm', 'bun']} groupId="package-manager">
  <Tab value="npm">
    ```bash
    npx hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet --parameters '{"CounterModule":{"initialValue":"100"}}'
    ```
  </Tab>

  <Tab value="yarn">
    ```bash
    yarn hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet --parameters '{"CounterModule":{"initialValue":"100"}}'
    ```
  </Tab>

  <Tab value="pnpm">
    ```bash
    pnpm exec hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet --parameters '{"CounterModule":{"initialValue":"100"}}'
    ```
  </Tab>

  <Tab value="bun">
    ```bash
    bun x hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet --parameters '{"CounterModule":{"initialValue":"100"}}'
    ```
  </Tab>
</Tabs>

## Next Steps

<Cards>
  <Card title="Verifying Contracts" href="/docs/builders/smart-contracts/hardhat/verifying" description="Verify your deployed contract on the explorer" />
</Cards>
