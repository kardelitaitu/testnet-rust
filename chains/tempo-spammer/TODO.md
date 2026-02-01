# Tempo-Spammer Development TODOs

This document tracks all TODO, FIXME, and HACK items found in the codebase for systematic resolution.

**Last Updated:** 2024-01-30
**Total Items:** 44

---

## Quick Stats

| Priority | Count | Status |
|----------|-------|--------|
| 🔴 High | 5 | 0/5 completed |
| 🟡 Medium | 15 | 0/15 completed |
| 🟢 Low | 24 | 0/24 completed |

---

## 🔴 High Priority

### [#770] Sophisticated 2D Pool Approach
**File:** `src/utils/transaction-pool/src/tt_2d_pool.rs:770`
**Type:** TODO
**Description:** Current implementation simply pops any non-pending transaction. Needs a more sophisticated approach for transaction selection.
**Impact:** Affects transaction pool efficiency and throughput
**Suggested Solution:** Implement priority-based selection, fee market sorting, or time-in-pool weighting

### [#699] Support 2D Pool
**File:** `src/utils/transaction-pool/src/tempo_pool.rs:699`
**Type:** TODO
**Description:** Add support for 2-dimensional transaction pool (account + time based)
**Impact:** Major feature for advanced mempool management
**Dependencies:** Related to #770

### [#50] Remove AMM Read-Only Wrapper Hack
**File:** `src/utils/precompiles/src/tip_fee_manager/amm.rs:50`
**Type:** TODO (rusowsky)
**Description:** Create a proper read-only wrapper callable from read-only context with DB access instead of current hack
**Impact:** Code quality, maintainability
**Priority:** Technical debt reduction

### [#43] Remove Account Keychain Read-Only Wrapper Hack
**File:** `src/utils/precompiles/src/account_keychain/mod.rs:43`
**Type:** TODO (rusowsky)
**Description:** Similar to #50 - create proper read-only wrapper for account keychain
**Impact:** Code quality, maintainability
**Related:** #50

### [#1397] Implement Multi-Slot Storage Support
**File:** `src/utils/precompiles/src/storage/types/vec.rs:1397`
**Type:** TODO (rusowsky)
**Description:** Implement and test multi-slot support for storage vectors
**Impact:** Storage scalability
**Status:** Partially implemented, needs testing

---

## 🟡 Medium Priority

### EVM Module Optimizations

#### [#85] Move EVM Operations to Function Beginning
**File:** `src/utils/evm/src/evm.rs:85`
**Type:** TODO (rakita)
**Description:** Move certain EVM operations to the beginning of the function (requires fork)
**Impact:** Performance optimization
**Note:** Three similar TODOs at lines 85, 111, 170

### Precompile Macros Improvements

#### [#153] Compiler Evaluation Checks (HACK)
**File:** `src/utils/precompiles-macros/src/packing.rs:153`
**Type:** HACK
**Description:** Leverage compiler evaluation checks to ensure full type can fit
**Impact:** Type safety at compile time
**Locations:** Lines 153, 180, 319 (three similar HACKs)

### Testing Infrastructure

#### [#178] Fix TIP Fee AMM Test
**File:** `src/utils/node/tests/it/tip_fee_amm.rs:178`
**Type:** TODO
**Description:** Fix failing test (marked as currently broken)
**Impact:** Test coverage

#### [#366] Suggested Fee Recipient Test
**File:** `src/utils/node/tests/it/tip_fee_amm.rs:366`
**Type:** TODO
**Description:** Uncomment test when suggested fee recipient can be set to non-zero in debug config
**Impact:** Fee management testing

#### [#304] PrecompileError Propagation
**File:** `src/utils/node/tests/it/tip20.rs:304`
**Type:** TODO
**Description:** Update to expect exact error once PrecompileError is propagated through revm
**Impact:** Error handling accuracy
**Locations:** Lines 304, 480, 497, 635 (four similar TODOs)

### E2E Testing

#### [#112] Remove Execution Provider Panic Workaround
**File:** `src/utils/e2e/src/tests/sync.rs:112`
**Type:** TODO
**Description:** Remove workaround once panic using `execution_provider_offline` is fixed
**Impact:** Test reliability
**Locations:** Lines 112, 286 (two similar TODOs)

#### [#124] FIXME: Execution Provider Panic
**File:** `src/utils/e2e/src/tests/sync.rs:124`
**Type:** FIXME
**Description:** Test panics even though docs suggest it shouldn't
**Impact:** Test stability
**Locations:** Lines 124, 298 (two similar FIXMEs)

#### [#11] Linkage Test Loop
**File:** `src/utils/e2e/src/tests/linkage.rs:11`
**Type:** FIXME (janis)
**Description:** Figure out how to run linkage test in a loop
**Impact:** Test coverage for linkage scenarios
**Locations:** Lines 11, 74 (two similar FIXMEs)

#### [#24] Marshal vs Sync Terminology
**File:** `src/utils/e2e/src/tests/linkage.rs:24`
**Type:** TODO (janis)
**Description:** Commonware calls this "marshal", we call this "sync" - align terminology
**Impact:** Code clarity, cross-team communication
**Locations:** Lines 24, 34, 38, 51, 131 (five similar TODOs)

