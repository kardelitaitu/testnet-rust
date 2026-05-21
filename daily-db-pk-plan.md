# Plan: DB-Level Limit Enforcement via PK Change

## Current schema

```sql
CREATE TABLE daily_task_completions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,   -- synthetic, no uniqueness
    wallet_address TEXT NOT NULL,
    task_name TEXT NOT NULL,
    date TEXT NOT NULL,
    completed_at INTEGER NOT NULL,
    success INTEGER NOT NULL DEFAULT 0,
    message TEXT DEFAULT ''
);
```

- **PK**: synthetic `id` — no DB-level uniqueness on (wallet, task, date)
- **Counting**: `SELECT COUNT(*) WHERE success=1 GROUP BY task_name` — multiple rows per wallet-task-date
- **Row growth**: unbounded — every completion = new row
- **Failures**: stored as `success=0`, never queried

## Proposed schema

```sql
CREATE TABLE daily_task_completions (
    wallet_address TEXT NOT NULL,
    task_name TEXT NOT NULL,
    date TEXT NOT NULL,
    count_success INTEGER NOT NULL DEFAULT 0,
    count_failed INTEGER NOT NULL DEFAULT 0,
    message TEXT DEFAULT '',
    completed_at INTEGER NOT NULL,
    PRIMARY KEY (wallet_address, task_name, date)    -- ← DB-enforced uniqueness
);
```

- **PK**: `(wallet_address, task_name, date)` — physically impossible to have duplicate entries
- **Counting**: `SELECT count_success` — no GROUP BY needed, just read the column
- **Row growth**: EXACTLY 17 rows per wallet per day (one per task) — bounded
- **Failures**: tracked in `count_failed` column, separate from success

## What changes

### A. Schema (init_schema)

- Migration detection: check for `id` column (v3 schema) → DROP and recreate
- New table: PK = (wallet_address, task_name, date), columns: count_success, count_failed, message, completed_at
- No index needed — PK IS the lookup key

### B. Queries (simplify)

| Query | Current | New |
|-------|---------|-----|
| `get_completed_counts` | `SELECT task_name, COUNT(*) WHERE success=1 GROUP BY task_name` | `SELECT task_name, count_success WHERE date=?` |
| `get_total_completed` | `SELECT COUNT(*) WHERE success=1` | `SELECT SUM(count_success) WHERE date=?` |
| `get_all_completed_counts` | `SELECT wallet, task, COUNT(*) WHERE success=1 GROUP BY wallet, task` | `SELECT wallet, task, count_success WHERE date=?` |

All GROUP BY and success filters eliminated — simpler, faster.

### C. record_task_completion (UPSERT)

Current:
```sql
INSERT INTO daily_task_completions (wallet_address, task_name, date, completed_at, success, message)
VALUES (?, ?, ?, ?, ?, ?)
```

New on success:
```sql
INSERT INTO daily_task_completions (wallet_address, task_name, date, completed_at, message)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT(wallet_address, task_name, date) DO UPDATE SET
    count_success = count_success + 1,
    completed_at = excluded.completed_at,
    message = excluded.message
```

New on failure:
```sql
INSERT INTO daily_task_completions (wallet_address, task_name, date, completed_at, message)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT(wallet_address, task_name, date) DO UPDATE SET
    count_failed = count_failed + 1,
    completed_at = excluded.completed_at,
    message = excluded.message
```

### D. record_task_completion signature

Current: `success: bool` parameter
New: no change — still takes `success: bool`, maps to either `count_success += 1` or `count_failed += 1`

### E. Tests

- Update `test_empty_db_returns_empty_counts` — unchanged (still returns empty)
- Update `test_record_and_retrieve_single` — verify count_success = 1
- Update `test_record_multiple_same_task_increments_count` — verify count_success = 5
- Update `test_failed_task_not_counted` — verify count_success = 0, count_failed = 1
- Update schema migration test — drop old schema, create new one
- Update all integration tests that call `record_task_completion` — likely pass as-is (interface unchanged)

**New test: `test_duplicate_insert_does_not_overflow_limit`**
- Insert same wallet+task+date 100 times
- Verify count_success = 1 (first UPSERT sets it to 1, subsequent UPSERTs increment)
- This proves the DB physically prevents over-counting

## Risk

| Change | Risk | Mitigation |
|--------|------|------------|
| Schema migration | LOW — old data is daily-scoped, safe to drop | Migration detects old table and DROPs it |
| Query changes | LOW — simpler, fewer moving parts | All queries return same types |
| UPSERT semantics | LOW — SQLite ON CONFLICT is well-tested | Existing tests cover success/failure paths |
| Interface change | NONE — same `record_task_completion(wallet, task, date, success, msg)` | No callers need changing |

## Execution order

1. Update `init_schema` — detect old `id` column, DROP + recreate with new PK
2. Update `record_task_completion` — split into success/failure UPSERT paths
3. Update `get_completed_counts` — `SELECT count_success` instead of `COUNT(*)`
4. Update `get_total_completed` — `SELECT SUM(count_success)` instead of `COUNT(*)`
5. Update `get_all_completed_counts` — `SELECT count_success` instead of `COUNT(*)`
6. Update migration test
7. Add `test_duplicate_insert_does_not_overflow_limit`
8. Update all affected database tests
9. `cargo check + cargo test` — all 190 tests + new ones
