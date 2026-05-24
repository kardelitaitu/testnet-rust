# sepolia-funder

Multi-hop obfuscated ETH funding binary for **sepolia-overlayer**.

*Last audited: 21-05-26 by docs-auditor*

*Re-audited: 21-05-26 by Buffy*

## Concept

Routes ETH from high-balance wallets to low-balance wallets through **N ephemeral proxy wallets** (5–7 hops default), randomizing every axis independently per hop to avoid Sybil cluster detection.

```
Sender → P1 → P2 → ... → PN → Target
```

Each proxy `P` is a fresh `LocalWallet` discarded after its single forwarding tx — no on-chain history, no trace on disk.

## Usage

```powershell
# Dry run (no txs sent)
$env:WALLET_PASSWORD="password"
cargo run -p sepolia-overlayer --bin sepolia-funder -- `
    --config chains/sepolia-overlayer/config.toml `
    --min-balance 0.05 --max-balance 0.01 `
    --min-target 0.02 --max-target 0.04 `
    --max-hops 3 --dry-run

# Real execution with 4-hour spread window
$env:WALLET_PASSWORD="password"
cargo run -p sepolia-overlayer --bin sepolia-funder -- `
    --config chains/sepolia-overlayer/config.toml `
    --min-balance 0.05 --max-balance 0.01 `
    --min-target 0.02 --max-target 0.04 `
    --spread-hours 4 --max-hops 3
```

## All flags

| Flag | Default | Description |
|------|---------|-------------|
| `--config` | `chains/sepolia-overlayer/config.toml` | Config path |
| `--min-balance` | `0.500` | Minimum ETH balance to be a sender |
| `--max-balance` | `0.010` | Maximum ETH balance to be a target |
| `--min-target` | `0.020` | Min ETH to send per target (randomized) |
| `--max-target` | `0.040` | Max ETH to send per target (randomized) |
| `--min-hops` | `5` | Minimum proxy hops per target |
| `--max-hops` | `7` | Maximum proxy hops per target |
| `--min-delay-secs` | `15` | Min delay between hops (seconds) |
| `--max-delay-secs` | `30` | Max delay between hops (seconds) |
| `--min-gwei` | `1.2` | Floor gas price (gwei) |
| `--max-gwei` | `1.5` | Ceiling gas price (gwei) |
| `--workers` | `1` | Number of concurrent funding workers |
| `--min-worker-interval-secs` | `30` | Minimum pause between completed worker cycles (seconds) |
| `--max-worker-interval-secs` | `30` | Maximum pause between completed worker cycles (seconds) |
| `--spread-hours` | *none* | Spread funding across N hours |
| `--max-targets` | *all* | Cap on how many targets to fund |
| `--load-concurrency` | `100` | Concurrent wallet decryptions during startup |
| `--dry-run` | `false` | Print plan, send no txs |
| `--yes` | `false` | Skip confirmation prompt |
| `--json-log` | *none* | Write structured JSON log (duration stats, summary) |

## Randomization axes

Every dimension varies independently:

- **Amount**: randomized per target between `[min_target, max_target]`
- **Gas price**: randomized per hop between `[min_gwei, max_gwei]` (default `1.2-1.5` gwei), floored at 90% of network gas price, hard-capped at 100,000 mgwei (100 gwei)
- **Hop count**: uniform random between `min_hops` and `max_hops` per target
- **Sender rotation**: weighted round-robin across senders (ceil division of targets / senders), with concurrent locking to prevent double-allocation
- **Inter-hop delay**: random seconds between each proxy hop (`min_delay_secs`–`max_delay_secs`)
- **Worker distribution**: targets are split round-robin across workers so every worker keeps a steady queue
- **Worker rest**: random pause between completed funding cycles (`min_worker_interval_secs`–`max_worker_interval_secs`), only when more targets remain

## Spread timing (`--spread-hours`)

The total window (in hours) is divided into `available` slots, one per target. Each slot gets a **random offset within `[0, total_ms / available]`**. These offsets are pre-computed **before** execution and applied **before** sender selection. This ensures:

