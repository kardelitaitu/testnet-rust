@echo off
TITLE Tempo Spammer
cd /d "%~dp0"

cargo run --release -p sepolia-overlayer --bin sepolia-daily -- --config chains/sepolia-overlayer/config.toml --base-config chains/sepolia-overlayer/config-base.toml --workers 50 --db-path sepolia-overlayer-daily.db
:: Preserves operational clarity by holding the terminal open on exit
pause