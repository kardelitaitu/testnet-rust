# Bridge Back — Base Sepolia → Ethereum Sepolia

## Goal
Bridge T+ (USDT+) and C+ (USDC+) from Base Sepolia back to Ethereum Sepolia.

## Approach
**Option A** — shared crate (`sepolia-overlayer`), separate Base Sepolia config.
Does not break existing tasks — they continue using the existing `config.toml`.

## Files Created

### 1. `chains/sepolia-overlayer/config-base.toml`
- RPC: `https://base-sepolia-rpc.publicnode.com`
- chain_id: Base Sepolia (84532)
- wallet_dir: `chains/sepolia-overlayer/wallets-json-sepolia-overlayer` (same wallets)

### 2. `chains/sepolia-overlayer/src/task/t16_bridge_back_tplus.rs`
- Bridges USDT+ (T+) from Base Sepolia → Eth Sepolia via LayerZero OFT `send()`
- **Contract**: `0xdE287B4a0918102511b027d53688c169fb308762` (T+ on Base Sepolia)
- **dstEid**: `40161` (Ethereum Sepolia, from working tx `0x9ce1`)
- **nativeFee**: `0x5e505169ab6c` = ~0.0001037 ETH (from working tx)
- Amount: 5% of T+ balance, rounded to nearest whole T+
- extraOptions: `0x0003`

### 3. `chains/sepolia-overlayer/src/task/tXX_bridge_back_cplus.rs` (TODO)
- Same as t16 but for USDC+ (C+) on Base Sepolia
- Need C+ contract address on Base Sepolia

## How to Run
```powershell
# Bridge T+ back from Base Sepolia
cargo run -p sepolia-overlayer --bin sepolia-debug_task ^
  --config chains/sepolia-overlayer/config-base.toml --task 16 --wallet 0

# Existing tasks on Eth Sepolia still work:
cargo run -p sepolia-overlayer --bin sepolia-debug_task ^
  --config chains/sepolia-overlayer/config.toml --task 2 --wallet 0
```

## Items Still Needed
| Item | Details |
|------|---------|
| USDC+ (C+) address on Base Sepolia | Need tx hash or explorer lookup |
```
