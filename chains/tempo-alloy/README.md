# tempo-alloy

Tempo blockchain client migrated to use Alloy v1.0 instead of ethers-rs.

## Overview

This is a complete rewrite of the Tempo off-chain tools using:
- **Alloy v1.0** - Modern Ethereum library with 10x faster ABI encoding
- **sol! macro** - Compile-time contract binding generation
- **AnyNetwork** - Support for Tempo's custom transaction types

## Structure

```
tempo-alloy/
├── apps/
│   ├── cli/              # Main CLI application (tempo-project)
│   └── debugger/         # Debug tool
├── crates/
│   ├── client/           # Alloy-based client library
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── provider.rs
│   │   │   ├── tasks/    # Task implementations
│   │   │   └── utils/
│   │   └── Cargo.toml
│   ├── protocol/         # Tempo protocol types
│   └── evm/              # EVM configuration
├── config/               # Configuration files
├── contracts/            # Solidity contracts
└── Cargo.toml           # Workspace configuration
```

## Building

```bash
# Build all crates
cargo build --workspace

# Build specific binary
cargo build -p tempo-cli
```

## Running

```bash
# Run spammer with default config
cargo run -p tempo-cli

# Run specific task
cargo run -p tempo-cli -- run --task 01_deploy_contract

# List available tasks
cargo run -p tempo-cli -- list
```

## Configuration

Edit `config/config.toml`:

```toml
rpc_url = "https://rpc.moderato.tempo.xyz"
chain_id = 42431
worker_count = 1
```

Set wallet password:
```bash
export WALLET_PASSWORD=your_password
```

## Tasks

| ID | Name | Description |
|----|------|-------------|
| 01 | deploy_contract | Deploy a Solidity contract |
| 02 | claim_faucet | Claim tokens from Tempo faucet |
| 03 | send_token | Send TIP-20 tokens |
| 04 | create_stable | Create a new stablecoin |
| 05 | swap_stable | Swap on Tempo DEX |

## Migration from ethers-rs

Key changes from the original tempo codebase:

1. **Provider**:
   ```rust
   // Old (ethers)
   let provider = Provider::<Http>::try_from(url)?;
   let wallet = private_key.parse::<LocalWallet>()?;
   let client = SignerMiddleware::new(provider, wallet);
   
   // New (alloy)
   let signer: PrivateKeySigner = private_key.parse()?;
   let provider = ProviderBuilder::new()
       .with_recommended_fillers()
       .wallet(signer)
       .connect_http(url)?;
   ```

2. **Contract ABI**:
   ```rust
   // Old (ethers abigen!)
   ethers::contract::abigen!(IToken, "[...]");
   
   // New (alloy sol!)
   sol!(IToken, r#"[...]"#);
   ```

3. **Transaction Building**:
   ```rust
   // Old (ethers)
   let tx = TransactionRequest::new()
       .to(address)
       .value(amount);
   
   // New (alloy)
   let tx = TransactionRequest::default()
       .with_to(address)
       .with_value(amount);
   ```

## Performance

Alloy provides significant performance improvements:
- **10x faster** ABI encoding/decoding
- **60% faster** U256 operations
- **Better async** patterns with modern Rust
