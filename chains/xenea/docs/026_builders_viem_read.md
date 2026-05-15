# Viem (/docs/builders/frontend/viem)

Viem is a TypeScript interface for Ethereum that provides low-level stateless primitives for interacting with Ethereum. It's lightweight, modular, and type-safe.

## Why Viem?

* **Type-safe**: First-class TypeScript support with inferred types
* **Lightweight**: Tree-shakeable with minimal dependencies
* **Fast**: Optimized for performance
* **Modular**: Only import what you need
* **Modern**: Built with latest JavaScript features

## Quick Start

```bash
npm install viem
```

## Basic Setup

```typescript
import { createPublicClient, http } from 'viem'
import { riseTestnet } from 'viem/chains'

// Create a public client
const client = createPublicClient({
  chain: riseTestnet,
  transport: http()
})

// Get the latest block number
const blockNumber = await client.getBlockNumber()
console.log('Current block:', blockNumber)
```

## Next Steps

<Cards>
  <Card icon={<Rocket />} title="Get Started" href="/docs/builders/frontend/viem/get-started" description="Set up your first Viem project" />

  <Card icon={<BookOpen />} title="Read Contracts" href="/docs/builders/frontend/viem/reading" description="Query blockchain data and contract state" />

  <Card icon={<Zap />} title="Write Contracts" href="/docs/builders/frontend/viem/writing" description="Send transactions and interact with contracts" />
</Cards>

## Resources

* [Official Viem Documentation](https://viem.sh)
* [Viem GitHub](https://github.com/wevm/viem)
* [TypeScript Support](https://viem.sh/docs/typescript)
