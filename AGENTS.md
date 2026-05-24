# AI/Agent Instructions for testnet-framework

Short, code-accurate guide for agents working in this repo.

## 1) MCP usage rule (CRITICAL - READ FIRST)

**⚠️ MCP tools MUST be used FIRST before any other action. Shell commands are LAST resort only.**

**PRIORITY ORDER: MCP tools FIRST, shell commands ONLY as fallback.**

### When to use which MCP tool:

#### filesystem MCP (Local file operations)
**USE FOR:** Reading, writing, searching, listing files in this repo
- ✅ `read_text_file` - Read file contents
- ✅ `read_multiple_files` - Read several files at once
- ✅ `list_directory` - See directory contents
- ✅ `search_files` - Find files by glob pattern (e.g., `**/*.rs`)
- ✅ `get_file_info` - File metadata (size, dates, permissions)
- ✅ `write_file` - Create/overwrite files
- ✅ `edit_file` - Make targeted edits (use `dryRun: true` to preview)
- ✅ `create_directory` - Create directories
- ✅ `move_file` - Move/rename files

**RULES:**
- ALWAYS use absolute paths: `C:\My Script\testnet-framework\...`
- For discovery tasks, use `search_files` or `list_directory` FIRST
- For reading code, use `read_text_file` NOT shell `cat`/`type`
- For finding files, use `search_files` NOT shell `find`/`dir`

#### context-mode MCP (Command execution + large output handling)
**USE FOR:** Running commands that produce lots of output, indexing documentation

- ✅ `ctx_execute` - Run commands, auto-index output, search with queries
  - **PREFER this over shell** for: `git log`, `cargo build`, test runs, `npm test`, API calls
  - Use `intent` parameter to describe what you're looking for
  - Output gets indexed - use `ctx_search` to retrieve specific sections
- ✅ `ctx_execute_file` - Process a file without loading it into context
  - Use for: Large logs, data files (CSV/JSON), big source files
- ✅ `ctx_index` - Index documentation/knowledge into searchable database
  - Use for: API docs, README files, migration guides, code examples
- ✅ `ctx_search` - Search indexed content with multiple queries
  - Batch ALL questions in one call
  - Use 2-4 specific technical terms per query
- ✅ `ctx_batch_execute` - Execute multiple commands, index all output, search once
  - **PRIMARY TOOL** for complex multi-step tasks
  - Replaces 30+ execute calls + 10+ search calls
  - Provide all commands and all search queries in ONE call

**RULES:**
- Force repo cwd: `cd "C:\My Script\testnet-framework" && ...`
- Use `intent` parameter to guide output filtering
- After indexing, use `search` to retrieve specific sections

#### tavily MCP (Web search and content extraction)
**USE FOR:** Web searches, extracting content from URLs, research tasks

- ✅ `tavily_search` - Search web for current information
  - Use for: News, facts, latest versions, error solutions
  - Returns snippets + source URLs
- ✅ `tavily_extract` - Extract content from specific URLs
  - Use when user provides URLs to check
  - Returns markdown or text
- ✅ `tavily_research` - Comprehensive research on a topic
  - Use for: Complex topics needing multiple sources
  - Returns detailed research report
- ✅ `tavily_skill` - Search library/API documentation
  - Use when working with specific libraries
  - Returns structured documentation chunks
- ⚠️ `tavily_map` - May fail with URL validation in this environment
- ⚠️ `tavily_crawl` - May fail with URL validation in this environment

**RULES:**
- Prefer `tavily_search` over `tavily_map/crawl` for reliability
- Use `tavily_extract` if `ctx_fetch_and_index` fails with TLS error
- Always specify `max_results` when you need specific number of results

#### memory MCP (Persistent knowledge)
**USE FOR:** Saving important facts that should persist across conversations

