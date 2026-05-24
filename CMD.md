# Command Reference (CMD)

Short command cookbook for current workspace.

## 1) Build / Validate

Primary:
```powershell
.\_clean_and_compile_all.bat
```

Alternative:
```powershell
cargo build --workspace
```

Fast checks:
```powershell
cargo check --workspace
cargo fmt
cargo clippy --workspace
```

## 2) RISE (`rise-project`)

Run spammer:
```powershell
$env:WALLET_PASSWORD="password"; cargo run -p rise-project -- --config chains/risechain/config.toml
```

Alternative (binary direct after build):
```powershell
$env:WALLET_PASSWORD="password"; .\target\debug\rise-project.exe --config chains/risechain/config.toml
```

Run debugger interactive:
```powershell
$env:WALLET_PASSWORD="password"; cargo run -p rise-project --bin debug_task -- --config chains/risechain/config.toml
```

Check all wallet balances:
```powershell
$env:WALLET_PASSWORD="password"; cargo run -p rise-project --bin debug_task -- --config chains/risechain/config.toml --all
```

Run one task:
```powershell
$env:WALLET_PASSWORD="password"; cargo run -p rise-project --bin debug_task -- --config chains/risechain/config.toml --task 1
```

Alternative task targeting:
- Use task prefix match via task names in `debug_task.rs`.
- If prefix fails, fallback index is used when valid.

## 3) Xenea (`xenea-project`)

Main:
```powershell
$env:WALLET_PASSWORD="password"; cargo run -p xenea-project -- --config chains/xenea/config.toml
```

Debug:
```powershell
$env:WALLET_PASSWORD="password"; cargo run -p xenea-project --bin xenea-debug_task -- --config chains/xenea/config.toml
```

Task checks:
```powershell
$env:WALLET_PASSWORD="password"; cargo run -p xenea-project --bin xenea-debug_task -- --config chains/xenea/config.toml --all
$env:WALLET_PASSWORD="password"; cargo run -p xenea-project --bin xenea-debug_task -- --config chains/xenea/config.toml --task 1
$env:WALLET_PASSWORD="password"; cargo run -p xenea-project --bin xenea-debug_task -- --config chains/xenea/config.toml --task 61
```

Alternative:
- Build once, run `target\debug\xenea-project.exe` directly.

## 4) DA-Chain (`da-chain-project`)

Main:
```powershell
$env:WALLET_PASSWORD="password"; cargo run -p da-chain-project -- --config chains/da-chain/config.toml --min-gwei 600
```

Debug:
```powershell
$env:WALLET_PASSWORD="password"; cargo run -p da-chain-project --bin debug_task -- --config chains/da-chain/config.toml --min-gwei 600 --no-proxy
```

Gas behavior:
- `--min-gwei` sets the floor for explorer/RPC fee selection.
- `ExplorerGasTracker` reads `https://exptest.dachain.tech/gas-tracker` and feeds the shared gas helper.

## 5) Sepolia (`sepolia-overlayer`)

Wallet balance dump:
```powershell
$env:WALLET_PASSWORD="password"; cargo run -p sepolia-overlayer --bin wallet-balance-dump -- --config chains/sepolia-overlayer/config.toml --output wallet-balances.txt
```

Funder:
```powershell
$env:WALLET_PASSWORD="password"; cargo run -p sepolia-overlayer --bin sepolia-funder -- --config chains/sepolia-overlayer/config.toml
```

## 6) Robinhood (`robinhood-spammer`)

Main:
```powershell
cargo run -p robinhood-spammer --bin robinhood-spammer -- --config chains/robinhood/config.toml
```

Debug:
```powershell
cargo run -p robinhood-spammer --bin robinhood-debug -- --config chains/robinhood/config.toml
```

Runner:
```powershell
cargo run -p robinhood-spammer --bin robinhood-runner -- --config chains/robinhood/config.toml
```

## 7) Tempo (`tempo-spammer`)

Main:
```powershell
cargo run -p tempo-spammer --bin tempo-spammer -- --config chains/tempo-spammer/config/config.toml
```

Debug/runner variants:
```powershell
cargo run -p tempo-spammer --bin tempo-debug -- --config chains/tempo-spammer/config/config.toml
cargo run -p tempo-spammer --bin tempo-runner -- --config chains/tempo-spammer/config/config.toml
cargo run -p tempo-spammer --bin tempo-sequence -- --config chains/tempo-spammer/config/config.toml
cargo run -p tempo-spammer --bin debug_proxy -- --config chains/tempo-spammer/config/config.toml
cargo run -p tempo-spammer --bin wallet-check -- --config chains/tempo-spammer/config/config.toml
```

Alternative:
- Build once, run `target\debug\tempo-*.exe` directly.

## 8) Useful Environment Variables

```powershell
$env:WALLET_PASSWORD="your_password"
$env:RUST_BACKTRACE=1
$env:RUST_LOG="debug"
```

## 9) Quick Troubleshooting

1. Wallet decrypt fails:
- Verify `WALLET_PASSWORD`.
- Try `debug_task --all` first.

2. Build/file-lock issues:
- Use `._clean_and_compile_all.bat`.
- Close lingering `cargo`, `rustc`, or running binaries.

3. DB lock issues:
- Stop other processes touching `rise.db` / `tempo-spammer.db`.
- Retry with fewer workers.

4. RPC/proxy instability:
- Lower worker count / TPS / semaphores.
- Retry without proxies to isolate network issues.

## 10) MCP Quick Start (Smoke Test)

Goal: verify MCP can access this repo and run at least one external-tool path.

1. Filesystem MCP check:
- list allowed dirs and confirm `C:\My Script\testnet-framework` exists.
- list repo directory via MCP.

2. Context-mode MCP check:
- run with explicit repo cwd:
```bash
cd "C:\My Script\testnet-framework" && pwd && ls AGENTS.md Cargo.toml
```

3. Tavily MCP check:
- run `tavily_search` with a simple query.
- optionally run `tavily_extract` on `https://example.com`.

4. Composio MCP check:
- run `COMPOSIO_SEARCH_TOOLS` first.
- reuse returned `session_id`.
- execute one safe read-only tool (example: GitHub list repos).

Expected known caveats in this environment:
- `context-mode` URL fetch can fail TLS certificate chain.
- `tavily_map` / `tavily_crawl` may fail with invalid start URL.

Fallback alternatives:
- if context fetch fails TLS -> use `tavily_extract`.
- if tavily map/crawl fails -> use `tavily_search` + `tavily_extract`.
