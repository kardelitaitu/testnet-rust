# Get Started with Viem (/docs/builders/frontend/viem/get-started)

Learn how to set up and configure Viem for building on RISE.

## Installation

Install Viem using your preferred package manager:

```bash
npm install viem
```

## Project Setup

Create a new TypeScript project:

```bash
mkdir my-rise-viem-project
cd my-rise-viem-project
npm init -y
npm install --save-dev typescript @types/node
npx tsc --init
```

## Import RISE Testnet

RISE Testnet is available in Viem's built-in chains:

```typescript
import { riseTestnet } from 'viem/chains'
```

That's it! No need to manually configure the chain.

## Create Clients

### Public Client (Read-only)

For reading blockchain data:

```typescript
import { createPublicClient, http } from 'viem'
import { riseTestnet } from 'viem/chains'

const publicClient = createPublicClient({
  chain: riseTestnet,
  transport: http()
})

// Get block number
const blockNumber = await publicClient.getBlockNumber()
console.log('Block number:', blockNumber)

// Get account balance
const balance = await publicClient.getBalance({
  address: '0x...'
})
console.log('Balance:', balance)
```

### Wallet Client (Read & Write)

For sending transactions:

```typescript
import { createWalletClient, http } from 'viem'
import { privateKeyToAccount } from 'viem/accounts'
import { riseTestnet } from 'viem/chains'

const account = privateKeyToAccount('0xYOUR_PRIVATE_KEY')

const walletClient = createWalletClient({
  account,
  chain: riseTestnet,
  transport: http()
})

// Send a transaction
const hash = await walletClient.sendTransaction({
  to: '0x...',
  value: 1000000000000000000n // 1 ETH
})
console.log('Transaction hash:', hash)
```

## Environment Variables

Create a `.env` file for sensitive data:

```bash
PRIVATE_KEY=0x...
RPC_URL=https://testnet.riselabs.xyz
```

Load environment variables:

```typescript
import { config } from 'dotenv'
config()

const account = privateKeyToAccount(process.env.PRIVATE_KEY as `0x${string}`)
```

## Example: Complete Setup

```typescript
// index.ts
import { createPublicClient, createWalletClient, http } from 'viem'
import { privateKeyToAccount } from 'viem/accounts'
import { riseTestnet } from 'viem/chains'

// Initialize clients
const publicClient = createPublicClient({
  chain: riseTestnet,
  transport: http()
})

const account = privateKeyToAccount(process.env.PRIVATE_KEY as `0x${string}`)

const walletClient = createWalletClient({
  account,
  chain: riseTestnet,
  transport: http()
})

// Get account info
const address = account.address
const balance = await publicClient.getBalance({ address })
const blockNumber = await publicClient.getBlockNumber()

console.log('Address:', address)
console.log('Balance:', balance)
console.log('Latest block:', blockNumber)
```

## Next Steps

* [Reading from Contracts](/docs/builders/viem/reading) - Query contract data
* [Writing to Contracts](/docs/builders/viem/writing) - Send transactions and interact with contracts
