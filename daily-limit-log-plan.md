# Plan: LIMIT logging with wallet address + real proxy

## Goal

When a wallet is fully exhausted (all tasks at daily limit), log:
```
05:39:01 [WK:000][WL:0170][P:588] LIMIT  [09_unstakeCplus] Daily tasks done - Address : 0x4324...
```

Key changes:
1. **Select proxy FIRST** before checking wallet exhaustion — so P:xxx shows a real proxy
2. **Carry wallet address** in WalletComplete outcome
3. **Carry a task name** so the log shows which task was attempted
4. **LIMIT → no change** (user is happy with LIMIT label)

## Changes

### A. TaskOutcome::WalletComplete — add wallet_address

**File:** `daily_runner/mod.rs`

Current:
```rust
WalletComplete { wallet_idx: usize, proxy_id: String },
```

New:
```rust
WalletComplete { wallet_idx: usize, wallet_address: String, proxy_id: String },
```

### B. Reorder execute_one_task: proxy BEFORE wallet-complete check

Current order in `execute_one_task`:
```
1. wallet_addr = addresses[wallet_idx]
2. Get counts from DB
3. Compute pending tasks
4. If pending empty → WalletComplete (proxy_id: "---")
5. Pick task + find implementation
6. Select proxy ← TOO LATE
7. Execute task
```

New order:
```
1. wallet_addr = addresses[wallet_idx]
2. Get counts from DB
3. Select proxy (always, before any limit check)
4. Compute pending tasks
5. If pending empty → WalletComplete with real proxy_id + wallet_address
6. Pick task + find implementation
7. Execute task
```

### C. Wallet — which task name to show?

When wallet is exhausted, there's no single "last attempted" task. Use the first task in `ALL_TASK_NAMES` as a representative (or the task with the most completions). Simple approach: `ALL_TASK_NAMES[0]` (always shows `01_checkBalance`).

Better approach: pick the task with the **highest limit** that's at capacity — most meaningful to see.

Or simplest: `task_name: ALL_TASK_NAMES[0].to_string()`. Just one log message per exhausted wallet.

### D. Update WalletComplete match arm

Current:
```rust
TaskOutcome::WalletComplete { wallet_idx, proxy_id } => {
    info!(
        target: "task_result",
        "{} [WK:{:03}][WL:{:04}][P:{}] {:<7} All tasks completed for today",
        Local::now().format("%H:%M:%S"),
        worker_id, wallet_idx, proxy_id,
        "LIMIT",
    );
}
```

New:
```rust
TaskOutcome::WalletComplete { wallet_idx, wallet_address, proxy_id } => {
    info!(
        target: "task_result",
        "{} [WK:{:03}][WL:{:04}][P:{}] {:<7} [{}] Daily tasks done - Address : {}",
        Local::now().format("%H:%M:%S"),
        worker_id, wallet_idx, proxy_id,
        "LIMIT", task_name, wallet_address,
    );
}
```

Where `task_name` is the representative task name from the WalletComplete variant.

Wait — WalletComplete doesn't have task_name currently. Let me add it:

```rust
WalletComplete { wallet_idx: usize, wallet_address: String, task_name: String, proxy_id: String },
```

### E. Return sites

Only one return site for WalletComplete in `execute_one_task`:
```rust
// After proxy selection, if pending is empty:
TaskOutcome::WalletComplete {
    wallet_idx,
    wallet_address: wallet_addr.to_string(),
    task_name: ALL_TASK_NAMES[0].to_string(),
    proxy_id,
}
```

But `self.wallet_addresses` is `Vec<String>` so `wallet_addr` is `&String`. That's fine.

## Risk

| Change | Risk | Mitigation |
|--------|------|------------|
| Reorder proxy selection before limit check | LOW — just moves existing code up | Compiler catches any issues |
| Add fields to WalletComplete | LOW — only one return site + one match arm | Compiler catches |
| Log format change | LOW — cosmetic, visible in console | Run tests to verify no breakage |

## Test impact

- Unit tests that construct `TaskOutcome::WalletComplete` directly need updating
- No logic change — just new fields
- 190 existing tests must still pass

## Execution order

1. Add `wallet_address` and `task_name` to `WalletComplete` enum
2. Move proxy selection above the pending check in `execute_one_task`
3. Update the WalletComplete return site with real proxy_id + wallet_address
4. Update the WalletComplete match arm with new format
5. `cargo check` + `cargo test`