- ✅ `create_entities` - Save new concepts with observations
- ✅ `add_observations` - Add notes to existing entities
- ✅ `read_graph` - Read entire knowledge graph
- ✅ `search_nodes` - Search for entities by query
- ✅ `open_nodes` - Retrieve specific entities by name

**RULES:**
- ONLY use when information should persist beyond current session
- DO NOT use for temporary context or current session facts
- Keep observations concise and factual
- Use for: User preferences, project architecture decisions, common patterns

#### sequential-thinking MCP (Complex reasoning)
**USE FOR:** Breaking down complex problems into structured steps

- ✅ `sequentialthinking` - Multi-step analytical thinking
  - Use for: Architecture decisions, debugging complex issues, multi-step planning
  - Can revise previous thoughts, branch into new paths
  - Express uncertainty when present

**RULES:**
- Use for problems requiring 3+ analytical steps
- Can adjust `totalThoughts` up/down as understanding evolves
- Mark `isRevision: true` when reconsidering previous thoughts
- Set `nextThoughtNeeded: false` ONLY when truly done

### MCP Tool Selection Decision Tree:

```
Need to read/find files in repo?
  → filesystem MCP (read_text_file, search_files, list_directory)

Need to run commands with output?
  → context-mode MCP (ctx_execute, ctx_batch_execute)

Need web information/latest versions?
  → tavily MCP (tavily_search, tavily_research, tavily_skill)

Need to save important facts?
  → memory MCP (create_entities, add_observations)

Need to analyze complex problem?
  → sequential-thinking MCP

Need external app integration (GitHub, Slack)?
  → composio MCP (start with COMPOSIO_SEARCH_TOOLS)
```

### Required operating rules:
1. Always use absolute paths with filesystem MCP tools
2. For context-mode, force repo cwd: `cd "C:\My Script\testnet-framework" && ...`
3. For composio: Start with `COMPOSIO_SEARCH_TOOLS`, reuse `session_id` in subsequent calls
4. For composio tools with `schemaRef`: Fetch schema via `COMPOSIO_GET_TOOL_SCHEMAS` first
5. Never invent tool arguments - stay schema-compliant
6. **ALWAYS try MCP tools before falling back to shell commands**

### Known environment caveats (verified):
- ✅ `filesystem` MCP - Full functionality working
- ✅ `context-mode` `ctx_execute` - Working correctly
- ⚠️ `context-mode` `ctx_fetch_and_index` - May fail with TLS error (`UNABLE_TO_GET_ISSUER_CERT_LOCALLY`)
  - **Fallback:** Use `tavily_extract` for URL content
- ✅ `memory` MCP - Entity creation/retrieval working
- ✅ `sequential-thinking` MCP - Working correctly
- ✅ `tavily_search`, `tavily_extract`, `tavily_research`, `tavily_skill` - All working
- ⚠️ `tavily_map`, `tavily_crawl` - May return invalid start URL errors
  - **Fallback:** Use `tavily_search` + `tavily_extract` combination

### Fallback hierarchy:
1. Try appropriate MCP tool first
2. If MCP fails, try alternative MCP tool (e.g., tavily_extract if ctx_fetch fails)
3. Only use shell commands as last resort for local repo operations
4. Document any MCP failures in conversation for future reference

## 2) Current workspace map

- Workspace members (`Cargo.toml`):
  - `core-logic`
  - `chains/risechain`
  - `chains/xenea`
  - `chains/da-chain`
  - `chains/tempo-spammer`
  - `chains/robinhood`
  - `chains/sepolia-overlayer`
- Chain templates exist as folders only: `_template_evm`, `_template_solana`.

## 3) Fast command reference

Build/check alternatives:

```powershell
# Main build script (uses target\final)
.\_clean_and_compile_all.bat

# Alternative: workspace build directly
cargo build --workspace

# Fast validation
cargo check --workspace

# Formatting and lint
cargo fmt
cargo clippy --workspace
```

Run alternatives by crate/bin:

