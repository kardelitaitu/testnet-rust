@echo off
TITLE Arc Testnet Spammer
cd /d "%~dp0"

cargo run --release -p arc-project -- --config chains/arc/config.toml

:: Preserves operational clarity by holding the terminal open on exit
pause
