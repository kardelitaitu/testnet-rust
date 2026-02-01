# Writing to Contracts (/docs/builders/frontend/viem/writing)

Learn how to send transactions and write to smart contracts on RISE using Viem.

## Setup Wallet Client

Create a wallet client with your account:

```typescript
import { createWalletClient, http } from 'viem'
import { privateKeyToAccount } from 'viem/accounts'
import { riseTestnet } from './config'

const account = privateKeyToAccount('0xYOUR_PRIVATE_KEY')

const walletClient = createWalletClient({
  account,
  chain: riseTestnet,
  transport: http()
})
```

## Send ETH

Transfer ETH to another address:

```typescript
const hash = await walletClient.sendTransaction({
  to: '0x...',
  value: 1000000000000000000n, // 1 ETH in wei
})

console.log('Transaction hash:', hash)
```

## Write to Contract

Call a contract function that modifies state:

```typescript
import { parseAbi } from 'viem'

const abi = parseAbi([
  'function transfer(address to, uint256 amount) returns (bool)',
])

const hash = await walletClient.writeContract({
  address: '0x...', // Contract address
  abi,
  functionName: 'transfer',
  args: ['0x...', 1000000000000000000n], // recipient, amount
})

console.log('Transaction hash:', hash)
```

## Wait for Transaction

Wait for transaction confirmation:

```typescript
import { createPublicClient, http } from 'viem'

const publicClient = createPublicClient({
  chain: riseTestnet,
  transport: http()
})

// Send transaction
const hash = await walletClient.writeContract({
  address: '0x...',
  abi,
  functionName: 'mint',
  args: [1000n],
})

// Wait for confirmation
const receipt = await publicClient.waitForTransactionReceipt({ hash })

console.log('Transaction confirmed!')
console.log('Block number:', receipt.blockNumber)
console.log('Gas used:', receipt.gasUsed)
console.log('Status:', receipt.status) // 'success' or 'reverted'
```

## Deploy Contract

Deploy a new smart contract:

```typescript
import { parseAbi } from 'viem'

const abi = parseAbi([
  'constructor(string name, string symbol)',
])

const bytecode = '0x...' // Contract bytecode

const hash = await walletClient.deployContract({
  abi,
  bytecode,
  args: ['My Token', 'MTK'],
})

// Get deployed contract address
const receipt = await publicClient.waitForTransactionReceipt({ hash })
console.log('Contract deployed at:', receipt.contractAddress)
```

## Sign Messages

Sign a message with your private key:

```typescript
const signature = await walletClient.signMessage({
  message: 'Hello RISE!',
})

console.log('Signature:', signature)
```

## Sign Typed Data (EIP-712)

Sign structured data:

```typescript
const signature = await walletClient.signTypedData({
  domain: {
    name: 'My Dapp',
    version: '1',
    chainId: 11155931,
    verifyingContract: '0x...',
  },
  types: {
    Mail: [
      { name: 'from', type: 'address' },
      { name: 'to', type: 'address' },
      { name: 'contents', type: 'string' },
    ],
  },
  primaryType: 'Mail',
  message: {
    from: '0x...',
    to: '0x...',
    contents: 'Hello!',
  },
})

console.log('Signature:', signature)
```

## Gas Estimation & Control

### Estimate Gas

```typescript
const gas = await publicClient.estimateContractGas({
  address: '0x...',
  abi,
  functionName: 'transfer',
  args: ['0x...', 1000n],
  account,
})

console.log('Estimated gas:', gas)
```

### Set Gas Parameters

```typescript
const hash = await walletClient.writeContract({
  address: '0x...',
  abi,
  functionName: 'transfer',
  args: ['0x...', 1000n],
  gas: 100000n, // Gas limit
})
```

## Error Handling

Handle transaction errors:

```typescript
try {
  const hash = await walletClient.writeContract({
    address: '0x...',
    abi,
    functionName: 'transfer',
    args: ['0x...', 1000n],
  })

  const receipt = await publicClient.waitForTransactionReceipt({ hash })

  if (receipt.status === 'success') {
    console.log('Transaction successful!')
  } else {
    console.log('Transaction reverted')
  }
} catch (error) {
  if (error.name === 'ContractFunctionExecutionError') {
    console.error('Contract execution failed:', error.message)
  } else if (error.name === 'InsufficientFundsError') {
    console.error('Insufficient funds')
  } else {
    console.error('Transaction failed:', error)
  }
}
```

## Simulate Transaction

Test a transaction before sending:

```typescript
const { result } = await publicClient.simulateContract({
  address: '0x...',
  abi,
  functionName: 'transfer',
  args: ['0x...', 1000n],
  account,
})

console.log('Simulation result:', result)

// If simulation succeeds, send the transaction
const hash = await walletClient.writeContract({
  address: '0x...',
  abi,
  functionName: 'transfer',
  args: ['0x...', 1000n],
})
```

## Example: Complete ERC-20 Transfer

```typescript
import { createPublicClient, createWalletClient, http, parseAbi, formatUnits } from 'viem'
import { privateKeyToAccount } from 'viem/accounts'
import { riseTestnet } from './config'

// Setup
const account = privateKeyToAccount(process.env.PRIVATE_KEY as `0x${string}`)

const publicClient = createPublicClient({
  chain: riseTestnet,
  transport: http(),
})

const walletClient = createWalletClient({
  account,
  chain: riseTestnet,
  transport: http(),
})

const tokenAddress = '0x...'
const recipientAddress = '0x...'
const amount = 1000000000000000000n // 1 token (18 decimals)

const abi = parseAbi([
  'function transfer(address to, uint256 amount) returns (bool)',
  'function balanceOf(address) view returns (uint256)',
])

// Check balance before
const balanceBefore = await publicClient.readContract({
  address: tokenAddress,
  abi,
  functionName: 'balanceOf',
  args: [account.address],
})

console.log('Balance before:', formatUnits(balanceBefore, 18))

// Send transfer
const hash = await walletClient.writeContract({
  address: tokenAddress,
  abi,
  functionName: 'transfer',
  args: [recipientAddress, amount],
})

console.log('Transaction hash:', hash)

// Wait for confirmation
const receipt = await publicClient.waitForTransactionReceipt({ hash })
console.log('Transaction confirmed in block:', receipt.blockNumber)

// Check balance after
const balanceAfter = await publicClient.readContract({
  address: tokenAddress,
  abi,
  functionName: 'balanceOf',
  args: [account.address],
})

console.log('Balance after:', formatUnits(balanceAfter, 18))
```

## Next Steps

* [Contract Addresses](/docs/builders/contract-addresses) - Find deployed contract addresses on RISE
* [Testnet Tokens](/docs/builders/testnet-tokens) - Get testnet tokens for testing
