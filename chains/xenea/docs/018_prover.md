# Testing Contracts (/docs/builders/smart-contracts/hardhat/testing)

import { Step, Steps } from 'fumadocs-ui/components/steps';
import { Tab, Tabs } from 'fumadocs-ui/components/tabs';

Hardhat includes a built-in testing framework using Mocha and Chai. Write tests to ensure your contracts work correctly before deploying to RISE.

## Write Tests

<Steps>
  <Step>
    ### Create Test File

    Make sure you're in your project's root directory and create a new directory called `test`.

    ```bash
    mkdir test
    ```

    Create a test file in the `test/` directory using ESM syntax:

    ```typescript title="test/Counter.ts"
    import hre from "hardhat";
    import { expect } from "chai";

    const { ethers } = await hre.network.connect();

    describe("Counter", function () {
      it("should start with 0", async function () {
        const counter = await ethers.deployContract("Counter");
        expect(await counter.number()).to.equal(0n);
      });

      it("should increment", async function () {
        const counter = await ethers.deployContract("Counter");
        await counter.increment();
        expect(await counter.number()).to.equal(1n);
      });

      it("should set number", async function () {
        const counter = await ethers.deployContract("Counter");
        await counter.setNumber(42n);
        expect(await counter.number()).to.equal(42n);
      });
    });
    ```

    Note: In Hardhat 3, you explicitly create network connections with `hre.network.connect()`.
  </Step>

  <Step>
    ### Run Tests

    Execute your tests:

    <Tabs items={['npm', 'yarn', 'pnpm', 'bun']} groupId="package-manager">
      <Tab value="npm">
        ```bash
        npx hardhat test
        ```
      </Tab>

      <Tab value="yarn">
        ```bash
        yarn hardhat test
        ```
      </Tab>

      <Tab value="pnpm">
        ```bash
        pnpm exec hardhat test
        ```
      </Tab>

      <Tab value="bun">
        ```bash
        bun x hardhat test
        ```
      </Tab>
    </Tabs>

    Output:

    ```
    Counter
      ✔ should start with 0
      ✔ should increment
      ✔ should set number

    3 passing
    ```
  </Step>
</Steps>

## Run Specific Tests

Run a single test file:

<Tabs items={['npm', 'yarn', 'pnpm', 'bun']} groupId="package-manager">
  <Tab value="npm">
    ```bash
    npx hardhat test test/Counter.ts
    ```
  </Tab>

  <Tab value="yarn">
    ```bash
    yarn hardhat test test/Counter.ts
    ```
  </Tab>

  <Tab value="pnpm">
    ```bash
    pnpm exec hardhat test test/Counter.ts
    ```
  </Tab>

  <Tab value="bun">
    ```bash
    bun x hardhat test test/Counter.ts
    ```
  </Tab>
</Tabs>

Run tests matching a pattern:

<Tabs items={['npm', 'yarn', 'pnpm', 'bun']} groupId="package-manager">
  <Tab value="npm">
    ```bash
    npx hardhat test --grep "increment"
    ```
  </Tab>

  <Tab value="yarn">
    ```bash
    yarn hardhat test --grep "increment"
    ```
  </Tab>

  <Tab value="pnpm">
    ```bash
    pnpm exec hardhat test --grep "increment"
    ```
  </Tab>

  <Tab value="bun">
    ```bash
    bun x hardhat test --grep "increment"
    ```
  </Tab>
</Tabs>

## Next Steps

<Cards>
  <Card title="Deploying Contracts" href="/docs/builders/smart-contracts/hardhat/deploying" description="Deploy tested contracts to RISE" />

  <Card title="Verifying Contracts" href="/docs/builders/smart-contracts/hardhat/verifying" description="Verify on the explorer" />
</Cards>