```powershell
# RISE main spammer
$env:WALLET_PASSWORD="password"; cargo run -p rise-project -- --config chains/risechain/config.toml

# RISE debugger binary (targeted task or --all)
$env:WALLET_PASSWORD="password"; cargo run -p rise-project --bin debug_task -- --config chains/risechain/config.toml --all

# XENEA main spammer
$env:WALLET_PASSWORD="password"; cargo run -p xenea-project -- --config chains/xenea/config.toml

# XENEA debugger binary (targeted task or --all)
$env:WALLET_PASSWORD="password"; cargo run -p xenea-project --bin xenea-debug_task -- --config chains/xenea/config.toml --all

# Robinhood
cargo run -p robinhood-spammer --bin robinhood-spammer -- --config chains/robinhood/config.toml

# Tempo
cargo run -p tempo-spammer --bin tempo-spammer -- --config chains/tempo-spammer/config/config.toml

# Sepolia
$env:WALLET_PASSWORD="password"; cargo run -p sepolia-overlayer -- --config chains/sepolia-overlayer/config.toml

# Sepolia debugger
$env:WALLET_PASSWORD="password"; cargo run -p sepolia-overlayer --bin sepolia-debug_task -- --config chains/sepolia-overlayer/config.toml --all
```

Alternative execution path if `cargo run` is slow:
- build once with `cargo build --workspace`
- run binaries from `target\debug\...`.

## 4) Real code entry points

- Core shared logic: `core-logic/src/lib.rs`
  - Exports: config, database, metrics, security, traits, templates, selected utils.
- Gas tracker utility: `core-logic/src/utils/explorer_gas_tracker.rs`
  - Fetches and parses explorer gas pages from a payload-driven URL + row label.
- Wallet flow: `core-logic/src/utils/wallet_manager.rs`
  - Auto-detects `wallet-json/`.
  - Fallback to `pv.txt`.
  - Supports chain-targeted key extraction and cache.
  - Uses `#[derive(Default)]` with `#[default]` on `ChainType::Evm`.
- Worker orchestration: `core-logic/src/utils/runner.rs`
  - Runs spammers concurrently with `CancellationToken` and Ctrl+C graceful shutdown.

RISE:
- Main: `chains/risechain/src/main.rs`
- Spammer: `chains/risechain/src/spammer/mod.rs`
- Task registry/context: `chains/risechain/src/task/mod.rs`
- Debug tool: `chains/risechain/src/bin/debug_task.rs`

XENEA:
- Main: `chains/xenea/src/main.rs`
- Spammer: `chains/xenea/src/spammer/mod.rs`
- Task registry/context: `chains/xenea/src/task/mod.rs`
- Debug tool: `chains/xenea/src/bin/debug_task.rs`
- Meme flow: `t07` deploys a mintable MEME contract, `t61` mints a random amount from a DB-selected MEME contract.

DA-CHAIN:
- Main: `chains/da-chain/src/main.rs`
- Spammer: `chains/da-chain/src/spammer/mod.rs`
- Debug tool: `chains/da-chain/src/bin/debug_task.rs`
- Gas helper: `chains/da-chain/src/utils/gas.rs`
- Uses shared `ExplorerGasTracker` payloads for explorer gas reads.

Robinhood:
- bins in `chains/robinhood/bin/`
- core in `chains/robinhood/src/`

Tempo:
- bins in `chains/tempo-spammer/bin/`
- task catalog in `chains/tempo-spammer/src/tasks/`

Sepolia:
- Main: `chains/sepolia-overlayer/src/main.rs`
- Spammer: `chains/sepolia-overlayer/src/spammer/mod.rs`
- Task registry/context: `chains/sepolia-overlayer/src/task/mod.rs`
- Debug tool: `chains/sepolia-overlayer/src/bin/debug_task.rs`
- Funder binary (`sepolia-funder`): `chains/sepolia-overlayer/src/bin/fund.rs`
  - Strongly TDD'd for fast/safe iteration: pure helpers for filtering, `generate_dry_run_plan`, gas selection (`choose_gas_price_mgwei`), hop math (`calculate_seed_amount`, etc.), and injectable confirmation (`confirm_funding` taking BufRead).
  - 47 unit tests + CLI integration tests (`tests/fund_cli.rs` using assert_cmd).
  - See `sepolia-funder.md` for usage and `src/bin/fund.rs` tests for the testable surface.

