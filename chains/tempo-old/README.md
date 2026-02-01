# Tempo Chain Project

This project contains the automation tasks and logic for the Tempo testnet, built using the `core-logic` framework.

## Structure

- `src/bin/tempo-project.rs`: The main executable that runs the spammer/task runner.
- `src/bin/debug_task.rs`: A CLI tool for debugging individual tasks and checking wallet states.
- `src/tasks/`: Contains the specific task implementations (Faucet, Transfer, etc.) for Tempo.
- `src/utils/`: Tempo-specific utilities (Gas management, Nonce management, etc.).


we use tempo.db as our database for wallet tracking:
task_metrics: Logs every task execution.
id: INTEGER PRIMARY KEY
worker_id: TEXT (which thread ran it)
wallet_address: TEXT (who ran it)
task_name: TEXT (what ran)
status: TEXT ("SUCCESS" or "FAILED")
message: TEXT (error or success msg)
duration_ms: INTEGER
timestamp: INTEGER
created_assets: Tracks created tokens (Stablecoins, Memecoins).
id: INTEGER PRIMARY KEY
wallet_address: TEXT (Creator)
asset_address: TEXT (Contract Address)
asset_type: TEXT (e.g., "Stablecoin", "Meme")
name: TEXT
symbol: TEXT
timestamp: INTEGER
created_counter_contracts: Tracks deployed dummy contracts (Task 01).
contract_address: TEXT
chain_id: INTEGER
proxy_stats: Tracks proxy reliability.
proxy_url: TEXT
success_count: INTEGER
fail_count: INTEGER



## Usage

### Running the Spammer
To start the automated task runner:
```bash
cargo run --bin tempo-project
```

### Debugging Tasks
To interactively debug or run specific tasks:
```bash
cargo run --bin debug_task
```
You can select a specific wallet index using the interactive prompt.

## Configuration
Ensure your `wallet-json` directory is populated in the root workspace folder.
