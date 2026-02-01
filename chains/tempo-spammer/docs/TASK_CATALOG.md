# Tempo-Spammer Task Catalog

Complete reference guide for all 50 tasks available in the tempo-spammer.

## Table of Contents
- [Quick Reference](#quick-reference)
- [Core Operations (01-10)](#core-operations-01-10)
- [Token Operations (21-30)](#token-operations-21-30)
- [Batch Operations (24-27, 43-44)](#batch-operations-24-27-43-44)
- [Multi-Send Operations (28-33)](#multi-send-operations-28-33)
- [Transfer Later Operations (37-39)](#transfer-later-operations-37-39)
- [Share Distribution (40-42)](#share-distribution-40-42)
- [Batch Transactions (34-36)](#batch-transactions-34-36)
- [Analytics & Monitoring (19-20)](#analytics--monitoring-19-20)
- [NFT Operations (14-16, 47-48)](#nft-operations-14-16-47-48)
- [Advanced Features (45-46, 49-50)](#advanced-features-45-46-49-50)
- [System Tasks (999)](#system-tasks-999)

---

## Quick Reference

| ID | Name | Category | Complexity | Dependencies | Gas Estimate |
|----|------|----------|------------|--------------|--------------|
| 01 | deploy_contract | Core | Low | None | 500,000 |
| 02 | claim_faucet | Core | Low | None | 100,000 |
| 03 | send_token | Core | Medium | None | 100,000 |
| 04 | create_stable | Token | Medium | None | 500,000 |
| 05 | swap_stable | Token | Medium | Task 04 | 200,000 |
| 06 | add_liquidity | Token | High | Task 04 | 300,000 |
| 07 | mint_stable | Token | Low | Task 04 | 100,000 |
| 08 | burn_stable | Token | Low | Task 04 | 100,000 |
| 09 | transfer_token | Token | Low | Task 04 | 100,000 |
| 10 | transfer_memo | Token | Low | Task 04 | 100,000 |
| 11 | limit_order | Trading | High | Task 04 | 250,000 |
| 12 | remove_liquidity | Token | Medium | Task 06 | 200,000 |
| 13 | grant_role | Admin | Low | Task 04 | 100,000 |
| 14 | nft_create_mint | NFT | Medium | None | 400,000 |
| 15 | mint_domain | NFT | Low | None | 200,000 |
| 16 | mint_random_nft | NFT | Low | Task 14 | 150,000 |
| 17 | batch_eip7702 | Advanced | High | None | 400,000 |
| 18 | tip403_policies | Advanced | Medium | None | 200,000 |
| 19 | wallet_analytics | Analytics | Low | None | 50,000 |
| 20 | wallet_activity | Analytics | Low | None | 50,000 |
| 21 | create_meme | Token | Medium | None | 500,000 |
| 22 | mint_meme | Token | Low | Task 21 | 100,000 |
| 23 | transfer_meme | Token | Low | Task 21 | 100,000 |
| 24 | batch_swap | Batch | High | Task 04 | 600,000 |
| 25 | batch_system_token | Batch | Medium | None | 400,000 |
| 26 | batch_stable_token | Batch | Medium | Task 04 | 400,000 |
| 27 | batch_meme_token | Batch | Medium | Task 21 | 400,000 |
| 28 | multi_send_disperse | Multi-Send | High | None | 500,000 |
| 29 | multi_send_disperse_stable | Multi-Send | High | Task 04 | 500,000 |
| 30 | multi_send_disperse_meme | Multi-Send | High | Task 21 | 500,000 |
| 31 | multi_send_concurrent | Multi-Send | High | None | 600,000 |
| 32 | multi_send_concurrent_stable | Multi-Send | High | Task 04 | 600,000 |
| 33 | multi_send_concurrent_meme | Multi-Send | High | Task 21 | 600,000 |
| 34 | batch_send_transaction | Batch | Medium | None | 300,000 |
| 35 | batch_send_transaction_stable | Batch | Medium | Task 04 | 300,000 |
| 36 | batch_send_transaction_meme | Batch | Medium | Task 21 | 300,000 |
| 37 | transfer_later | Scheduling | Medium | None | 200,000 |
| 38 | transfer_later_stable | Scheduling | Medium | Task 04 | 200,000 |
| 39 | transfer_later_meme | Scheduling | Medium | Task 21 | 200,000 |
| 40 | distribute_shares | Distribution | Medium | None | 300,000 |
| 41 | distribute_shares_stable | Distribution | Medium | Task 04 | 300,000 |
| 42 | distribute_shares_meme | Distribution | Medium | Task 21 | 300,000 |
| 43 | batch_mint_stable | Batch | Medium | Task 04 | 300,000 |
| 44 | batch_mint_meme | Batch | Medium | Task 21 | 300,000 |
| 45 | deploy_viral_faucet | Viral | High | None | 600,000 |
| 46 | claim_viral_faucet | Viral | Low | Task 45 | 100,000 |
| 47 | deploy_viral_nft | Viral | High | None | 600,000 |
| 48 | mint_viral_nft | Viral | Medium | Task 47 | 200,000 |
| 49 | time_bomb | Viral | High | Task 45 | 400,000 |
| 50 | deploy_storm | Viral | High | None | 800,000 |
| 999 | check_native_balance | System | Low | None | 0 |

---

## Core Operations (01-10)

### 01 - Deploy Contract
**File:** `src/tasks/t01_deploy_contract.rs`

Deploys a minimal Counter contract to the Tempo blockchain.

**Workflow:**
1. Construct Counter contract bytecode from hex string
2. Create deployment transaction (to Address::ZERO for contract creation)
3. Send transaction via provider
4. Wait for transaction receipt confirmation
5. Log contract creation to database (`created_counter_contracts` table)

**Success Criteria:**
- Transaction is mined successfully
- Contract address is extracted from receipt
- Entry added to database

**Gas Limit:** 500,000

**Database Impact:**
- Inserts into `created_counter_contracts` table
- Fields: wallet_address, contract_address, chain_id, timestamp

---

### 02 - Claim Faucet
**File:** `src/tasks/t02_claim_faucet.rs`

Claims tokens from the Tempo testnet faucet.

**Workflow:**
1. Construct faucet claim calldata (selector `0x4f9828f6` + address)
2. Send transaction to faucet contract (`0x4200000000000000000000000000000000000019`)
3. Return transaction hash

**Faucet Contract:** `0x4200000000000000000000000000000000000019`

**Gas Limit:** 100,000

**Note:** Faucet typically provides PathUSD tokens for testing.

---

### 03 - Send Token
**File:** `src/tasks/t03_send_token.rs`

Sends TIP-20 system tokens (PathUSD, AlphaUSD, BetaUSD, ThetaUSD) to random addresses.

**Workflow:**
1. Randomly select one of 4 system tokens
2. Query token balance using `balanceOf` call
3. Check if balance > 1,000,000 (minimum threshold)
4. Calculate send amount (2% of balance)
5. Generate random recipient address
6. Build transfer calldata (selector `0xa9059cbb`)
7. Send transaction and return hash

**System Tokens:**
- PathUSD: `0x20C0000000000000000000000000000000000000`
- AlphaUSD: `0x20c0000000000000000000000000000000000001`
- BetaUSD: `0x20c0000000000000000000000000000000000002`
- ThetaUSD: `0x20c0000000000000000000000000000000000003`

**Gas Limit:** 100,000

**Requirements:**
- Wallet must have at least 1,000,000 tokens of selected type

---

### 04 - Create Stablecoin
**File:** `src/tasks/t04_create_stable.rs`

Creates a new TIP-20 stablecoin token via the factory contract.

**Workflow:**
1. Generate random name and symbol for the stablecoin
2. Check PathUSD balance (requires 100 PathUSD for creation fee)
3. Generate random 32-byte salt
4. Build `createToken` calldata with factory
5. Send transaction and wait for receipt
6. Parse token address from `TokenCreated` event logs
7. Grant ISSUER_ROLE to wallet
8. Mint initial supply (1M-10M tokens)
9. Log to database (`created_assets` table)

**Factory Contract:** `0x20fc000000000000000000000000000000000000`

**Gas Limit:** 500,000

**Requirements:**
- 100 PathUSD balance for creation fee

**Database Impact:**
- Inserts into `created_assets` table
- Asset type: "stable"

---

### 05 - Swap Stablecoin
**File:** `src/tasks/t05_swap_stable.rs`

Swaps between stablecoins on the Fee AMM DEX.

**Workflow:**
1. Check PathUSD balance
2. If balance < 10,000,000, claim from faucet first
3. Approve DEX to spend PathUSD (2x swap amount for safety)
4. Wait for approval confirmation
5. Build swap calldata for `swapExactAmountIn`
6. Execute swap with 20% slippage tolerance
7. Verify swap succeeded

**DEX Contract:** `0xdec0000000000000000000000000000000000000`

**Gas Limit:** 200,000

**Dependencies:** Task 04 (Create Stablecoin) - for understanding token structure

---

### 06 - Add Liquidity
**File:** `src/tasks/t06_add_liquidity.rs`

Adds liquidity to the Fee AMM DEX for a stablecoin pair.

**Workflow:**
1. Select random stablecoin pair
2. Check balances for both tokens
3. Calculate equal-value amounts for both tokens
4. Approve DEX for both tokens
5. Add liquidity via DEX contract
6. Receive LP tokens

**Gas Limit:** 300,000

**Dependencies:** Task 04 (Create Stablecoin)

---

### 07 - Mint Stablecoin
**File:** `src/tasks/t07_mint_stable.rs`

Mints additional supply of an existing stablecoin.

**Workflow:**
1. Query database for stablecoins created by this wallet
2. Select random stablecoin
3. Check if wallet has ISSUER_ROLE
4. Generate random mint amount
5. Build `mint` calldata
6. Send transaction

**Gas Limit:** 100,000

**Dependencies:** Task 04 (Create Stablecoin)

**Requirements:**
- Wallet must be the creator of the stablecoin (has ISSUER_ROLE)

---

### 08 - Burn Stablecoin
**File:** `src/tasks/t08_burn_stable.rs`

Burns (destroys) stablecoin tokens.

**Workflow:**
1. Query database for stablecoins created by this wallet
2. Select random stablecoin
3. Check wallet balance
4. Calculate burn amount (up to 50% of balance)
5. Build `burn` calldata
6. Send transaction

**Gas Limit:** 100,000

**Dependencies:** Task 04 (Create Stablecoin)

---

### 09 - Transfer Token
**File:** `src/tasks/t09_transfer_token.rs`

Transfers stablecoins to random addresses.

**Workflow:**
1. Query database for stablecoins created by this wallet
2. Select random stablecoin
3. Check balance
4. Generate random recipient
5. Calculate transfer amount (2% of balance)
6. Execute transfer

**Gas Limit:** 100,000

**Dependencies:** Task 04 (Create Stablecoin)

---

### 10 - Transfer with Memo
**File:** `src/tasks/t10_transfer_memo.rs`

Transfers stablecoins with an attached memo message.

**Workflow:**
1. Query database for stablecoins
2. Generate random memo text
3. Execute transfer with memo in transaction data
4. Log memo to database for reference

**Gas Limit:** 100,000

**Dependencies:** Task 04 (Create Stablecoin)

---

## Token Operations (21-30)

### 21 - Create Meme Token
**File:** `src/tasks/t21_create_meme.rs`

Creates a new meme token via the TIP-20 factory.

**Workflow:**
1. Generate random meme name and symbol
   - Uses `mnemonic.txt` word list if available
   - Falls back to random generation
2. Check PathUSD balance (100 PathUSD required)
3. Generate random salt
4. Create token via factory
5. Parse token address from event logs
6. Grant ISSUER_ROLE
7. Mint initial supply (1M-10M tokens)
8. Log to database

**Gas Limit:** 500,000

**Database Impact:**
- Inserts into `created_assets` table
- Asset type: "meme"

---

### 22 - Mint Meme Token
**File:** `src/tasks/t22_mint_meme.rs`

Mints additional supply of a meme token.

**Workflow:**
1. Query database for meme tokens created by wallet
2. Select random meme token
3. Verify ISSUER_ROLE
4. Mint random amount

**Gas Limit:** 100,000

**Dependencies:** Task 21 (Create Meme)

---

### 23 - Transfer Meme Token
**File:** `src/tasks/t23_transfer_meme.rs`

Transfers meme tokens to random addresses.

**Workflow:**
1. Query database for meme tokens
2. Check balance
3. Transfer 2% of balance to random address

**Gas Limit:** 100,000

**Dependencies:** Task 21 (Create Meme)

---

## Batch Operations (24-27, 43-44)

### 24 - Batch Swap
**File:** `src/tasks/t24_batch_swap.rs`

Executes multiple swaps in a single transaction batch.

**Workflow:**
1. Prepare multiple swap operations
2. Bundle into single transaction
3. Execute atomically

**Gas Limit:** 600,000

**Benefits:**
- Reduced gas costs per operation
- Atomic execution (all succeed or all fail)

---

### 25 - Batch System Token
**File:** `src/tasks/t25_batch_system_token.rs`

Batch operations on system tokens (PathUSD, AlphaUSD, etc.).

**Operations:**
- Multiple transfers
- Balance checks
- Approvals

**Gas Limit:** 400,000

---

### 26 - Batch Stable Token
**File:** `src/tasks/t26_batch_stable_token.rs`

Batch operations on created stablecoins.

**Gas Limit:** 400,000

**Dependencies:** Task 04 (Create Stablecoin)

---

### 27 - Batch Meme Token
**File:** `src/tasks/t27_batch_meme_token.rs`

Batch operations on meme tokens.

**Gas Limit:** 400,000

**Dependencies:** Task 21 (Create Meme)

---

### 43 - Batch Mint Stable
**File:** `src/tasks/t43_batch_mint_stable.rs`

Batch minting of stablecoins.

**Gas Limit:** 300,000

**Dependencies:** Task 04 (Create Stablecoin)

---

### 44 - Batch Mint Meme
**File:** `src/tasks/t44_batch_mint_meme.rs`

Batch minting of meme tokens.

**Gas Limit:** 300,000

**Dependencies:** Task 21 (Create Meme)

---

## Multi-Send Operations (28-33)

### 28 - Multi-Send Disperse
**File:** `src/tasks/t28_multi_send_disperse.rs`

Disperse ETH to multiple recipients in one transaction.

**Workflow:**
1. Generate multiple random recipients
2. Calculate distribution amounts
3. Use disperse contract for single-transaction multi-send

**Gas Limit:** 500,000

---

### 29 - Multi-Send Disperse Stable
**File:** `src/tasks/t29_multi_send_disperse_stable.rs`

Disperse stablecoins to multiple recipients.

**Gas Limit:** 500,000

**Dependencies:** Task 04 (Create Stablecoin)

---

### 30 - Multi-Send Disperse Meme
**File:** `src/tasks/t30_multi_send_disperse_meme.rs`

Disperse meme tokens to multiple recipients.

**Gas Limit:** 500,000

**Dependencies:** Task 21 (Create Meme)

---

### 31 - Multi-Send Concurrent
**File:** `src/tasks/t31_multi_send_concurrent.rs`

Send multiple ETH transfers concurrently.

**Workflow:**
1. Spawn multiple transfer tasks
2. Execute in parallel
3. Aggregate results

**Gas Limit:** 600,000

---

### 32 - Multi-Send Concurrent Stable
**File:** `src/tasks/t32_multi_send_concurrent_stable.rs`

Concurrent stablecoin transfers.

**Gas Limit:** 600,000

**Dependencies:** Task 04 (Create Stablecoin)

---

### 33 - Multi-Send Concurrent Meme
**File:** `src/tasks/t33_multi_send_concurrent_meme.rs`

Concurrent meme token transfers.

**Gas Limit:** 600,000

**Dependencies:** Task 21 (Create Meme)

---

## Transfer Later Operations (37-39)

### 37 - Transfer Later
**File:** `src/tasks/t37_transfer_later.rs`

Schedules ETH transfer for future execution.

**Workflow:**
1. Create time-locked transfer request
2. Store in database
3. Execute when time condition met

**Gas Limit:** 200,000

---

### 38 - Transfer Later Stable
**File:** `src/tasks/t38_transfer_later_stable.rs`

Schedules stablecoin transfer for future.

**Gas Limit:** 200,000

**Dependencies:** Task 04 (Create Stablecoin)

---

### 39 - Transfer Later Meme
**File:** `src/tasks/t39_transfer_later_meme.rs`

Schedules meme token transfer for future.

**Gas Limit:** 200,000

**Dependencies:** Task 21 (Create Meme)

---

## Share Distribution (40-42)

### 40 - Distribute Shares
**File:** `src/tasks/t40_distribute_shares.rs`

Distributes ETH shares among multiple parties.

**Workflow:**
1. Calculate share percentages
2. Distribute to multiple addresses
3. Log distribution details

**Gas Limit:** 300,000

---

### 41 - Distribute Shares Stable
**File:** `src/tasks/t41_distribute_shares_stable.rs`

Distributes stablecoin shares.

**Gas Limit:** 300,000

**Dependencies:** Task 04 (Create Stablecoin)

---

### 42 - Distribute Shares Meme
**File:** `src/tasks/t42_distribute_shares_meme.rs`

Distributes meme token shares.

**Gas Limit:** 300,000

**Dependencies:** Task 21 (Create Meme)

---

## Batch Transactions (34-36)

### 34 - Batch Send Transaction
**File:** `src/tasks/t34_batch_send_transaction.rs`

Bundles multiple ETH transfers into batch.

**Gas Limit:** 300,000

---

### 35 - Batch Send Transaction Stable
**File:** `src/tasks/t35_batch_send_transaction_stable.rs`

Bundles multiple stablecoin transfers.

**Gas Limit:** 300,000

**Dependencies:** Task 04 (Create Stablecoin)

---

### 36 - Batch Send Transaction Meme
**File:** `src/tasks/t36_batch_send_transaction_meme.rs`

Bundles multiple meme token transfers.

**Gas Limit:** 300,000

**Dependencies:** Task 21 (Create Meme)

---

## Analytics & Monitoring (19-20)

### 19 - Wallet Analytics
**File:** `src/tasks/t19_wallet_analytics.rs`

Collects and reports wallet analytics.

**Metrics:**
- Transaction count
- Success/failure rates
- Average gas usage
- Token balances

**Gas Limit:** 50,000 (read-only operations)

---

### 20 - Wallet Activity
**File:** `src/tasks/t20_wallet_activity.rs`

Monitors and logs wallet activity.

**Tracks:**
- Recent transactions
- Balance changes
- Token transfers

**Gas Limit:** 50,000

---

## NFT Operations (14-16, 47-48)

### 14 - NFT Create & Mint
**File:** `src/tasks/t14_nft_create_mint.rs`

Creates NFT collection and mints initial token.

**Workflow:**
1. Deploy NFT contract
2. Configure metadata
3. Mint first token
4. Log to database

**Gas Limit:** 400,000

---

### 15 - Mint Domain
**File:** `src/tasks/t15_mint_domain.rs`

Mints a domain name NFT.

**Workflow:**
1. Generate random domain name
2. Check availability
3. Mint domain NFT
4. Register in domain system

**Gas Limit:** 200,000

---

### 16 - Mint Random NFT
**File:** `src/tasks/t16_mint_random_nft.rs`

Mints random NFT from existing collection.

**Workflow:**
1. Query available collections
2. Select random collection
3. Generate random metadata
4. Mint NFT

**Gas Limit:** 150,000

**Dependencies:** Task 14 (NFT Create & Mint)

---

### 47 - Deploy Viral NFT
**File:** `src/tasks/t47_deploy_viral_nft.rs`

Deploys viral NFT contract with special mechanics.

**Features:**
- Viral spreading mechanics
- Referral rewards
- Limited supply mechanics

**Gas Limit:** 600,000

---

### 48 - Mint Viral NFT
**File:** `src/tasks/t48_mint_viral_nft.rs`

Mints from viral NFT collection.

**Gas Limit:** 200,000

**Dependencies:** Task 47 (Deploy Viral NFT)

---

## Advanced Features (45-46, 49-50)

### 45 - Deploy Viral Faucet
**File:** `src/tasks/t45_deploy_viral_faucet.rs`

Deploys viral faucet contract.

**Features:**
- Referral-based distribution
- Viral growth mechanics
- Reward multipliers

**Gas Limit:** 600,000

---

### 46 - Claim Viral Faucet
**File:** `src/tasks/t46_claim_viral_faucet.rs`

Claims from viral faucet.

**Workflow:**
1. Check eligibility
2. Calculate rewards based on referrals
3. Claim tokens

**Gas Limit:** 100,000

**Dependencies:** Task 45 (Deploy Viral Faucet)

---

### 49 - Time Bomb
**File:** `src/tasks/t49_time_bomb.rs`

Creates time-locked transaction bomb.

**Mechanics:**
- Transaction executes at specific time
- Can be defused before execution
- Rewards for successful execution

**Gas Limit:** 400,000

**Dependencies:** Task 45 (Deploy Viral Faucet)

---

### 50 - Deploy Storm
**File:** `src/tasks/t50_deploy_storm.rs`

Deploys storm contract for high-volume testing.

**Features:**
- Massive transaction generation
- Stress testing capabilities
- Performance monitoring

**Gas Limit:** 800,000

---

## System Tasks (999)

### 999 - Check Native Balance
**File:** `src/tasks/check_native_balance.rs`

Utility task to check native TEM balance.

**Purpose:**
- Debugging
- Balance verification
- No transaction sent (read-only)

**Gas Limit:** 0 (no transaction)

---

## Task Dependencies Graph

```
Core Tasks (01-03)
    ↓
Token Creation (04, 21)
    ↓
Token Operations (05-10, 22-23)
    ↓
Batch & Multi-Send (24-27, 28-33, 34-36)
    ↓
Advanced Features (45-50)
```

## Database Schema

### created_counter_contracts
```sql
CREATE TABLE created_counter_contracts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_address TEXT NOT NULL,
    contract_address TEXT NOT NULL,
    chain_id INTEGER,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### created_assets
```sql
CREATE TABLE created_assets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_address TEXT NOT NULL,
    asset_address TEXT NOT NULL,
    asset_type TEXT NOT NULL,   -- "token", "stable", "meme", "nft"
    name TEXT,
    symbol TEXT,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### task_metrics
```sql
CREATE TABLE task_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    worker_id TEXT NOT NULL,
    wallet_address TEXT NOT NULL,
    task_name TEXT NOT NULL,
    status TEXT NOT NULL,       -- "success" or "failed"
    message TEXT,
    duration_ms INTEGER,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

## Important Contract Addresses

### System Tokens
- **PathUSD:** `0x20C0000000000000000000000000000000000000`
- **AlphaUSD:** `0x20c0000000000000000000000000000000000001`
- **BetaUSD:** `0x20c0000000000000000000000000000000000002`
- **ThetaUSD:** `0x20c0000000000000000000000000000000000003`

### Core Contracts
- **TIP-20 Factory:** `0x20fc000000000000000000000000000000000000`
- **Fee AMM DEX:** `0xdec0000000000000000000000000000000000000`
- **Faucet:** `0x4200000000000000000000000000000000000019`

## Gas Estimation Guide

| Operation Type | Typical Gas | Notes |
|----------------|-------------|-------|
| Simple Transfer | 21,000 | Standard ETH transfer |
| Token Transfer | 65,000 | ERC-20 transfer |
| Contract Deployment | 500,000-800,000 | Varies by contract size |
| DEX Swap | 150,000-250,000 | Includes approval |
| Batch Operations | 300,000-600,000 | Amortized per operation |
| NFT Operations | 150,000-400,000 | Mint vs deploy |

## Weighted Task Distribution

The spammer uses weighted random selection to favor high-volume tasks:

```rust
let weighted_tasks = vec![
    (1, 5),   // 01_deploy_contract
    (2, 10),  // 02_claim_faucet
    (3, 20),  // 03_send_token (higher weight)
    // ... etc
];
```

Higher weights = More frequent execution

## Task Timeout

Default task timeout: **180 seconds**

Configurable in `config.toml`:
```toml
task_timeout = 60  # seconds
```

## See Also

- [Configuration Reference](./CONFIG_REFERENCE.md)
- [Task Development Guide](./TASK_DEVELOPMENT.md)
- [Troubleshooting Guide](./TROUBLESHOOTING.md)
- [Architecture Overview](./ARCHITECTURE.md)
