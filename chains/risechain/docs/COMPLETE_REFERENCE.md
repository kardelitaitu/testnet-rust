# RISE Chain Documentation - Complete Reference

## 1. Quick Reference

### Network Configuration

| Property | Testnet Value |
|----------|---------------|
| Network Name | RISE Testnet |
| Chain ID | 11155931 |
| RPC URL | https://testnet.riselabs.xyz |
| Explorer | https://explorer.testnet.riselabs.xyz |
| Currency Symbol | ETH |
| Max Gas per Transaction | 16M |
| Blocks to Finality | ~3 days (259,200 blocks) |

### Important Addresses

#### L2 Predeployed Contracts

| Contract | Address |
|----------|---------|
| L2ToL1MessagePasser | 0x4200000000000000000000000000000000000016 |
| L2CrossDomainMessenger | 0x4200000000000000000000000000000000000007 |
| L2StandardBridge | 0x4200000000000000000000000000000000000010 |
| L2ERC721Bridge | 0x4200000000000000000000000000000000000014 |
| SequencerFeeVault | 0x4200000000000000000000000000000000000011 |
| OptimismMintableERC20Factory | 0x4200000000000000000000000000000000000012 |
| OptimismMintableERC721Factory | 0x4200000000000000000000000000000000000017 |
| L1Block | 0x4200000000000000000000000000000000000015 |
| GasPriceOracle | 0x420000000000000000000000000000000000000F |
| ProxyAdmin | 0x4200000000000000000000000000000000000018 |
| BaseFeeVault | 0x4200000000000000000000000000000000000019 |
| L1FeeVault | 0x420000000000000000000000000000000000001A |
| GovernanceToken | 0x4200000000000000000000000000000000000042 |
| SchemaRegistry | 0x4200000000000000000000000000000000000020 |
| EAS | 0x4200000000000000000000000000000000000021 |
| WETH | 0x4200000000000000000000000000000000000006 |
| Create2Deployer | 0x13b0D85CcB8bf860b6b79AF3029fCA081AE9beF2 |
| MultiCall3 | 0xcA11bde05977b3631167028862bE2a173976CA11 |
| Permit2 | 0x000000000022D473030F116dDEE9F6B43aC78BA3 |
| EntryPoint (v0.7.0) | 0x0000000071727De22E5E9d8BAf0edAc6f37da032 |

#### Testnet Tokens

| Token | Symbol | Decimals | Address |
|-------|--------|----------|---------|
| Wrapped ETH | WETH | 18 | 0x4200000000000000000000000000000000000006 |
| USD Coin | USDC | 6 | 0x8a93d247134d91e0de6f96547cb0204e5be8e5d8 |
| Tether USD | USDT | 8 | 0x40918ba7f132e0acba2ce4de4c4baf9bd2d7d849 |
| Wrapped Bitcoin | WBTC | 18 | 0xf32d39ff9f6aa7a7a64d7a4f00a54826ef791a55 |
| RISE | RISE | 18 | 0xd6e1afe5ca8d00a2efc01b89997abe2de47fdfaf |
| Mog Coin | MOG | 18 | 0x99dbe4aea58e518c50a1c04ae9b48c9f6354612f |
| Pepe | PEPE | 18 | 0x6f6f570f45833e249e27022648a26f4076f48f78 |

#### Internal Price Oracles

| Ticker | Address |
|--------|---------|
| ETH | 0x7114E2537851e727678DE5a96C8eE5d0Ca14f03D |
| USDC | 0x50524C5bDa18aE25C600a8b81449B9CeAeB50471 |
| USDT | 0x9190159b1bb78482Dca6EBaDf03ab744de0c0197 |
| BTC | 0xadDAEd879D549E5DBfaf3e35470C20D8C50fDed0 |

#### L1 (Sepolia) System Contracts

| Contract | Address |
|----------|---------|
| AnchorStateRegistryProxy | 0x5ca4bfe196aa3a1ed9f8522f224ec5a7a7277d5a |
| BatchSubmitter | 0x45Bd8Bc15FfC21315F8a1e3cdF67c73b487768e8 |
| Challenger | 0xb49077bAd82968A1119B9e717DBCFb9303E91f0F |
| DisputeGameFactoryProxy | 0x790e18c477bfb49c784ca0aed244648166a5022b |
| L1CrossDomainMessengerProxy | 0xcc1c4f905d0199419719f3c3210f43bb990953fc |
| L1StandardBridgeProxy | 0xe9a531a5d7253c9823c74af155d22fe14568b610 |
| OptimismPortalProxy | 0x77cce5cd26c75140c35c38104d0c655c7a786acb |
| SystemConfigProxy | 0x5088a091bd20343787c5afc95aa002d13d9f3535 |

