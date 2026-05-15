# Deploying Contracts (/docs/builders/smart-contracts/foundry/deploying)

import { Step, Steps } from 'fumadocs-ui/components/steps';

Deploy your compiled smart contracts to the RISE Testnet using Foundry's `forge create` or deployment scripts.

## Prerequisites

* Foundry project configured for RISE (see [Get Started](/docs/builders/smart-contracts/foundry/get-started))
* Testnet ETH from the [RISE Faucet](https://faucet.testnet.riselabs.xyz/)
* Private key set in environment variables

## Deploy with forge create

The simplest way to deploy a contract:

```bash
forge create \
  --rpc-url https://testnet.riselabs.xyz \
  --private-key $PRIVATE_KEY \
  src/Counter.sol:Counter
```

Output:

```
Deployer: 0x1234...
Deployed to: 0xabcd...
Transaction hash: 0x5678...
```

### With Constructor Arguments

If your contract has constructor arguments:

```bash
forge create \
  --rpc-url https://testnet.riselabs.xyz \
  --private-key $PRIVATE_KEY \
  src/Lock.sol:Lock \
  --constructor-args 1706745600
```

### With ETH Value

To send ETH during deployment:

```bash
forge create \
  --rpc-url https://testnet.riselabs.xyz \
  --private-key $PRIVATE_KEY \
  src/Lock.sol:Lock \
  --constructor-args 1706745600 \
  --value 0.001ether
```

## Deploy with Scripts

For more complex deployments, use Forge scripts:

<Steps>
  <Step>
    ### Create Deploy Script

    Create a deployment script in `script/Counter.s.sol`:

    ```solidity title="script/Counter.s.sol"
    // SPDX-License-Identifier: UNLICENSED
    pragma solidity ^0.8.13;

    import {Script, console} from "forge-std/Script.sol";
    import {Counter} from "../src/Counter.sol";

    contract CounterScript is Script {
        function run() public {
            uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");

            vm.startBroadcast(deployerPrivateKey);

            Counter counter = new Counter();
            console.log("Counter deployed to:", address(counter));

            vm.stopBroadcast();
        }
    }
    ```
  </Step>

  <Step>
    ### Run Deployment Script

    Deploy to RISE Testnet:

    ```bash
    forge script script/Counter.s.sol:CounterScript \
      --rpc-url https://testnet.riselabs.xyz \
      --broadcast
    ```

    Add `-vvvv` for verbose output to see transaction details.
  </Step>

  <Step>
    ### View on Explorer

    View your deployed contract on the [RISE Testnet Explorer](https://explorer.testnet.riselabs.xyz) by searching for the contract address.
  </Step>
</Steps>

## Deploy and Verify

Deploy and verify in a single command:

```bash
forge create \
  --rpc-url https://testnet.riselabs.xyz \
  --private-key $PRIVATE_KEY \
  src/Counter.sol:Counter \
  --verify \
  --verifier blockscout \
  --verifier-url https://explorer.testnet.riselabs.xyz/api/
```

## Using RPC Endpoint Aliases

If you configured `foundry.toml` with RPC endpoints, use the alias:

```bash
forge create \
  --rpc-url rise \
  --private-key $PRIVATE_KEY \
  src/Counter.sol:Counter
```

## Next Steps

<Cards>
  <Card title="Verifying Contracts" href="/docs/builders/smart-contracts/foundry/verifying" description="Verify your deployed contract on the explorer" />
</Cards>