- Worker slots are only occupied during actual work (hop transactions + delays), not during the spread wait
- Worker count doesn't limit the spread — all staggered launches proceed independently
- Previously, the spread was applied *inside* the worker task after work started, causing early workers to sleep before they could process the next target

## Architecture

### Key structs

| Struct | Purpose |
|--------|---------|
| `Funder` | Orchestrator — loads wallets, classifies senders/targets, dispatches funding |
| `WalletInfo` | A single wallet's index, address, and ETH balance |
| `SenderState` | Runtime bookkeeping: use counts, locked senders, funded/failed counters |
| `PlannedFund` | Dry-run description: which sender → which target → how many hops → how much |

### Testing philosophy

The binary is strongly TDD'd with **58 unit tests** covering every pure logic path:

| Layer | Tests | What's tested |
|-------|-------|---------------|
| **Filtering** | 9 | `filter_senders`, `filter_targets`, `compute_max_per_sender`, `select_targets_to_fund` |
| **`pick_sender`** | 4 | Valid index, respects use counts, fallback when all at limit, prefers under-limit even with random seed |
| **SenderState** | 7 | Pick/lock, max-per-sender, unlock, empty, multi-pick increments |
| **Dry-run planner** | 6 | Allocation, caps, empty cases, range bounds |
| **Gas price** | 7 | Bounds, 90% floor, 100k cap, determinism, edge cases |
| **Math** | 4 | Seed amount, forward amount, saturation, scaling with hops |
| **Hop address/label** | 4 | Next hop, last hop goes to target, label formatting |
| **Confirmation** | 5 | `y`/`Y`/`n`/empty/invalid |
| **`prepare_funding_sets`** | 1 | Integration through `Funder` |
| **`execute_dry_run`** | 1 | Does not panic |
| **`should_skip_confirmation`** | 3 | `yes`, `dry_run`, neither |
| **CLI integration** | 4 | `assert_cmd` end-to-end run (via `tests/fund_cli.rs`) |

### Code cleanup history

- **`TxExecutor`/`TxParams`/`RealExecutor` removed** — mock-injection trait that was never wired into `orchestrate`; pure function TDD replaced the need for mock objects
- **`try_pick_and_lock_in_place` doc fixed** — stale comment about returning `(idx, did_unlock)` corrected
- **`WalletInfo.idx` annotated** — `#[allow(dead_code)]` added (field used only in tests)
- **Test assertion bugs fixed** — 3 pre-existing bugs exposed when tests first compiled (`Http::new` signature, seed amount assertion, sender count assertion)
- **Sender index resolution bug fixed** — `try_pick_and_lock` returns a senders-list index (position within the filtered senders `Vec`), but this was passed directly to `manager.get_wallet()` which expects the **absolute wallet index** from the full wallet list. This caused the wrong wallet to be loaded — a low-balance target wallet instead of the actual sender. Fixed by resolving `senders[sender_list_idx].idx` before calling `fund_via_chain`.

## How it works (step by step)

1. **Parallel wallet loading** — scan `wallet_dir`, decrypt up to `--load-concurrency` (default 100) wallets concurrently via `Semaphore`, fetch balances
2. **Classify** — `prepare_funding_sets` filters senders (`balance ≥ min_balance`) and targets (`balance ≤ max_balance`), applies optional `max_targets` cap
3. **Get confirmation** — prints summary table, asks `[y/N]` (skip with `--yes` or `--dry-run`)
4. **Pre-compute spread** — if `--spread-hours` is set, generate per-target staggered delay offsets
5. **Execute loop** — split targets round-robin across `--workers` long-lived tasks; each worker sleeps for spread offset before sender selection and rests between its own cycles
6. **Pick sender** — `SenderState.try_pick_and_lock` picks a sender with remaining capacity and locks it
7. **Build proxy chain** — generate N fresh `LocalWallet`s, compute seed amount (target + gas for all hops + sender tx gas)
8. **Sender → P1** — send seed ETH from sender to first proxy, then wait for confirmation with heartbeat logs and a timeout
9. **P1 → P2 → ... → PN → Target** — each proxy forwards `remaining - 21k*gas_price` to the next, with random delay and fresh gas price; transient send failures retry up to 3 times, and confirmation waits also emit heartbeat logs before timing out
10. **Discard** — proxy private keys dropped after forwarding tx confirms
11. **Report** — `SenderState` tracks funded/failed counts + per-worker wall-clock durations
12. **Log final stats** — prints `=== Done ===` block with total wall-clock duration (`HH:MM:SS`) and per-worker min/max/avg (`HH:MM:SS`)
13. **Optional JSON log** — if `--json-log <path>` is given, writes structured JSON with all timing, funded/failed, and summary fields

