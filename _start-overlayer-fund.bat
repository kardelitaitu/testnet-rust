@echo off
TITLE Tempo Spammer
cd /d "%~dp0"

cargo run --release -p sepolia-overlayer --bin sepolia-funder -- --workers 34 --min-target 0.07 --max-target 0.11

:: Preserves operational clarity by holding the terminal open on exit
pause