# Rust Multi-Chain Testnet Framework - Codebase Reference

Short reference based on current repository state.

## 1) Workspace Summary

Root `Cargo.toml` workspace members:
- `core-logic`
- `chains/risechain`
- `chains/xenea`
- `chains/da-chain`
- `chains/sepolia-overlayer`
- `chains/tempo-spammer`
- `chains/robinhood`

Template folders (not workspace members by default):
- `chains/_template_evm`
- `chains/_template_solana`

## 2) Crate Roles

### `core-logic`
Shared runtime/components used by chain crates.

Key modules:
- `core-logic/src/config/mod.rs` - `SpamConfig`, `WalletSource`, `ProxyConfig`, `ChainConfig`
- `core-logic/src/database.rs` - SQLite manager and metrics logging
- `core-logic/src/metrics.rs` - runtime metrics collector/export
- `core-logic/src/security/mod.rs` - wallet decryption helpers
- `core-logic/src/traits/mod.rs` - `Spammer`, `Task`, `TaskResult`, `SpammerStats`
- `core-logic/src/utils/explorer_gas_tracker.rs` - payload-driven explorer gas fetcher/parser
- `core-logic/src/utils/wallet_manager.rs` - wallet discovery/decryption/cache
- `core-logic/src/utils/proxy_manager.rs` - proxy loading
- `core-logic/src/utils/runner.rs` - concurrent worker runner + graceful shutdown

### `chains/risechain` (`rise-project`)
Main EVM-style spammer implementation with many task modules.

Entrypoints:
- `chains/risechain/src/main.rs` (`rise-project`)
- `chains/risechain/src/bin/debug_task.rs`
- extra utility/test bins in `chains/risechain/src/bin/`

Task wiring:
- Task modules: `chains/risechain/src/task/*.rs`
- Registry/context: `chains/risechain/src/task/mod.rs`
- Production task list + weighted selection: `chains/risechain/src/spammer/mod.rs`

### `chains/xenea` (`xenea-project`)
EVM-style spammer clone of `risechain` for Xenea.

Entrypoints:
- `chains/xenea/src/main.rs` (`xenea-project`)
- `chains/xenea/src/bin/debug_task.rs`

Task wiring:
- Task modules: `chains/xenea/src/task/*.rs`
- Registry/context: `chains/xenea/src/task/mod.rs`
- Production task list + weighted selection: `chains/xenea/src/spammer/mod.rs`
- Meme flow: `t07` deploys the mintable MEME contract, `t61` mints a random amount from a DB-selected MEME contract.

### `chains/da-chain` (`da-chain-project`)
EVM-style spammer with a shared explorer gas tracker flow.

Entrypoints:
- `chains/da-chain/src/main.rs` (`da-chain-project`)
- `chains/da-chain/src/bin/debug_task.rs`

Runtime helpers:
- `chains/da-chain/src/spammer/mod.rs`
- `chains/da-chain/src/utils/gas.rs`
- Uses `ExplorerGasTracker` from `core-logic` with a payload for the gas tracker page.

### `chains/sepolia-overlayer` (`sepolia-overlayer`)
Sepolia-specific overlayer implementation with main, daily, debug, funding, and balance dump binaries.

Entrypoints:
- `chains/sepolia-overlayer/src/main.rs` (`sepolia-overlayer`) — main spammer
- `chains/sepolia-overlayer/src/bin/debug_task.rs` (`sepolia-debug_task`) — interactive task debugger
- `chains/sepolia-overlayer/src/bin/daily.rs` (`sepolia-daily`) — scheduled daily task execution loop
- `chains/sepolia-overlayer/src/bin/fund.rs` (`sepolia-funder`) — multi-hop obfuscated ETH funding
- `chains/sepolia-overlayer/src/bin/wallet-balance-dump.rs` (`wallet-balance-dump`) — parallel wallet balance scanner
- `chains/sepolia-overlayer/src/bin/check_rpcs.rs` (`check-rpcs`) — RPC health/throughput auditing utility