---

## 2. Architecture Overview

### Core Components

RISE is a high-performance EVM-compatible Layer 2 rollup designed for speed and scalability.

**Key Performance Targets:**
- 100,000+ TPS (transactions per second)
- Sub-10ms end-to-end latency
- Over 1 GGas/s execution

**Main Components:**

1. **Execution Engine**
   - Continuous Block Pipeline (CBP) - executes transactions while in mempool
   - Shreds - efficient interrupt-driven block construction
   - Parallel EVM (PEVM) - parallel transaction execution (future)

2. **Data Availability**
   - Primary: EigenDA (100 MB/s throughput, 5s confirmation)
   - Fallback: Ethereum blobs (EIP-4844)

3. **Settlement**
   - ZK Fraud Proofs using OP Succinct Lite
   - Hybrid approach: optimistic with ZK proof on challenge

4. **Decentralization**
   - Based Sequencing - leveraging Ethereum L1 validators
   - Cryptographically enforced preconfirmations
   - Rotating gateways

---

## 3. Core Components Details

### Continuous Block Pipeline (CBP)

Traditional L2s have sequential block production: Consensus -> Execution -> Merkleization. CBP restructures this to maximize execution time.

**Key Principles:**
- Execution starts immediately when mempool has transactions (not waiting for consensus)
- First block in epoch requires consensus (L1 derivation)
- Subsequent blocks in epoch can execute immediately after previous block finishes
- Merkleization runs in parallel with next block's execution

**Benefits:**
- Continuous execution - no idle time waiting for consensus
- Higher throughput - more block time allocated to execution
- Optimized mempool - pre-orders transactions to maximize parallelization

### Shreds

Shreds partition L2 blocks (12 seconds) into smaller segments (sub-second) without state roots. This enables millisecond-level preconfirmations.

**How Shreds Work:**
1. Sequencer groups transactions into a Shred
2. Shred is signed and broadcast via P2P network
3. Each Shred contains a ChangeSetRoot (commits to state changes)
4. Nodes optimistically apply changes without re-executing
5. Merkleization only at L2 block level (after all Shreds)

**Advantages:**
- Sub-10ms transaction confirmations
- Reduced latency without sacrificing security
- Efficient batch processing

### ZK Fraud Proofs

RISE uses a hybrid approach combining optimistic rollups with ZK proving:

**Phase 1 (Current):**
- Sequencer posts state commitments optimistically
- Challengers can trigger fraud challenges
- On challenge, sequencer must provide ZK validity proof
- Most of the time (99.9999%), proofs are never needed

**Phase 2 (Future - Proactive Proving):**
- Sequencer voluntarily submits ZK proofs even without challenges
- Faster cryptographic finality

**Phase 3 (Future - Full ZK Rollup):**
- Validity proofs mandatory for every state transition
- Instant cryptographic finality as standard

### Based Sequencing

Replaces centralized sequencer with Ethereum L1 block proposers as sequencers.

**Key Concepts:**

1. **Preconfirmations (Preconfs)**
   - Cryptographically enforced promises from proposers
   - Slashing penalties for not honoring
   - Reduces latency from 12s (L1 block time) to milliseconds

2. **Gateways (Preconfers)**
   - Sophisticated entities that proposers delegate to
   - Handle sequencing while providing L1-secured preconfirmations
   - Face slashing for misbehavior

3. **Phases:**
   - Phase 1: Single gateway operated by RISE team
   - Phase 2: Multiple whitelisted gateways in rotation
   - Phase 3: Permissionless - any ETH validator can become gateway

---

## 4. Network Participants

### Participant Types

| Type | Role | Hardware Requirements |
|------|------|----------------------|
| **Sequencers** | Execute transactions, build blocks | 32GB RAM |
| **Replicas** | Sync via state-diff (no re-execution) | 8-16GB RAM |
| **Fullnodes** | Re-execute with metadata aids | 16-32GB RAM |
| **Challengers** | Run fullnodes, submit fraud challenges | 16-32GB RAM |
| **Provers** | Generate ZK proofs on demand | FPGA/GPU (rarely used) |

**Sequencers** - Process all transactions, maintain highest hardware requirements. Essential for network throughput.

**Replicas** - Apply state-diffs from sequencer rather than re-executing. Lightweight, suitable for indexing services and explorers.

**Fullnodes** - Re-execute transactions for independent verification. Security checkpoint - can detect invalid state transitions.

**Challengers** - Special fullnodes that submit fraud challenges. Only one honest challenger needed for network security.

**Provers** - Generate ZK validity proofs when challenges occur. On-demand, cost-effective despite hardware needs.

---

## 5. Transaction Lifecycle

