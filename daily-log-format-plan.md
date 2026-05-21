# Plan: Console Log Format for sepolia-daily

## Goal

Change the 4 logging lines in `daily_runner/mod.rs` from:

```
[Daily WK:0][WL:5] OK  [01_checkBalance] Balance: 0.5 ETH
[Daily WK:0][WL:5] RETRY [10_aaveUsdtFaucet] RPC timeout
[Daily WK:0][WL:5] TIMEOUT [unknown] Task exceeded 300s timeout
[Daily WK:0][WL:5] All tasks at daily capacity
```

To:

```
14:32:17 [WK:000][WL:0005][P:003] OK     01_checkBalance Balance: 0.5 ETH
14:32:17 [WK:000][WL:0005][P:---] RETRY  10_aaveUsdtFaucet RPC timeout
14:32:17 [WK:000][WL:0005][P:---] TIMEOUT unknown Task exceeded 300s timeout
14:32:17 [WK:000][WL:0005][P:---] LIMIT  All tasks completed for today
```

## Why `select_proxy` already returns proxy_id

The `select_proxy()` function (line 716) already returns `(Option<ProxyConfig>, String)` — the string is `"003"` (1-indexed, 0-padded) or `"000"` when no proxy. It's just being discarded with `_proxy_id`.

## Changes Required

### 1. TaskOutcome enum — add `proxy_id` field to all 4 variants

**File:** `daily_runner/mod.rs` L149-158

```rust
enum TaskOutcome {
    Success { task_name: String, message: String, proxy_id: String },
    Retry { task_name: String, message: String, proxy_id: String },
    Timeout { task_name: String, message: String, proxy_id: String },
    WalletComplete { wallet_idx: usize, proxy_id: String },
}
```

Rationale: Walling off proxy_id into a separate return channel (e.g. `(TaskOutcome, String)`) creates more messy wiring through the timeout wrapper. Putting it on every variant is uniform and the field cost is trivial.

For sites where no proxy exists yet (early retry before `select_proxy`, Timeout, WalletComplete), use `"---".into()`.

### 2. execute_one_task — capture proxy_id (1 line)

**L565:** Change `_proxy_id` to `proxy_id`.

### 3. execute_one_task — add proxy_id to all 8 return sites

**Mapping:**

| Line | Variant | proxy_id value | Site |
|------|---------|----------------|------|
| 536  | Retry   | `"---"` | DB error (before proxy select) |
| 547  | WalletComplete | `"---"` | No remaining capacity |
| 557  | Retry   | `"---"` | Task not found |
| 607  | Retry   | `proxy_id.clone()` | Wallet parse error (has proxy) |
| 615  | Retry   | `proxy_id.clone()` | Wallet decrypt error (has proxy) |
| 674  | Success | `proxy_id.clone()` | Task succeeded |
| 680  | Retry   | `proxy_id.clone()` | Task returned success=false |
| 703  | Retry   | `proxy_id.clone()` | Task returned Err |

Total: 8 distinct return statements, 4 need `proxy_id.clone()`, 4 get `"---"`.

### 4. Worker loop Timeout arm (L462) — add proxy_id

The Timeout is created after `execute_one_task` timed out. We don't have proxy info at this level — use `"---"`.

### 5. Worker loop match (L478-512) — 4 format string changes

Each arm destructures `proxy_id` and uses the new format string:

**Format string:**
```
"{} [WK:{:03}][WL:{:04}][P:{}] {:<7}"
```

Where:
- `{}` = `chrono::Local::now().format("%H:%M:%S")`
- `{:03}` = worker_id zero-padded to 3 digits
- `{:04}` = wallet_idx zero-padded to 4 digits
- `{}` = proxy_id (already a formatted string like "003" or "---")
- `{:<7}` = left-aligned status label in 7-char field

**Status labels:**
- OK      (2 + 5 spaces = 7)
- RETRY   (5 + 2 spaces = 7)
- TIMEOUT (7 + 0 spaces = 7)
- LIMIT   (5 + 2 spaces = 7)

**Outcome → label mapping:**

```
Success         → "OK"      → `info!`
Retry           → "RETRY"   → `info!`
Timeout         → "TIMEOUT" → `error!`   (currently `warn!` — only ERROR visible on console)
WalletComplete  → "LIMIT"   → `info!`
```

For TIMEOUT: change from `warn!` to `error!` because only ERROR level is visible to console by default (see `setup_logger`). Timeouts should never be invisible.

### 6. Add `use chrono::Local;` import (L36)

Currently only `use chrono::Timelike;` is imported. Need `use chrono::{Local, Timelike};`.

### Files changed

**Only one file:** `chains/sepolia-overlayer/src/daily_runner/mod.rs`

No other files touched. No config changes. No new dependencies. No test changes needed.

## Risk Assessment

| Change | Risk | Mitigation |
|--------|------|------------|
| TaskOutcome enum change | LOW — internal struct, no pub API consumers | Compiler catches all match arms |
| 8 return sites updated | LOW — mechanical change | 1-char typos possible but caught by compiler |
| Format string change | LOW — cosmetic | Run a single wallet to verify output |
| warn! → error! for Timeout | MEDIUM — logging level change | Timeout should be visible, it indicates network/RPC issues |
| Working tests | NONE | 189 sepolia tests don't test the daily runner workload (no MCP server fixture), so no test impact |

## Test Plan

- `cargo check -p sepolia-overlayer` — ensures all TaskOutcome references compile
- Run `cargo test -p sepolia-overlayer` — all 189 existing tests must pass unchanged
- Visual inspect the binary output (1 worker, 1 wallet, no proxy) to verify format

## Execution Order

1. Add `chrono::Local` to imports
2. Update TaskOutcome enum definition (all 4 variants)
3. Change `_proxy_id` → `proxy_id` in `execute_one_task`
4. Add `proxy_id: "---".into()` to the 4 pre-proxy return sites
5. Add `proxy_id: proxy_id.clone()` to the 4 post-proxy return sites
6. Add `proxy_id: "---".into()` to the Timeout creation in worker_loop
7. Rewrite all 4 match arms in worker_loop with new format + destructuring
8. Build check + test + visual run
