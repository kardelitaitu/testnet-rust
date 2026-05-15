# Testing Contracts (/docs/builders/smart-contracts/foundry/testing)

import { Step, Steps } from 'fumadocs-ui/components/steps';

Foundry includes a powerful testing framework that lets you write tests in Solidity. Tests run extremely fast compared to JavaScript-based testing frameworks.

## Write Tests

<Steps>
  <Step>
    ### Create Test File

    Create a test file in the `test/` directory:

    ```solidity title="test/Counter.t.sol"
    // SPDX-License-Identifier: UNLICENSED
    pragma solidity ^0.8.13;

    import {Test, console} from "forge-std/Test.sol";
    import {Counter} from "../src/Counter.sol";

    contract CounterTest is Test {
        Counter public counter;

        function setUp() public {
            counter = new Counter();
        }

        function test_InitialValue() public view {
            assertEq(counter.number(), 0);
        }

        function test_Increment() public {
            counter.increment();
            assertEq(counter.number(), 1);
        }

        function test_SetNumber() public {
            counter.setNumber(42);
            assertEq(counter.number(), 42);
        }

        function testFuzz_SetNumber(uint256 x) public {
            counter.setNumber(x);
            assertEq(counter.number(), x);
        }
    }
    ```
  </Step>

  <Step>
    ### Run Tests

    Execute your tests:

    ```bash
    forge test
    ```

    Output:

    ```
    Running 4 tests for test/Counter.t.sol:CounterTest
    [PASS] test_InitialValue() (gas: 5453)
    [PASS] test_Increment() (gas: 28334)
    [PASS] test_SetNumber() (gas: 28312)
    [PASS] testFuzz_SetNumber(uint256) (runs: 256, μ: 27564, ~: 28387)
    Test result: ok. 4 passed; 0 failed; finished in 10.23ms
    ```
  </Step>
</Steps>

## Verbose Output

See detailed test output with `-v` flags:

```bash
forge test -vv    # Show logs
forge test -vvv   # Show execution traces
forge test -vvvv  # Show full traces including setup
```

## Gas Reports

Generate gas usage reports:

```bash
forge test --gas-report
```

## Run Specific Tests

Run a single test file:

```bash
forge test --match-path test/Counter.t.sol
```

Run tests matching a pattern:

```bash
forge test --match-test test_Increment
```

Run tests in a specific contract:

```bash
forge test --match-contract CounterTest
```

## Fuzz Testing

Foundry automatically runs fuzz tests on functions prefixed with `testFuzz_`:

```solidity
function testFuzz_SetNumber(uint256 x) public {
    counter.setNumber(x);
    assertEq(counter.number(), x);
}
```

Configure fuzz runs in `foundry.toml`:

```toml
[fuzz]
runs = 1000
```

## Cheatcodes

Foundry provides cheatcodes for testing advanced scenarios:

```solidity
// Set block timestamp
vm.warp(1641070800);

// Set msg.sender
vm.prank(address(0x1234));

// Expect a revert
vm.expectRevert("Error message");

// Deal ETH to an address
vm.deal(address(0x1234), 1 ether);
```

## Next Steps

<Cards>
  <Card title="Deploying Contracts" href="/docs/builders/smart-contracts/foundry/deploying" description="Deploy tested contracts to RISE" />

  <Card title="Verifying Contracts" href="/docs/builders/smart-contracts/foundry/verifying" description="Verify on the explorer" />
</Cards>
