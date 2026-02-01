# Reading Contract Data (/docs/builders/frontend/viem/reading)

Learn how to read data from smart contracts on RISE using Viem.

## Setup

First, create a public client:

```typescript
import { createPublicClient, http } from 'viem'
import { riseTestnet } from './config'

const client = createPublicClient({
  chain: riseTestnet,
  transport: http()
})
```

## Read Contract

Use `readContract` to call view/pure functions:

```typescript
const result = await client.readContract({
  address: '0x...',
  abi: [
    {
      name: 'balanceOf',
      type: 'function',
      stateMutability: 'view',
      inputs: [{ name: 'owner', type: 'address' }],
      outputs: [{ type: 'uint256' }],
    },
  ],
  functionName: 'balanceOf',
  args: ['0x...']
})

console.log('Balance:', result)
```

## Multiple Reads (Multicall)

Batch multiple contract reads efficiently:

```typescript
import { parseAbi } from 'viem'

const abi = parseAbi([
  'function name() view returns (string)',
  'function symbol() view returns (string)',
  'function totalSupply() view returns (uint256)',
])

const results = await client.multicall({
  contracts: [
    {
      address: '0x...',
      abi,
      functionName: 'name',
    },
    {
      address: '0x...',
      abi,
      functionName: 'symbol',
    },
    {
      address: '0x...',
      abi,
      functionName: 'totalSupply',
    },
  ],
})

console.log('Name:', results[0].result)
console.log('Symbol:', results[1].result)
console.log('Total Supply:', results[2].result)
```

## Get Contract Events

Query past events from a contract:

```typescript
import { parseAbiItem } from 'viem'

const logs = await client.getLogs({
  address: '0x...',
  event: parseAbiItem('event Transfer(address indexed from, address indexed to, uint256 value)'),
  fromBlock: 0n,
  toBlock: 'latest'
})

console.log('Transfer events:', logs)
```

## Watch Contract Events

Subscribe to realtime contract events:

```typescript
const unwatch = client.watchContractEvent({
  address: '0x...',
  abi: parseAbi(['event Transfer(address indexed from, address indexed to, uint256 value)']),
  eventName: 'Transfer',
  onLogs: logs => {
    logs.forEach(log => {
      console.log('Transfer:', {
        from: log.args.from,
        to: log.args.to,
        value: log.args.value
      })
    })
  }
})

// Stop watching
// unwatch()
```

## Get Block Information

```typescript
// Get latest block
const block = await client.getBlock()
console.log('Latest block:', block.number)

// Get specific block
const specificBlock = await client.getBlock({
  blockNumber: 1000000n
})

// Get block with transactions
const blockWithTxs = await client.getBlock({
  blockNumber: 1000000n,
  includeTransactions: true
})
```

## Get Transaction Data

```typescript
// Get transaction by hash
const transaction = await client.getTransaction({
  hash: '0x...'
})

console.log('Transaction:', transaction)

// Get transaction receipt
const receipt = await client.getTransactionReceipt({
  hash: '0x...'
})

console.log('Receipt:', receipt)
console.log('Status:', receipt.status) // 'success' or 'reverted'
```

## Estimate Gas

Estimate gas for a contract call:

```typescript
const gasEstimate = await client.estimateContractGas({
  address: '0x...',
  abi: parseAbi(['function mint(address to, uint256 amount)']),
  functionName: 'mint',
  args: ['0x...', 1000000000000000000n],
  account: '0x...'
})

console.log('Estimated gas:', gasEstimate)
```

## Example: ERC-20 Token Info

```typescript
import { parseAbi, formatUnits } from 'viem'

const tokenAddress = '0x...'
const userAddress = '0x...'

const abi = parseAbi([
  'function name() view returns (string)',
  'function symbol() view returns (string)',
  'function decimals() view returns (uint8)',
  'function totalSupply() view returns (uint256)',
  'function balanceOf(address) view returns (uint256)',
])

// Batch read all token info
const [name, symbol, decimals, totalSupply, balance] = await Promise.all([
  client.readContract({
    address: tokenAddress,
    abi,
    functionName: 'name',
  }),
  client.readContract({
    address: tokenAddress,
    abi,
    functionName: 'symbol',
  }),
  client.readContract({
    address: tokenAddress,
    abi,
    functionName: 'decimals',
  }),
  client.readContract({
    address: tokenAddress,
    abi,
    functionName: 'totalSupply',
  }),
  client.readContract({
    address: tokenAddress,
    abi,
    functionName: 'balanceOf',
    args: [userAddress],
  }),
])

console.log({
  name,
  symbol,
  decimals,
  totalSupply: formatUnits(totalSupply, decimals),
  balance: formatUnits(balance, decimals)
})
```

## Next Steps

* [Writing to Contracts](/docs/builders/viem/writing) - Send transactions and modify state
* [Contract Addresses](/docs/builders/contract-addresses) - Find deployed contract addresses