#### [#101] Non-Deterministic Events
**File:** `src/utils/e2e/src/tests/linkage.rs:101`
**Type:** FIXME (janis)
**Description:** Events are currently not fully deterministic, affecting test reliability
**Impact:** Test stability

#### [#111] Reach Height 1000 in Tests
**File:** `src/utils/e2e/src/tests/linkage.rs:111`
**Type:** TODO (janis)
**Description:** Would be great to reach height 1000, but execution provider setup limits this
**Impact:** Long-running test scenarios

### Node Configuration

#### [#445] Configure Dedicated Limit
**File:** `src/utils/node/src/node.rs:445`
**Type:** TODO
**Description:** Configure dedicated limit for specific resource
**Impact:** Resource management

#### [#75] Replace with Config Struct
**File:** `src/utils/e2e/src/testing_node.rs:75`
**Type:** FIXME
**Description:** Replace complex parameter list with a `Config` struct for better readability
**Impact:** Code maintainability

### E2E Library

#### [#359] Peer List Exposure
**File:** `src/utils/e2e/src/lib.rs:359`
**Type:** TODO
**Description:** Should be possible to remove peer tracking if Commonware simulated network exposes list of registered peers
**Impact:** Simplify E2E testing infrastructure
**Dependencies:** External - Commonware API

### Execution Runtime

#### [#137] Test Genesis Owner
**File:** `src/utils/e2e/src/execution_runtime.rs:137`
**Type:** TODO (janis)
**Description:** Figure out the owner of the test-genesis.json
**Impact:** Test data management

#### [#320] Cargo Manifest Prefix
**File:** `src/utils/e2e/src/execution_runtime.rs:320`
**Type:** TODO (janis)
**Description:** Determine if cargo manifest prefix is needed
**Impact:** Build configuration

#### [#670] Allow Configuring Parameter
**File:** `src/utils/e2e/src/execution_runtime.rs:670`
**Type:** TODO (janis)
**Description:** Allow configuring this parameter
**Impact:** Test flexibility

#### [#680] Node Identification
**File:** `src/utils/e2e/src/execution_runtime.rs:680`
**Type:** TODO (janis)
**Description:** Would be nicer if we could identify the node somehow
**Impact:** Debugging, logging

---

## 🟢 Low Priority

### Transaction Envelope

#### [#537] Transaction Validation Question
**File:** `src/utils/primitives/src/transaction/envelope.rs:537`
**Type:** TODO
**Description:** "Will this work?" - question about transaction validation logic
**Impact:** Validation correctness
**Status:** Needs verification

### Storage Migration

#### [#1407] StorageCtx Migration Test
**File:** `src/utils/precompiles/src/storage/types/vec.rs:1407`
**Type:** MIGRATION TODO
**Description:** Test needs migration to StorageCtx::enter pattern
**Impact:** Test modernization

### Fee Manager

#### [#257] Deploy and Set User Token
**File:** `src/utils/precompiles/src/tip_fee_manager/mod.rs:257`
**Type:** TODO
**Description:** Loop through and deploy/set user token for some range
**Impact:** Fee token setup automation

### EVM Logging

#### [#102] Revert Log Support
**File:** `src/utils/evm/src/evm.rs:102`
**Type:** TODO
**Description:** Remove once revm supports emitting logs for reverted transactions
**Impact:** Debugging capabilities
**Dependencies:** External - revm library update

#### [#372] Block Revert Log Support
**File:** `src/utils/evm/src/block.rs:372`
**Type:** TODO
**Description:** Remove once revm supports emitting logs for reverted transactions
**Impact:** Debugging capabilities
**Related:** #102

#### [#235] Add Namespace
**File:** `src/utils/evm/src/block.rs:235`
**Type:** TODO
**Description:** Add namespace for better organization
**Impact:** Code organization

---

## Resolution Guidelines

### When Resolving TODOs

1. **Update this file** - Mark as completed with date and PR/issue reference
2. **Remove from code** - Delete the TODO comment once resolved
3. **Document changes** - If behavior changes, update relevant documentation
4. **Test coverage** - Ensure tests exist for resolved items

### Priority Definitions

- **🔴 High:** Security issues, performance bottlenecks, or blocking bugs
- **🟡 Medium:** Important improvements, technical debt, or missing features
- **🟢 Low:** Nice-to-have improvements, documentation, or refactoring

### Format for New TODOs

When adding new TODOs to the codebase, use this format:

```rust
// TODO(username): Brief description
// Impact: What this affects
// Priority: High/Medium/Low
```

---

## Completed Items

| Date | Item | File | Resolution |
|------|------|------|------------|
| _None yet_ | - | - | - |

---

## How to Contribute

1. Pick a TODO from this list
2. Create a branch: `fix/todo-XXX-description`
3. Implement the fix
4. Update this TODO.md file
5. Submit PR with "Closes #XXX" reference

## Questions?

- High priority items: Discuss in #dev-critical channel
- Medium priority: Standard PR review process
- Low priority: Good for new contributors

---

**Maintained by:** Development Team  
**Review Schedule:** Weekly on Mondays  
**Next Review:** 2024-02-05
