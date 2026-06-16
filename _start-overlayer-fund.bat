@echo off
TITLE Tempo Spammer
cd /d "%~dp0"

cargo run --release -p sepolia-overlayer --bin sepolia-funder -- --workers 20 --min-target 0.15 --max-target 0.2 --min-gwei 1.5 --max-gwei 8

:: Preserves operational clarity by holding the terminal open on exit
pause