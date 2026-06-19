@echo off
TITLE OVERLAYER
cd /d "%~dp0"

cargo run --release -p sepolia-overlayer --bin wallet-balance-dump -- --config chains/sepolia-overlayer/config.toml

:: Preserves operational clarity by holding the terminal open on exit
pause