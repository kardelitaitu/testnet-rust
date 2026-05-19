# Bridge Back — Base Sepolia → Ethereum Sepolia

## Goal
Bridge T+ (USDT+) and C+ (USDC+) from Base Sepolia back to Ethereum Sepolia.

## Approach
**Option A** — shared crate (`sepolia-overlayer`), separate Base Sepolia config.
Does not break existing tasks — they continue using the existing `config.toml`.

## Files to Create

### 1. `chains/sepolia-overlayer/config-base.toml` (new)
- RPC: Base Sepolia endpoint (e.g. `https://base-sepolia-rpc.publicnode.com`)
- chain_id: Base Sepolia (84532)
- wallet_dir: `chains/sepolia-overlayer/wallets-json-sepolia` (same wallets, need Base Sepolia ETH for gas)

### 2. `chains/sepolia-overlayer/src/task/t14_bridge_back_tplus.rs` (new)
- Bridge USDT+ (T+) from Base Sepolia → Eth Sepolia via LayerZero OFT `send()`
- Contract: USDT+ address on **Base Sepolia** (need address)
- Function: `send((uint32,bytes32,uint256,uint256,bytes,bytes,bytes),(uint256,uint256),address)`
- Selector: `0xc7c7f5b3` (verified)
- dstEid: **Ethereum Sepolia** (need value — decode from a working bridge-back tx)
- Amount: 5% of T+ balance, rounded to nearest whole T+
- extraOptions: `0x0003`
- nativeFee: hardcoded 0.0002 ETH (msg.value must cover bridge fee)
- `to`: user's address as bytes32

### 3. `chains/sepolia-overlayer/src/task/t15_bridge_back_cplus.rs` (new)
- Same as t14 but for USDC+ (C+) on Base Sepolia
- Contract: USDC+ address on **Base Sepolia** (need address)

## Registration
- `task/mod.rs` — add modules + re-exports
- `spammer/mod.rs` — add to task list
- `debug_task.rs` — add to task list

## How to Run
```powershell
# Bridge T+ back from Base Sepolia
cargo run -p sepolia-overlayer --bin sepolia-debug_task ^
  --config chains/sepolia-overlayer/config-base.toml --task 14 --wallet 0

# Bridge C+ back from Base Sepolia
cargo run -p sepolia-overlayer --bin sepolia-debug_task ^
  --config chains/sepolia-overlayer/config-base.toml --task 15 --wallet 0

# Existing tasks on Eth Sepolia still work:
cargo run -p sepolia-overlayer --bin sepolia-debug_task ^
  --config chains/sepolia-overlayer/config.toml --task 2 --wallet 0
```

## Items Needed to Build
| Item | Details |
|------|---------|
| Base Sepolia RPC URL | e.g. `https://base-sepolia-rpc.publicnode.com` |
| USDT+ address on Base Sepolia | |
| USDC+ address on Base Sepolia | |
| dstEid for Ethereum Sepolia | — decode from a working bridge-back tx, or look up LayerZero endpoint IDs |
| Wallets | same keys on both chains? separate wallet folder? |
| Base Sepolia ETH | wallets need ETH on Base Sepolia for gas + bridge fee |