## 5) RISE task wiring (important)

- Tasks are explicitly imported and appended in `chains/risechain/src/spammer/mod.rs`.
- Tasks are also exposed in `chains/risechain/src/task/mod.rs`.
- Debug selection is name/prefix aware in `debug_task.rs`.

When adding a RISE task:
1. Create `chains/risechain/src/task/tXX_name.rs`.
2. Register module/export in `task/mod.rs`.
3. Add to `tasks: Vec<Box<RiseTask>>` in `spammer/mod.rs`.
4. Add to `debug_task.rs` task list if you need direct debug support.

Alternative pattern:
- If task is experimental, wire only in `debug_task.rs` first.
- After validation, add to spammer production list.

## 6) Debug checklist (first checks)

1. `WALLET_PASSWORD` is set and decrypts wallet 0.
2. `--config` path matches crate (`risechain`, `xenea`, `robinhood`, `tempo-spammer`).
3. DB lock conflicts (`rise.db`, `tempo-spammer.db`, etc.).
4. RPC reachable and chain id correct.
5. Proxy format valid if enabled.

Alternative diagnosis commands:

```powershell
$env:RUST_BACKTRACE=1
$env:RUST_LOG=debug
cargo run -p rise-project --bin debug_task -- --config chains/risechain/config.toml --task 1
```

## 7) Code conventions to preserve

- Rust 2021 in most crates (`tempo-spammer` is edition 2024).
- Prefer explicit imports over wildcard imports.
- Error handling with `anyhow` + context (`.context(...)`).
- Async-safe patterns (`tokio::select!`, cancellation token).
- Structured logging with `tracing`.
- Run `cargo fmt` after edits.
- Maintain zero clippy warnings (`cargo clippy --workspace`) before committing.

### Applied clippy conventions (core-logic reference)

When fixing clippy warnings, follow these patterns (examples from `core-logic`):

| Lint | Pattern | Example fix |
|------|---------|-------------|
| `derivable_impls` | Replace manual `impl Default` with `#[derive(Default)]` | `proxy_health.rs`, `wallet_manager.rs` |
| `unnecessary_map_or` | `.map_or(true, pred)` → `.is_none_or(pred)`, `.map_or(false, pred)` → `.is_some_and(pred)` | `memory_monitor.rs`, `memory_optimized_logger.rs` |
| `redundant_closure` | `|| Metric::default()` → `Metric::default` | `metrics.rs` |
| `await_holding_lock` | Scope `MutexGuard` in a block to drop before `.await` | `rate_limiter.rs` |
| `too_many_arguments` | Add `#[allow(clippy::too_many_arguments)]` on private helpers where refactoring would add unnecessary complexity | `database.rs` |

## 8) Security rules

- Never commit secrets (`.env`, keys, wallet payloads, proxy creds, db files, logs).
- Never log private keys, mnemonics, raw passwords, API keys.
- Keep sensitive structs zeroized where applicable.

Immediate security note (current repo state):
- `chains/tempo-spammer/Cargo.toml` contains Telegram bot token/chat_id in `[package.metadata.telegram]`.
- Recommended fix: move to environment variables or `.env` and rotate token.

Alternative mitigation if immediate rotation is blocked:
- keep token in local untracked config file and load at runtime,
- add explicit CI check to block committed token patterns.

## 9) Documentation sync rule

When code structure changes:
1. Update `AGENTS.md` (this file) first.
2. Then update `CODEBASE.md` and `CMD.md` to keep commands/tree accurate.

Keep explanations short and practical.