1. **Transaction Submission**
   - User signs and submits transaction via RPC endpoint

2. **P2P Propagation**
   - RPC node validates and broadcasts to sequencer via P2P network

3. **Mempool Pre-Execution**
   - Sequencer pre-executes transactions while in mempool (CBP)
   - Results cached for immediate use upon Shred inclusion

4. **Shred Inclusion**
   - Transactions grouped into Shreds (mini-blocks)
   - Pre-executed results reused
   - ChangeSetRoot calculated for state commitment

5. **Shred Propagation**
   - Sequencer broadcasts Shred to P2P network
   - Nodes immediately apply changes (or execute)
   - Transaction receipt available - user gets confirmation

6. **L2 Block Formation**
   - After blocktime, Shreds batch into canonical L2 block
   - Full merkleization performed

7. **L1/DA Submission**
   - L2 blocks batched and posted to DA layer and L1
   - Transactions considered safe after challenge period

---

## 6. Building on RISE

### Hardhat Configuration

```javascript
// hardhat.config.js
module.exports = {
  networks: {
    riseTestnet: {
      url: "https://testnet.riselabs.xyz",
      chainId: 11155931,
      accounts: process.env.PRIVATE_KEY ? [process.env.PRIVATE_KEY] : []
    }
  }
};
```

### Foundry Configuration

```toml
# foundry.toml
[profile.default]
src = "src"
out = "out"
libs = ["lib"]

[rpc_endpoints]
rise_testnet = "https://testnet.riselabs.xyz"

[blockscout]
rise_testnet = { key = "", url = "https://explorer.testnet.riselabs.xyz/api" }
```

### Get Testnet ETH

Visit https://faucet.testnet.riselabs.xyz to get free testnet ETH and tokens.

---

## 7. Smart Contract Development

### Foundry Workflow

**Install:**
```bash
curl -L https://foundry.paradigm.xyz | bash
foundryup
```

**Create Project:**
```bash
forge init my-rise-project
cd my-rise-project
```

**Configure:**
```toml
# foundry.toml
[profile.default]
src = "src"
out = "out"
solc = "0.8.30"

[rpc_endpoints]
rise = "https://testnet.riselabs.xyz"

[etherscan]
rise = { key = "", url = "https://explorer.testnet.riselabs.xyz/api" }
```

**Deploy:**
```bash
forge create \
  --rpc-url https://testnet.riselabs.xyz \
  --private-key $PRIVATE_KEY \
  src/Counter.sol:Counter
```

**Deploy with Verification:**
```bash
forge create \
  --rpc-url https://testnet.riselabs.xyz \
  --private-key $PRIVATE_KEY \
  src/Counter.sol:Counter \
  --verify \
  --verifier blockscout \
  --verifier-url https://explorer.testnet.riselabs.xyz/api/
```

### Hardhat Workflow

**Install:**
```bash
mkdir my-rise-project
cd my-rise-project
npx hardhat init
npm install @nomicfoundation/hardhat-toolbox-viem
```

**Configure:**
```typescript
// hardhat.config.ts
import hardhatToolboxViemPlugin from "@nomicfoundation/hardhat-toolbox-viem";
import { defineConfig } from "hardhat/config";

export default defineConfig({
  plugins: [hardhatToolboxViemPlugin],
  solidity: { version: "0.8.30" },
  networks: {
    riseTestnet: {
      type: "http",
      url: "https://testnet.riselabs.xyz",
      accounts: ["<PRIVATE_KEY>"],
      chainId: 11155931
    }
  }
});
```

**Deploy with Ignition:**
```bash
npx hardhat ignition deploy ignition/modules/Counter.ts --network riseTestnet
```

### Testing

**Foundry (Solidity):**
```solidity
// test/Counter.t.sol
import {Test, console} from "forge-std/Test.sol";
import {Counter} from "../src/Counter.sol";

contract CounterTest is Test {
    function test_Increment() public {
        Counter counter = new Counter();
        counter.increment();
        assertEq(counter.number(), 1);
    }
}
```
```bash
forge test
```

**Hardhat (TypeScript):**
```typescript
// test/Counter.ts
import hre from "hardhat";
import { expect } from "chai";

const { ethers } = await hre.network.connect();

describe("Counter", function () {
    it("should increment", async function () {
        const counter = await ethers.deployContract("Counter");
        await counter.increment();
        expect(await counter.number()).to.equal(1n);
    });
});
```
```bash
npx hardhat test
```

### Contract Verification

**Foundry:**
```bash
forge verify-contract <ADDRESS> src/Counter.sol:Counter \
  --verifier blockscout \
  --verifier-url https://explorer.testnet.riselabs.xyz/api/
```

