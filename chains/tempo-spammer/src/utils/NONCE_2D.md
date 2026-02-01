# Tempo 2D Nonce System

## Overview

Tempo extends the standard EVM nonce system with a **2D nonce structure**, enabling true parallel transaction execution without waiting for confirmations.

## How It Works

### Standard EVM (nonceKey: 0)
```
Transaction 1: nonce=0 -> must wait for confirmation
Transaction 2: nonce=1 -> must wait for #1
Transaction 3: nonce=2 -> must wait for #2
```

### Tempo 2D Nonces (nonceKey: 0, 1, 2, ...)
```
Key 0: T1(nonce=0), T2(nonce=1), T3(nonce=2)  <- Sequential
Key 1: T4(nonce=0)  --\
Key 2: T5(nonce=0)  --+-- Parallel (no waiting!)
Key 3: T6(nonce=0)  --/
```

## Key Addresses

| Contract | Address | Purpose |
|----------|---------|---------|
| Nonce Precompile | `0x4E4F4E4345000000000000000000000000000000` | Query/manage nonces |
| Account Keychain | `0x4B41434F554E5400000000000000000000000000` | Authorize nonce keys |

## Usage

### 1. Authorize a Nonce Key (One-Time Setup)

```rust
use crate::utils::nonce_2d::TempoNonceManager2D;

let manager = TempoNonceManager2D::new(provider.clone());

// Authorize nonce key 1 for parallel execution
manager.authorize_nonce_key(&signer, 1).await?;
```

### 2. Send Transactions in Parallel

```rust
use crate::utils::nonce_2d::ParallelSender;

let sender = ParallelSender::new(provider);

// Send 3 transactions in parallel using keys 1, 2, 3
let hashes = sender
    .send_parallel(
        &signer,
        chain_id,
        contract_addr,
        vec![calldata1, calldata2, calldata3],
        1, // start key
    )
    .await?;
```

### 3. Manual Parallel Sending

```rust
let manager = TempoNonceManager2D::new(provider.clone());

// Get next nonce for each key
let nonce1 = manager.get_next_nonce(address, 1).await?;
let nonce2 = manager.get_next_nonce(address, 2).await?;
let nonce3 = manager.get_next_nonce(address, 3).await?;

// Build transactions
let tx1 = manager.build_tx_with_nonce(to, data1, nonce1, chain_id, &signer);
let tx2 = manager.build_tx_with_nonce(to, data2, nonce2, chain_id, &signer);
let tx3 = manager.build_tx_with_nonce(to, data3, nonce3, chain_id, &signer);

// Send all in parallel
let (r1, r2, r3) = tokio::join!(
    provider.send_transaction(tx1),
    provider.send_transaction(tx2),
    provider.send_transaction(tx3)
);
```

## API Reference

### TempoNonceManager2D

| Method | Description |
|--------|-------------|
| `new(provider)` | Create a new nonce manager |
| `get_protocol_nonce(addr)` | Get nonce at key 0 |
| `get_user_nonce(addr, key)` | Get nonce at specific key |
| `get_nonce_key_count(addr)` | Get count of authorized keys |
| `get_next_nonce(addr, key)` | Get and cache next nonce |
| `get_next_protocol_nonce(addr)` | Get and cache next protocol nonce |
| `authorize_nonce_key(signer, key)` | Authorize a new nonce key |
| `build_tx(to, data, key, chain_id, signer)` | Build tx with nonce key |
| `build_tx_with_nonce(to, data, nonce, chain_id, signer)` | Build tx with specific nonce |
| `build_balance_of_calldata(owner)` | Build ERC20 balanceOf calldata |
| `build_approve_calldata(spender, amount)` | Build ERC20 approve calldata |
| `build_transfer_calldata(recipient, amount)` | Build ERC20 transfer calldata |

### ParallelSender

| Method | Description |
|--------|-------------|
| `new(provider)` | Create a new parallel sender |
| `authorize_keys(signer, start, count)` | Authorize multiple keys |
| `send_parallel(signer, chain_id, to, calldatas, start_key)` | Send txs in parallel |

## Calldata Examples

### Get Nonce
```
Selector: 0x27fcbacf
Input: owner (32 bytes) + key (32 bytes)
```

### Get Nonce Key Count
```
Selector: 0x6a166588
Input: owner (32 bytes)
```

### Authorize Nonce Key
```
Selector: 0xc8844e62
Input: key (32 bytes)
```

## Best Practices

1. **Key Authorization**: Authorize keys in batches to save gas
2. **Cache Management**: The manager caches nonces locally; clear cache if needed
3. **Error Handling**: Parallel sends may partially fail - handle individual results
4. **Gas Pricing**: Each transaction is independent - set gas per transaction

## Gas Comparison

| Method | Gas Usage | Speed |
|--------|-----------|-------|
| Sequential | Low (shared base fee) | Slow (waits) |
| Parallel 2D Nonces | Higher (multiple base fees) | ⚡ Fast (no waits) |

## Example Use Cases

1. **High-frequency trading**: Execute multiple trades simultaneously
2. **Mass airdrops**: Send to many recipients in parallel
3. **Atomic operations**: Group related operations across keys
4. **Load testing**: Flood the network with parallel transactions
