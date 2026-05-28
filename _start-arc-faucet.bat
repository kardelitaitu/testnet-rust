@echo off
TITLE Arc Faucet
cd /d "%~dp0"

:: Request USDC (default)
cargo run --release -p arc-project --bin arc-faucet -- --address %1 --token usdc

:: Preserves operational clarity by holding the terminal open on exit
pause