**Hardhat:**
```bash
npx hardhat ignition deploy ignition/modules/Counter.ts \
  --network riseTestnet \
  --verify
```

---

## 8. Frontend Integration

### Viem (Recommended)

**Install:**
```bash
npm install viem
```

**Setup:**
```typescript
import { createPublicClient, createWalletClient, http } from 'viem'
import { riseTestnet } from 'viem/chains'
import { privateKeyToAccount } from 'viem/accounts'

// Public client (read-only)
const publicClient = createPublicClient({
  chain: riseTestnet,
  transport: http()
})

// Wallet client (write)
const account = privateKeyToAccount('0xYOUR_PRIVATE_KEY')
const walletClient = createWalletClient({
  account,
  chain: riseTestnet,
  transport: http()
})
```

**Read Contract:**
```typescript
const result = await publicClient.readContract({
  address: '0x...',
  abi: [{ name: 'balanceOf', type: 'function', stateMutability: 'view', inputs: [{ name: 'owner', type: 'address' }], outputs: [{ type: 'uint256' }] }],
  functionName: 'balanceOf',
  args: ['0x...']
})
```

**Write Contract:**
```typescript
const hash = await walletClient.writeContract({
  address: '0x...',
  abi: [{ name: 'transfer', type: 'function', stateMutability: 'nonpayable', inputs: [{ name: 'to', type: 'address' }, { name: 'amount', type: 'uint256' }], outputs: [{ type: 'bool' }] }],
  functionName: 'transfer',
  args: ['0x...', 1000000000000000000n]
})

// Wait for confirmation
const receipt = await publicClient.waitForTransactionReceipt({ hash })
```

**Batch Reads (Multicall):**
```typescript
const results = await publicClient.multicall({
  contracts: [
    { address: '0x...', abi, functionName: 'name' },
    { address: '0x...', abi, functionName: 'symbol' },
    { address: '0x...', abi, functionName: 'totalSupply' },
  ]
})
```

### Ethers.js

**Install:**
```bash
npm install ethers
```

**Setup:**
```typescript
import { ethers } from 'ethers'

const provider = new ethers.JsonRpcProvider('https://testnet.riselabs.xyz')
const wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider)
```

### Web3.js

**Install:**
```bash
npm install web3
```

**Setup:**
```typescript
import Web3 from 'web3'

const web3 = new Web3('https://testnet.riselabs.xyz')
```

---

## 9. Bridging

### L1 to L2 (Deposit ETH)

```solidity
// On Sepolia (L1)
IL1StandardBridge bridge = IL1StandardBridge(0xe9a531a5d7253c9823c74af155d22fe14568b610);

bridge.depositETH{value: amount}(
    minGasLimit,
    emptyBytes  // No additional data
);
```

### L2 to L1 (Withdraw)

```solidity
// On RISE Testnet (L2)
IL2CrossDomainMessenger messenger = IL2CrossDomainMessenger(0x4200000000000000000000000000000000000007);

messenger.sendMessage(
    targetL1Address,
    abi.encodeWithSignature("someFunction(uint256)", value),
    minGasLimit
);
```

### Send Messages L2 to L1

```solidity
// After withdrawal is initiated, prove on L1 and finalize
// Complete process takes ~3 days (challenge period)
```

---

## 10. Example Commands

### Check Token Balance (Foundry)
```bash
cast call <TOKEN_ADDRESS> "balanceOf(address)(uint256)" <YOUR_ADDRESS> --rpc-url https://testnet.riselabs.xyz
```

### Transfer Tokens (Foundry)
```bash
cast send <TOKEN_ADDRESS> "transfer(address,uint256)(bool)" <RECIPIENT> <AMOUNT> --private-key $PRIVATE_KEY --rpc-url https://testnet.riselabs.xyz
```

### Wrap ETH (Foundry)
```bash
cast send 0x4200000000000000000000000000000000000006 "deposit()" --value <AMOUNT_WEI> --private-key $PRIVATE_KEY --rpc-url https://testnet.riselabs.xyz
```

### Get ETH Price from Oracle
```solidity
interface IPriceOracle {
    function latest_answer() external view returns (int256);
}

IPriceOracle ethOracle = IPriceOracle(0x7114E2537851e727678DE5a96C8eE5d0Ca14f03D);
int256 ethPrice = ethOracle.latest_answer();
```

---

## 11. Development Resources

### Faucet
- https://faucet.testnet.riselabs.xyz

### Explorers
- Block Explorer: https://explorer.testnet.riselabs.xyz
- API: https://explorer.testnet.riselabs.xyz/api

### Documentation
- This file serves as a complete reference
- For latest updates, check official RISE documentation

### Support
- Discord: https://discord.com/invite/qhKnePXdSM
