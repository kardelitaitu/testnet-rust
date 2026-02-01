# Task 01: Deploy Contract

Deploys a minimal Counter contract to the Tempo blockchain.

## Overview

This task deploys a simple counter contract that has increment and decrement functionality. It uses raw bytecode deployment and logs the contract creation to the database.

## Contract Details

- **Bytecode**: Minimal counter contract with `increment()` and `decrement()` functions
- **Decimals**: 18 (standard for counter-like contracts)
- **Factory**: Direct bytecode deployment (not via factory)

## Usage

```bash
# Run the task
$env:WALLET_PASSWORD="password"; cargo run --bin tempo-debug -- --config chains/tempo-spammer/config/config.toml --task 1 --wallet 0
```

## What It Does

1. Constructs the contract bytecode from hex
2. Creates a deployment transaction (to Address::ZERO)
3. Sends the transaction and waits for confirmation
4. Logs the contract creation to `tempo-spammer.db`

## Output Example

```
Creating contract...
Contract deployed: 0x1234...
```

## Database Logging

Logs to `created_counter_contracts` table:
- `creator_address`: Wallet that deployed the contract
- `contract_address`: Deployed contract address
- `chain_id`: Network chain ID

## Files

- Source: `src/tasks/t01_deploy_contract.rs`
