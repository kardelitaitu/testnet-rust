@echo off
TITLE Tempo Spammer
cd /d "%~dp0"

cargo run --release -p sepolia-overlayer --bin sepolia-funder -- --workers 4 --min-target 0.2 --max-target 0.3 --min-gwei 3 --max-gwei 32

:: Preserves operational clarity by holding the terminal open on exit
pause