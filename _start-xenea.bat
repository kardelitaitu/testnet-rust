@echo off
cd "C:\My Script\testnet-framework"
cargo run --release -p xenea-project -- --config chains/xenea/config.toml