Task area:
- `chains/sepolia-overlayer/src/task/` — 22 task modules (t01–t22)

Daily runner:
- `chains/sepolia-overlayer/src/daily_runner/mod.rs` — core execution loop, task dispatch, pause window logic, busy-wallet locking, per-task limits (hot-reloadable), proxy health management, task timeout overrides, and `confirm_with_retry` helper (exponential backoff with 5 retries)

Config default:
- `chains/sepolia-overlayer/config.toml` — chain RPC endpoints, task limits, gas bounds, timeouts
- `chains/sepolia-overlayer/config-base.toml` — Base Sepolia chain config (chain ID 84532)

Funder documentation:
- `chains/sepolia-overlayer/sepolia-funder.md` — full usage guide, architecture, test coverage, design decisions

### `chains/tempo-spammer` (`tempo-spammer`)
Tempo chain-specific implementation (edition 2024, alloy-based stack).

Entrypoints in `chains/tempo-spammer/bin/`:
- `tempo-spammer.rs`
- `tempo-debug.rs`
- `tempo-runner.rs`
- `tempo-sequence.rs`
- `debug_proxy.rs`
- `wallet-check.rs`

Task area:
- `chains/tempo-spammer/src/tasks/`

Config default:
- `chains/tempo-spammer/config/config.toml`

### `chains/robinhood` (`robinhood-spammer`)
Lean spammer variant with separate runner/debug binaries.

Entrypoints in `chains/robinhood/bin/`:
- `robinhood-spammer.rs`
- `robinhood-debug.rs`
- `robinhood-runner.rs`

## 3) Runtime Data/Artifacts

- Wallet source dir: `wallet-json/`
- Wallet fallback file: `pv.txt` (if present)
- Proxies file: `proxies.txt`
- Main DB examples: `rise.db`, `tempo-spammer.db`
- Xenea DB example: `xenea.db`
- Logs dir: `logs/`

## 4) Core Flows

### Wallet loading flow
1. `WalletManager::new()` scans `wallet-json/` (with fallback path handling).
2. Falls back to `pv.txt` if needed.
3. `get_wallet(...)` decrypts lazily and caches.
4. Optional chain-targeted extraction is supported in wallet manager.

Alternative approach:
- Preload/decrypt on startup for faster runtime.
- Keep lazy mode for lower startup latency and RAM use.

### Worker execution flow
1. Main crate loads config, wallet manager, proxies, DB.
2. Builds per-wallet spammer objects.
3. `WorkerRunner::run_spammers(...)` spawns workers via `JoinSet`.
4. Ctrl+C triggers `CancellationToken` for graceful stop.

Alternative approach:
- Full parallel workers for throughput.
- Lower worker count/semaphore for stability under weak RPC/proxy conditions.

### RISE task addition flow
1. Add `tXX_name.rs` under `chains/risechain/src/task/`.
2. Register in `task/mod.rs`.
3. Add into task vector in `spammer/mod.rs`.
4. Add into `debug_task.rs` for direct testing.

Alternative rollout:
- Add to `debug_task.rs` first for validation.
- Move into spammer list after successful tests.

## 5) Known Documentation Drift Fixed

Old docs referenced `evm-project`/`solana-project` as active members.
Current active members are `risechain`, `tempo-spammer`, and `robinhood`.

## 6) Security Notes

- Avoid committing secrets (`.env`, wallet material, proxy creds, DB/log dumps).
- Do not log private keys or passwords.
- Keep `zeroize` usage on sensitive structs.

Current risk to address:
- `chains/tempo-spammer/Cargo.toml` includes Telegram bot metadata token/chat id.

Alternatives:
- Move to env vars (`.env`) + rotate token (recommended).
- If rotation blocked, move to local untracked file and add CI secret scan.
