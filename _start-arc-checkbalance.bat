@echo off
TITLE Arc Balance Dump
cd /d "%~dp0"

cargo run --release -p arc-project --bin arc-balance-dump -- --config chains/arc/config.toml

:: Preserves operational clarity by holding the terminal open on exit
pause