## Output format

Runner lines use a compact timestamped prefix:

```text
hh:mm:ss [WK001] [Funder] address : 0x... balance: 2.524 ETH
hh:mm:ss [WK001] [Funder] Sending 0.0214 ETH to Proxy 1 {0x...} (target 0.02 ETH)
hh:mm:ss [WK001] [Proxy-1] Sending 0.0198 ETH to Proxy 2 {0x...}
hh:mm:ss [WK001] [Recipient] Address 0x... received 0.0196 ETH , tx : 0x... , cost 0.000001 ETH, duration : 3m 23s
```

- `cost` is computed as `funder_balance_before - funder_balance_after - recipient_received`.

After all funding completes, the terminal shows:

```
=== Done ===
Funded: 12, Failed: 1
Total duration: 00:03:47
Per-worker: min 00:00:12, max 00:00:34, avg 00:00:21 (12 workers)
```

- **Total duration** — wall-clock time from first target dispatch to last completion (HH:MM:SS)
- **Per-worker** — individual wall-clock time per spawned worker task (min / max / avg / count)

With `--json-log results.json` an additional file is written:

```json
{
  "funded": 12,
  "failed": 1,
  "total_duration_secs": 227.34,
  "per_worker": {
    "count": 12,
    "min_secs": 12.05,
    "max_secs": 34.82,
    "avg_secs": 21.14,
    "durations_secs": [12.05, 18.33, ...]
  },
  "summary": {
    "total_wallets": 50,
    "senders_count": 20,
    "targets_count": 15,
    "assigned": 12
  },
  "timestamp": "2026-05-21T12:34:56+00:00"
}
```

## Design decisions

- **Parallel wallet loading**: Wallets are decrypted concurrently at startup (`--load-concurrency`, default 100). This reduces startup time for large wallet sets and better uses the available CPU/RPC capacity.
- **Ephemeral wallets**: Each proxy is created via `LocalWallet::new(rng)` with `chain_id` set, used for exactly one forward tx, then dropped. No private key written to disk.
- **No refund of leftover dust**: Each hop leaves ~0.000001 ETH at 1 gwei. Not worth the extra 21k gas to collect. (t18/t19 now also keep the leftover ETH on the ephemeral proxy; no refund step.)
- **Gas price strategy**: Gets network gas price, sets floor to `max(min_gwei, 90% of network)`, ceiling to `min(max_gwei, 100)`. This keeps prices realistic while still random.
- **Seed over-funds**: The sender sends `target_amount + gas for all hops + gas for sender tx`. Each hop deducts just its own 21k gas cost. Ensures target receives at least `target_amount`.

## Dry run mode

`--dry-run` prints the full plan (sender → N hops → target, ETH amounts, addresses) without sending any transactions. Use this first to review what will happen.

## Related files

- `src/bin/fund.rs` — the binary itself (51 unit tests inline)
- `tests/fund_cli.rs` — CLI integration tests (4 tests via `assert_cmd`)
- `sepolia-funder.md` — this file
- `src/task/t18_receive_tplus.rs` — single-hop proxy pattern (inspiration)
- `src/task/t19_receive_cplus.rs` — same, C+ variant
