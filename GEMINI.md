# 🧠 GEMINI.md: Rust Multi-Chain Testnet Framework

**Context Signature:** `RUST-MCTF-V2` | **System Architect:** Modular Agentic Design
**Optimization Strategy:** Lazy-Loading / Token-Gating / Trait-Based Abstraction

---

## 🗺️ Cognitive Project Map

> **Agent Directive:** Prioritize the **Kernel (`core-logic`)** for architectural rules and the **Implementation (`chains/`)** for specific logic. Never ingest `target/` or `Cargo.lock`.

| Component | Responsibility | Token Priority |
| :--- | :--- | :--- |
| **`core-logic`** | **The Kernel.** `WalletManager` (Lazy), `ProxyManager`, `Logger`, and the `RiseTask` Trait. | **CRITICAL** |
| **`chains/risechain`** | **Production Logic.** RISE-specific tasks (Faucet, Swap) and Debug Binaries. | **HIGH** |
| **`chains/tempo-spammer`** | **Production Logic.** Tempo-specific tasks (Faucet, Swap) and Debug Binaries. | **HIGH** |
| **`chains/evm-project`** | **The Template.** Modular blueprint for spawning new chain modules. | **MEDIUM** |

---

## 🛠️ Architecture & Core Logic

### 1. Lazy Wallet Security ($O(1)$ Load)

* **Path:** `core-logic/src/utils/wallet_manager.rs`
* **Pattern:** Discovery $\rightarrow$ On-Demand Decryption $\rightarrow$ Mutex Cache.
* **Hardware Context:** Optimized for **Ryzen 9 7950x** (High-concurrency thread safety).
* **Constraint:** Never propose bulk decryption. Use `manager.get_wallet(index)` only when the worker thread is ready to execute.

### 2. The Task Trait (Modular Orchestration)

All automation units must implement the `RiseTask` trait to ensure variant spawning and audit-ready execution.

* **Method:** `run(ctx: TaskContext) -> TaskResult`
* **Agent Rule:** When creating tasks, keep them "pure." Logic belongs in the task; wallet/proxy state belongs in the `TaskContext`.

### 3. High-Throughput Logging

* **Engine:** `tracing` + `tracing-appender`.
* **Output:** Stdout (Human-Readable) + `logs/smart_main.log` (Audit-Ready / Grepable).
* **Instruction:** All scripts must include feedback loops to maintain transparency and session discipline.

---

## ⚡ Agentic Workflow Protocols

### [PROTOCOL: CONTEXT_GATING]

* **Depth Limit:** Do not crawl more than 2 files deep into imports unless requested.
* **Snippet Focus:** Analyze only the `cargo check` or `clippy` error lines. Ignore full terminal dumps.
* **Macro Policy:** Treat `#[derive(...)]` and `tokio::main` as black boxes. Do not expand unless debugging macro-expansion errors specifically.

### [PROTOCOL: MODES_OF_OPERATION]

* **`<MODE:PLAN>`**: Architectural design (no code). Focus on entropy-rich structure and strategic foresight.
* **`<MODE:IMPLEMENT>`**: Modular Rust generation. Follow strict types and avoid `.unwrap()`.
* **`<MODE:DEBUG>`**: Analyze `debug_task.rs` output to trace specific wallet or network failures.

### [PROTOCOL: PERFORMANCE METRICS]

Use math-style formatting for all framework benchmarks:

* $$TPS = \frac{TotalTransactions}{TotalTime}$$
* $$SuccessRate = \frac{Success}{Success + Failure} \times 100$$
* $$EntropyLevel = \frac{UniqueModules}{TotalLinesOfCode}$$

---

## 🚀 Common Command Shortcuts

* **Global Build:** `._clean_and_compile_all.bat` (Source of truth for lockfile hygiene).
* **Logic Test:** `cargo run --bin debug_task` (The interactive gatekeeper for new tasks).
* **Full Deployment:** `cargo run --bin rise-project` (The parallelized spammer swarm).

---

## 📜 Inheritance & Guardrails

1.  **Safety:** Memory safety and ownership are paramount. Prefer references `&` over Clones `.clone()`.
2.  **Clarity:** Variable naming must reflect the **Modular Systems** philosophy (e.g., `wallet_provider` vs `w`).
3.  **Resilience:** Every system must be "audit-ready" and compatible with future operational agents.
4.  **Session Hygiene:** Clear the LLM context if a logic path becomes circular or bloated.

## 📜 Rules for Future Development
1.  **Do Not Break Compilation**: Always run `cargo check --workspace` after significant changes.
2.  **Preserve Lazy Loading**: Do not revert to eager decryption in `main.rs`. Iterate wallets via `manager.get_wallet(index)`.
3.  **Use the Debugger**: Test logic changes in `debug_task.rs` before deploying to the spammer swarm.
4.  **Passwords**: Always support `WALLET_PASSWORD` env var, but fallback to interactive prompt in CLI tools.

## 🚀 Common Commands
*   **Build**: `._clean_and_compile_all.bat`
*   **Debug**: `cargo run --bin debug_task`
*   **Spam**: `cargo run --bin rise-project`
